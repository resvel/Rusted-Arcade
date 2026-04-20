use arcade_domain::N64CpuCoreMode;
use arcade_services::LaunchPlan;
use tracing::{info, warn};

use crate::app::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(crate) fn launch_selected_rom(&mut self) {
        let Some(rom_id) = self.state.selection.selected_rom_id_cloned() else {
            self.state.play.set_status("Select a ROM first.");
            return;
        };

        match self.services.prepare_launch(&rom_id) {
            Ok(plan) => {
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
                            fallback_emulation.n64.cpu_core_mode =
                                N64CpuCoreMode::CachedInterpreter;
                            self.host.update_core_variables(&fallback_emulation);

                            self.host
                                .load_for_rom(
                                    &plan.system,
                                    plan.effective_core.as_deref(),
                                    &plan.rom_path,
                                )
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
                            format!(
                                "{} (dynarec failed to boot; running Stable Cached lane)",
                                plan.status_message
                            )
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
                        self.state.play.begin_session(
                            status_message,
                            plan.rom_id,
                            plan.system,
                            core_name,
                        );
                        self.assets.last_frame_texture = None;
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
                        let status = if let Some(primary_err) = dynarec_error {
                            format!(
                                "Failed to start core: dynarec attempt failed ({primary_err}); cached fallback failed ({err})"
                            )
                        } else {
                            format!("Failed to start core: {err}")
                        };
                        self.state.play.set_status(status);
                    }
                }
            }
            Err(err) => {
                warn!(rom_id = %rom_id, error = %err, "Could not prepare play session");
                self.state
                    .play
                    .set_status(format!("Could not start play session: {err}"));
            }
        }
    }
}

fn should_retry_n64_with_cached_fallback(plan: &LaunchPlan, cpu_core_mode: N64CpuCoreMode) -> bool {
    cpu_core_mode == N64CpuCoreMode::DynamicRecompiler
        && plan.system.eq_ignore_ascii_case("N64")
        && plan
            .resolved_core_name
            .eq_ignore_ascii_case("mupen64plus_next")
}
