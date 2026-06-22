use arcade_domain::N64CpuCoreMode;
use arcade_domain::RomCard;
use arcade_services::LaunchPlan;
use eframe::egui;
use tracing::{info, warn};

use crate::app::NativeArcadeUiApp;
use crate::state::PlayLaunchPhase;

impl NativeArcadeUiApp {
    pub(crate) fn launch_selected_rom(&mut self) {
        let Some(rom_id) = self.state.selection.selected_rom_id_cloned() else {
            self.state.play.set_status("Select a ROM first.");
            return;
        };
        self.launch_rom_id(&rom_id, None);
    }

    pub(crate) fn launch_rom_with_core_override(&mut self, rom_id: &str, core: &str) {
        info!(rom_id = %rom_id, requested_core = %core, "Retrying launch with alternate core");
        self.launch_rom_id(rom_id, Some(core));
    }

    fn launch_rom_id(&mut self, rom_id: &str, core_override: Option<&str>) {
        let selected_rom = self.current_selected_rom().cloned();
        let attempted_core = core_override.map(str::to_owned);

        let launch_result = if let Some(core) = core_override {
            self.services
                .prepare_launch_with_core_override(rom_id, core)
        } else {
            self.services.prepare_launch(rom_id)
        };

        match launch_result {
            Ok(plan) => {
                self.state.play.queue_launch(plan, self.state.current_view);
                self.assets.last_frame_texture = None;
                self.assets.last_gl_texture_frame = None;
                #[cfg(target_os = "macos")]
                {
                    self.assets.last_macos_iosurface_frame = None;
                }
            }
            Err(err) => {
                warn!(rom_id = %rom_id, requested_core = attempted_core.as_deref().unwrap_or("auto"), error = %err, "Could not prepare play session");
                self.assets.last_frame_texture = None;
                self.assets.last_gl_texture_frame = None;
                #[cfg(target_os = "macos")]
                {
                    self.assets.last_macos_iosurface_frame = None;
                }
                self.fail_prepared_launch(selected_rom.as_ref(), attempted_core, err.to_string());
            }
        }
    }

    pub(crate) fn continue_pending_launch(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.state.play.take_ready_pending_launch() else {
            return;
        };

        self.state
            .play
            .set_launch_phase(PlayLaunchPhase::LoadingCore);
        self.state.play.launch_friendly_message = Some(String::from("Starting..."));
        ctx.request_repaint();

        let plan = pending.plan;
        let launch_view = pending.launch_view;
        self.host
            .set_external_vulkan_window_title(format!("Arcade - {}", plan.display_title));

        let active_config = self.services.config();
        self.host.update_core_variables(&active_config.emulation);

        let n64_cpu_core_mode = self.services.n64_cpu_core_mode();
        let should_retry_with_cached =
            should_retry_n64_with_cached_fallback(&plan, n64_cpu_core_mode);
        let mut dynarec_error: Option<String> = None;

        let load_result = if should_retry_with_cached {
            match self.host.load_for_rom(
                &plan.system,
                plan.effective_core.as_deref(),
                &plan.rom_path,
            ) {
                Ok(core_name) => Ok((core_name, false)),
                Err(primary_err) => {
                    let primary_error_text = primary_err.to_string();
                    dynarec_error = Some(primary_error_text.clone());
                    warn!(
                        system = %plan.system,
                        rom_id = %plan.rom_id,
                        rom_path = %plan.rom_path.display(),
                        requested_core = %plan.effective_core.as_deref().unwrap_or("auto"),
                        error = %primary_error_text,
                        "N64 dynarec launch failed; retrying with cached interpreter"
                    );

                    let mut fallback_emulation = active_config.emulation.clone();
                    fallback_emulation.n64.cpu_core_mode = N64CpuCoreMode::CachedInterpreter;
                    self.host.update_core_variables(&fallback_emulation);

                    self.host
                        .load_for_rom(&plan.system, plan.effective_core.as_deref(), &plan.rom_path)
                        .map(|core_name| (core_name, true))
                }
            }
        } else {
            self.host
                .load_for_rom(&plan.system, plan.effective_core.as_deref(), &plan.rom_path)
                .map(|core_name| (core_name, false))
        };

        match load_result {
            Ok((core_name, used_cached_fallback)) => {
                let status_message = if used_cached_fallback {
                    format!("{} (using stable cached lane)", plan.status_message)
                } else {
                    plan.status_message.clone()
                };
                info!(
                    system = %plan.system,
                    rom_id = %plan.rom_id,
                    rom_path = %plan.rom_path.display(),
                    core = %core_name,
                    used_cached_fallback,
                    "Started play session"
                );
                self.state.play.launch_note_message = used_cached_fallback
                    .then_some(String::from(
                        "Using the stable cached lane for this launch.",
                    ))
                    .or_else(|| plan.active_core_note.map(String::from));
                self.state.play.begin_session(
                    status_message,
                    plan.rom_id.clone(),
                    plan.system.clone(),
                    core_name.clone(),
                    launch_view,
                );
                if let Some(promote_core) = plan.promote_core_on_success.clone() {
                    match self.services.persist_successful_retry_core(&plan) {
                        Ok(true) => {
                            self.state.play.set_feedback(
                                std::time::Instant::now(),
                                std::time::Duration::from_secs(3),
                                format!("Saved {} for this game", display_core_name(&promote_core)),
                            );
                        }
                        Ok(false) => {}
                        Err(err) => {
                            self.state.play.set_feedback(
                                std::time::Instant::now(),
                                std::time::Duration::from_secs(4),
                                format!("Couldn’t save core preference: {err}"),
                            );
                        }
                    }
                    self.state
                        .play
                        .offer_core_promotion(plan.rom_id, promote_core);
                }
                self.start_play_runner_for_loaded_session(ctx, &core_name);
                self.show_play_bar();
            }
            Err(err) => {
                warn!(
                    system = %plan.system,
                    rom_id = %plan.rom_id,
                    rom_path = %plan.rom_path.display(),
                    requested_core = %plan.effective_core.as_deref().unwrap_or("auto"),
                    error = %err,
                    dynarec_attempt_error = dynarec_error.as_deref().unwrap_or("none"),
                    "Failed to start play session"
                );
                if !self.host.is_loaded() {
                    self.state.play.clear_active_launch();
                }
                let detail = if let Some(primary_err) = dynarec_error {
                    format!("Dynarec attempt failed: {primary_err}\nCached fallback failed: {err}")
                } else {
                    err.to_string()
                };
                self.state.play.fail_launch(
                    Some(plan.display_title.clone()),
                    Some(plan.system.clone()),
                    Some(plan.resolved_core_name.clone()),
                    plan.cover_path.clone(),
                    plan.preview_poster_path.clone(),
                    friendly_launch_error(&detail),
                    detail,
                );
                self.state.play.launch_rom_id = Some(plan.rom_id);
            }
        }
        ctx.request_repaint();
    }

    fn fail_prepared_launch(
        &mut self,
        rom: Option<&RomCard>,
        attempted_core: Option<String>,
        detail: String,
    ) {
        let friendly = friendly_launch_error(&detail);
        let failed_core =
            attempted_core.or_else(|| rom.and_then(|rom| rom.rom.emulator_core.clone()));
        self.state.play.fail_launch(
            rom.map(|rom| rom.display_title.clone()),
            rom.map(|rom| rom.rom.system.clone()),
            failed_core,
            rom.and_then(|rom| rom.rom.cover_path.clone()),
            rom.and_then(|rom| rom.rom.preview_poster_path.clone()),
            friendly,
            detail,
        );
        self.state.play.launch_rom_id = rom.map(|rom| rom.rom.id.clone());
    }
}

fn should_retry_n64_with_cached_fallback(plan: &LaunchPlan, cpu_core_mode: N64CpuCoreMode) -> bool {
    cpu_core_mode == N64CpuCoreMode::DynamicRecompiler
        && plan.system.eq_ignore_ascii_case("N64")
        && plan
            .resolved_core_name
            .eq_ignore_ascii_case("mupen64plus_next")
}

fn display_core_name(core: &str) -> &'static str {
    match core {
        "fbneo" => "FBNeo",
        "mame2003" => "MAME2003",
        "mame2003_plus" => "MAME2003 Plus",
        _ => "Selected Core",
    }
}

fn friendly_launch_error(detail: &str) -> String {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("bios") {
        String::from("Missing required BIOS files.")
    } else if lower.contains("rom file not found") || lower.contains("not found") {
        String::from("Couldn’t find this game file.")
    } else {
        String::from("Couldn’t start this game.")
    }
}
