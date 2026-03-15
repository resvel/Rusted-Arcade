use eframe::egui;
use tracing::{info, warn};

use arcade_services::SaveOperationError;

use crate::{app::NativeArcadeUiApp, state::HoldAction};

impl NativeArcadeUiApp {
    const PLAY_OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_millis(1800);
    const PLAY_BAR_DURATION: std::time::Duration = std::time::Duration::from_secs(2);
    const PLAY_RETURN_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);
    const PLAY_RESET_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);

    pub(crate) fn tick_play_session(&mut self, ctx: &egui::Context) {
        const MAX_STALE_FRAMES: u32 = 3;
        const ARCADE_MAX_CATCH_UP_FRAMES: u32 = 2;
        const ARCADE_MAX_FRAME_BUDGET: f64 = 2.5;
        const ARCADE_MAX_REPAINT_WAIT: std::time::Duration = std::time::Duration::from_millis(1);

        if !self.host.is_loaded() {
            return;
        }

        self.apply_keyboard_and_gamepad_input(ctx);
        if !self.host.is_loaded() {
            // Controller-triggered exits can unload mid-frame without any new egui input event.
            // Force one more frame so the shell re-renders the prior view immediately.
            ctx.request_repaint();
            return;
        }
        let frame_interval = self
            .host
            .frame_interval()
            .unwrap_or_else(|| std::time::Duration::from_secs_f64(1.0 / 60.0));
        let now = std::time::Instant::now();
        let mut elapsed = now.duration_since(self.state.play.last_frame_run_at);
        let max_stale_interval = frame_interval.saturating_mul(MAX_STALE_FRAMES);
        if elapsed > max_stale_interval {
            self.reset_play_clock();
            elapsed = frame_interval;
        }
        let parallel_n64_target_pacing = self
            .state
            .play
            .active_core
            .as_deref()
            .is_some_and(|core| core.eq_ignore_ascii_case("parallel_n64"));
        let arcade_low_latency_pacing = self
            .state
            .play
            .active_system
            .as_deref()
            .is_some_and(|system| system.eq_ignore_ascii_case("ARCADE"));
        if parallel_n64_target_pacing {
            let frame_budget = self.state.play.catch_up_frame_debt
                + (elapsed.as_secs_f64() / frame_interval.as_secs_f64());
            if frame_budget < 1.0 {
                let wait = std::time::Duration::from_secs_f64(
                    frame_interval.as_secs_f64() * (1.0 - frame_budget),
                );
                ctx.request_repaint_after(wait);
                return;
            }
        } else if elapsed < frame_interval {
            if arcade_low_latency_pacing {
                // On macOS this avoids timer jitter that can land ARCADE cores in a ~30-40 Hz
                // cadence despite low frame work time.
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(frame_interval - elapsed);
            }
            return;
        }

        let disable_catch_up = self
            .state
            .play
            .active_system
            .as_deref()
            .is_some_and(|system| system.eq_ignore_ascii_case("N64"))
            && !self
                .state
                .play
                .active_core
                .as_deref()
                .is_some_and(|core| core.eq_ignore_ascii_case("parallel_n64"));
        let frames_to_run = if disable_catch_up {
            self.state.play.set_last_frame_run_at(now);
            1
        } else if parallel_n64_target_pacing {
            let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
            let frame_budget =
                (self.state.play.catch_up_frame_debt + elapsed_frames).clamp(1.0, 3.0);
            let frames_to_run = frame_budget.floor().clamp(1.0, 2.0) as u32;
            self.state.play.set_last_frame_run_at(now);
            self.state.play.catch_up_frame_debt =
                (frame_budget - frames_to_run as f64).clamp(0.0, 1.0);
            frames_to_run
        } else if arcade_low_latency_pacing {
            let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
            let frame_budget = (self.state.play.catch_up_frame_debt + elapsed_frames)
                .clamp(1.0, ARCADE_MAX_FRAME_BUDGET);
            let frames_to_run = frame_budget
                .floor()
                .clamp(1.0, ARCADE_MAX_CATCH_UP_FRAMES as f64)
                as u32;
            self.state.play.set_last_frame_run_at(now);
            self.state.play.catch_up_frame_debt =
                (frame_budget - frames_to_run as f64).clamp(0.0, 1.0);
            frames_to_run
        } else {
            const MAX_CATCH_UP_FRAMES: u32 = 2;
            let frames_to_run = ((elapsed.as_secs_f64() / frame_interval.as_secs_f64()).floor()
                as u32)
                .clamp(1, MAX_CATCH_UP_FRAMES);
            let advance = frame_interval.saturating_mul(frames_to_run);
            let leftover = elapsed.saturating_sub(advance);
            self.state
                .play
                .set_last_frame_run_at(now.checked_sub(leftover).unwrap_or(now));
            frames_to_run
        };

        let mut latest_frame = None;
        let tick_started_at = std::time::Instant::now();
        let mut frames_executed = 0;
        for _ in 0..frames_to_run {
            frames_executed += 1;
            match self.host.run_frame() {
                Ok(Some(frame)) => latest_frame = Some(frame),
                Ok(None) => {}
                Err(err) => {
                    warn!("frame loop failed: {err}");
                    break;
                }
            }
        }
        if let Some(frame) = latest_frame {
            self.update_frame_texture(ctx, frame);
        }
        let tick_work = tick_started_at.elapsed();
        if let Some(sample) = self
            .state
            .play
            .record_perf_tick(elapsed, tick_work, frames_executed)
        {
            info!(
                target: "arcade_ui::perf",
                "play_tick system={} core={} ticks={} tick_hz={:.1} avg_gap_ms={:.2} avg_work_ms={:.2} avg_frames_per_tick={:.2}",
                self.state.play.active_system.as_deref().unwrap_or("unknown"),
                self.state.play.active_core.as_deref().unwrap_or("unknown"),
                sample.ticks,
                sample.tick_hz,
                sample.avg_gap_ms,
                sample.avg_work_ms,
                sample.avg_frames_per_tick,
            );
        }

        if parallel_n64_target_pacing {
            // If emulation work already exceeded the target frame interval, request
            // immediate repaint to avoid adding extra sleep and compounding lag.
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < frame_interval {
                ctx.request_repaint_after(frame_interval - post_tick_elapsed);
            } else {
                self.state.play.catch_up_frame_debt = 0.0;
                ctx.request_repaint();
            }
        } else if arcade_low_latency_pacing {
            let target_interval = frame_interval.saturating_mul(frames_executed.max(1));
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                let remaining = target_interval - post_tick_elapsed;
                ctx.request_repaint_after(remaining.min(ARCADE_MAX_REPAINT_WAIT));
            } else {
                ctx.request_repaint();
            }
        } else {
            let target_interval = frame_interval.saturating_mul(frames_executed.max(1));
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                ctx.request_repaint_after(target_interval - post_tick_elapsed);
            } else {
                ctx.request_repaint();
            }
        }
    }

    pub(crate) fn reset_play_session(&mut self) {
        if let Err(err) = self.host.reset() {
            self.state.play.set_status(format!("reset failed: {err}"));
        } else {
            self.reset_play_clock();
        }
    }

    pub(crate) fn stop_play_session(&mut self) {
        self.state.play.reset_frontend_shortcut_latches();
        if let Err(err) = self.host.unload() {
            self.state.play.set_status(format!("stop failed: {err}"));
        } else {
            self.state.play.clear_session();
            self.assets.last_frame_texture = None;
            self.reset_play_clock();
        }
    }

    pub(crate) fn quick_save_play_session(&mut self) {
        let Some(rom_id) = self.state.play.active_rom_id.clone() else {
            self.set_play_feedback("No active ROM loaded.");
            return;
        };
        match self.host.serialize_state() {
            Ok(Some(bytes)) => match self.services.save_save_slot(
                &rom_id,
                self.state.play.save_slot,
                Some("Native quick save"),
                None,
                &bytes,
            ) {
                Ok(summary) => {
                    self.set_play_feedback(format!(
                        "Saved slot {} ({} bytes, v={})",
                        summary.slot,
                        summary.size_bytes,
                        summary.version.unwrap_or_default()
                    ));
                }
                Err(SaveOperationError::VersionConflict(conflict)) => {
                    self.set_play_feedback(format!(
                        "Save conflict on slot {} (current version {:?})",
                        conflict.slot, conflict.version
                    ));
                }
                Err(SaveOperationError::SlotLimitExceeded) => {
                    self.set_play_feedback("Save rejected: per-slot limit exceeded.");
                }
                Err(SaveOperationError::ProfileLimitExceeded) => {
                    self.set_play_feedback("Save rejected: local storage limit exceeded.");
                }
                Err(SaveOperationError::Other(err)) => {
                    self.set_play_feedback(format!("Save failed: {err}"));
                }
            },
            Ok(None) => {
                self.set_play_feedback("Core does not provide serialize state data.");
            }
            Err(err) => {
                self.set_play_feedback(format!("Serialize failed: {err}"));
            }
        }
    }

    pub(crate) fn quick_load_play_session(&mut self) {
        let Some(rom_id) = self.state.play.active_rom_id.clone() else {
            self.set_play_feedback("No active ROM loaded.");
            return;
        };

        match self
            .services
            .load_save_slot(&rom_id, self.state.play.save_slot)
        {
            Ok(Some(slot_data)) => match self.host.unserialize_state(&slot_data.bytes) {
                Ok(true) => {
                    self.reset_play_clock();
                    self.set_play_feedback(format!(
                        "Loaded slot {} (version {})",
                        slot_data.slot, slot_data.version
                    ));
                }
                Ok(false) => {
                    self.set_play_feedback("Core rejected unserialize payload.");
                }
                Err(err) => {
                    self.set_play_feedback(format!("Unserialize failed: {err}"));
                }
            },
            Ok(None) => {
                self.set_play_feedback("Slot is empty.");
            }
            Err(err) => {
                self.set_play_feedback(format!("Could not load slot: {err}"));
            }
        }
    }

    pub(crate) fn cycle_save_slot_forward(&mut self) {
        let slot = self
            .state
            .play
            .advance_save_slot(self.services.save_limits().slot_count);
        self.set_play_feedback(format!("Selected save slot {slot}."));
    }

    pub(crate) fn show_play_bar(&mut self) {
        self.state
            .play
            .show_bar(std::time::Instant::now(), Self::PLAY_BAR_DURATION);
    }

    pub(crate) fn keep_play_bar_visible(&mut self) {
        self.show_play_bar();
    }

    pub(crate) fn play_bar_visible(&self) -> bool {
        self.state.play.bar_visible(std::time::Instant::now())
    }

    pub(crate) fn handle_session_return_shortcut(&mut self, return_pressed: bool) {
        let now = std::time::Instant::now();
        let action = self.state.play.handle_return_input(
            now,
            return_pressed,
            Self::PLAY_RETURN_HOLD_DURATION,
        );

        match action {
            HoldAction::None => {}
            HoldAction::Started => {
                self.set_play_feedback("Hold Exit/Esc to exit.");
            }
            HoldAction::Triggered => {
                self.stop_play_session();
            }
        }
    }

    pub(crate) fn handle_session_reset_shortcut(&mut self, reset_pressed: bool) {
        let now = std::time::Instant::now();
        let action =
            self.state
                .play
                .handle_reset_input(now, reset_pressed, Self::PLAY_RESET_HOLD_DURATION);

        match action {
            HoldAction::None => {}
            HoldAction::Started => {
                self.set_play_feedback("Hold Reset to restart the game.");
            }
            HoldAction::Triggered => {
                self.reset_play_session();
            }
        }
    }

    pub(crate) fn reset_play_clock(&mut self) {
        self.state.play.reset_clock();
    }

    pub(crate) fn active_play_overlay(&mut self) -> Option<String> {
        self.state.play.active_overlay(std::time::Instant::now())
    }

    fn set_play_feedback(&mut self, message: impl Into<String>) {
        self.state.play.set_feedback(
            std::time::Instant::now(),
            Self::PLAY_OVERLAY_DURATION,
            message,
        );
    }
}
