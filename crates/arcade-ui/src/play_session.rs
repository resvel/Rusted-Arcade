use eframe::egui;
use tracing::{info, warn};

use arcade_libretro::AudioQueueSnapshot;
use arcade_services::SaveOperationError;
use std::sync::OnceLock;

use crate::{
    app::NativeArcadeUiApp,
    state::{HoldAction, MenuFocusRegion},
};

const AUDIO_MASTER_DRAIN_WATERMARK: f64 = 1.25;
const AUDIO_MASTER_REFILL_WATERMARK: f64 = 0.85;

fn audio_master_frames_to_run(snapshot: Option<AudioQueueSnapshot>) -> Option<u32> {
    let snapshot = snapshot?;
    if snapshot.target_frames == 0 {
        return None;
    }

    let queue_frames = snapshot.queue_frames as f64;
    let target_frames = snapshot.target_frames as f64;
    if queue_frames >= target_frames * AUDIO_MASTER_DRAIN_WATERMARK {
        Some(0)
    } else if queue_frames <= target_frames * AUDIO_MASTER_REFILL_WATERMARK {
        Some(2)
    } else {
        Some(1)
    }
}

fn is_audio_master_pacing_core(core_name: &str) -> bool {
    core_name.eq_ignore_ascii_case("flycast")
        || core_name.eq_ignore_ascii_case("mupen64plus_next")
        || core_name.eq_ignore_ascii_case("mednafen_psx_hw")
        || core_name.eq_ignore_ascii_case("pcsx2")
        || core_name.eq_ignore_ascii_case("play")
}

impl NativeArcadeUiApp {
    const PLAY_OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_millis(1800);
    const PLAY_BAR_DURATION: std::time::Duration = std::time::Duration::from_secs(2);
    const PLAY_RETURN_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);
    const PLAY_RESET_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);

    pub(crate) fn tick_play_session(&mut self, ctx: &egui::Context) {
        const MAX_STALE_FRAMES: u32 = 3;
        const ARCADE_MAX_CATCH_UP_FRAMES: u32 = 2;
        const ARCADE_MAX_FRAME_BUDGET: f64 = 2.5;
        const DREAMCAST_MAX_CATCH_UP_FRAMES: u32 = 1;
        const DREAMCAST_MAX_FRAME_BUDGET: f64 = 1.35;
        const LOW_LATENCY_MAX_REPAINT_WAIT: std::time::Duration =
            std::time::Duration::from_millis(1);

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
        let active_core = self.state.play.active_core.as_deref();
        let n64_target_pacing =
            active_core.is_some_and(|core| core.eq_ignore_ascii_case("mupen64plus_next"));
        let arcade_low_latency_pacing = self
            .state
            .play
            .active_system
            .as_deref()
            .is_some_and(|system| system.eq_ignore_ascii_case("ARCADE"));
        let dreamcast_low_latency_pacing =
            active_core.is_some_and(|core| core.eq_ignore_ascii_case("flycast"));
        let low_latency_pacing = arcade_low_latency_pacing || dreamcast_low_latency_pacing;
        let play_tight_pacing = active_core.is_some_and(|core| core.eq_ignore_ascii_case("play"));
        let audio_master_frames = audio_master_frames_to_run(
            active_core
                .is_some_and(is_audio_master_pacing_core)
                .then(|| self.host.audio_queue_snapshot())
                .flatten(),
        );
        if audio_master_frames.is_some() {
            // Audio-master pacing derives frame budget from queue occupancy,
            // not wall-clock elapsed time.
        } else if n64_target_pacing {
            let frame_budget = self.state.play.catch_up_frame_debt
                + (elapsed.as_secs_f64() / frame_interval.as_secs_f64());
            if frame_budget < 1.0 {
                ctx.request_repaint();
                return;
            }
        } else if play_tight_pacing {
            if elapsed < frame_interval {
                let remaining = frame_interval - elapsed;
                ctx.request_repaint_after(remaining);
                return;
            }
        } else if elapsed < frame_interval {
            if low_latency_pacing {
                // Immediate repaint avoids timer jitter that can degrade effective cadence
                // despite low frame work time.
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(frame_interval - elapsed);
            }
            return;
        }

        let frames_to_run = if let Some(frames_to_run) = audio_master_frames {
            self.state.play.set_last_frame_run_at(now);
            self.state.play.catch_up_frame_debt = 0.0;
            frames_to_run
        } else if n64_target_pacing {
            let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
            let frame_budget =
                (self.state.play.catch_up_frame_debt + elapsed_frames).clamp(1.0, 3.0);
            let smooth_pacing = n64_smooth_pacing_enabled();
            let max_frames = if smooth_pacing {
                // Favor even pacing by default; only allow burst catch-up when we're
                // substantially behind after a long stall.
                if frame_budget >= 2.5 {
                    2.0
                } else {
                    1.0
                }
            } else {
                2.0
            };
            let frames_to_run = frame_budget.floor().clamp(1.0, max_frames) as u32;
            self.state.play.set_last_frame_run_at(now);
            let debt_cap = if smooth_pacing { 0.75 } else { 1.0 };
            self.state.play.catch_up_frame_debt =
                (frame_budget - frames_to_run as f64).clamp(0.0, debt_cap);
            frames_to_run
        } else if play_tight_pacing {
            let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
            let frame_budget =
                (self.state.play.catch_up_frame_debt + elapsed_frames).clamp(1.0, 1.35);
            // Keep Play pacing strict (single-step) to avoid burst catch-up that can make
            // FMV sequences appear speed-shifted during timing jitter.
            let frames_to_run = 1;
            self.state.play.set_last_frame_run_at(now);
            self.state.play.catch_up_frame_debt =
                (frame_budget - frames_to_run as f64).clamp(0.0, 0.35);
            frames_to_run
        } else if low_latency_pacing {
            let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
            let (max_frame_budget, max_catch_up_frames) = if dreamcast_low_latency_pacing {
                (DREAMCAST_MAX_FRAME_BUDGET, DREAMCAST_MAX_CATCH_UP_FRAMES)
            } else {
                (ARCADE_MAX_FRAME_BUDGET, ARCADE_MAX_CATCH_UP_FRAMES)
            };
            let frame_budget =
                (self.state.play.catch_up_frame_debt + elapsed_frames).clamp(1.0, max_frame_budget);
            let frames_to_run = frame_budget.floor().clamp(1.0, max_catch_up_frames as f64) as u32;
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
        let mut terminal_frame_error = None;
        for _ in 0..frames_to_run {
            frames_executed += 1;
            match self.host.run_frame() {
                Ok(Some(frame)) => latest_frame = Some(frame),
                Ok(None) => {}
                Err(err) => {
                    let message = err.to_string();
                    warn!("frame loop failed: {message}");
                    terminal_frame_error = Some(message);
                    break;
                }
            }
        }
        if let Some(message) = terminal_frame_error {
            self.state.play.set_status(message);
            self.stop_play_session();
            ctx.request_repaint();
            return;
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
            let target_fps = if frame_interval.as_nanos() > 0 {
                1.0 / frame_interval.as_secs_f64()
            } else {
                0.0
            };
            let effective_run_fps = sample.tick_hz * sample.avg_frames_per_tick;
            let speed_ratio = if target_fps > 0.0 {
                effective_run_fps / target_fps
            } else {
                0.0
            };
            info!(
                target: "arcade_ui::perf",
                "play_tick system={} core={} ticks={} tick_hz={:.1} avg_gap_ms={:.2} avg_work_ms={:.2} avg_frames_per_tick={:.2} effective_run_fps={:.2} target_fps={:.2} speed_ratio={:.3}",
                self.state.play.active_system.as_deref().unwrap_or("unknown"),
                self.state.play.active_core.as_deref().unwrap_or("unknown"),
                sample.ticks,
                sample.tick_hz,
                sample.avg_gap_ms,
                sample.avg_work_ms,
                sample.avg_frames_per_tick,
                effective_run_fps,
                target_fps,
                speed_ratio,
            );
        }

        if audio_master_frames.is_some() {
            ctx.request_repaint();
        } else if n64_target_pacing {
            let smooth_pacing = n64_smooth_pacing_enabled();
            if smooth_pacing && tick_work > frame_interval.saturating_mul(2) {
                // A single long frame (e.g. shader compile) can create bursty catch-up.
                // Cap debt so recovery stays smoother without changing average target FPS.
                self.state.play.catch_up_frame_debt = self.state.play.catch_up_frame_debt.min(0.5);
            }
            let target_interval = frame_interval.saturating_mul(frames_executed.max(1));
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                if smooth_pacing {
                    let remaining = target_interval - post_tick_elapsed;
                    ctx.request_repaint_after(remaining.min(LOW_LATENCY_MAX_REPAINT_WAIT));
                } else {
                    ctx.request_repaint();
                }
            } else {
                if smooth_pacing {
                    self.state.play.catch_up_frame_debt =
                        self.state.play.catch_up_frame_debt.min(0.5);
                } else {
                    self.state.play.catch_up_frame_debt = 0.0;
                }
                ctx.request_repaint();
            }
        } else if play_tight_pacing {
            let target_interval = frame_interval;
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                let remaining = target_interval - post_tick_elapsed;
                ctx.request_repaint_after(remaining);
            } else {
                self.state.play.catch_up_frame_debt = self.state.play.catch_up_frame_debt.min(0.25);
                ctx.request_repaint();
            }
        } else if low_latency_pacing {
            let target_interval = frame_interval.saturating_mul(frames_executed.max(1));
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                let remaining = target_interval - post_tick_elapsed;
                ctx.request_repaint_after(remaining.min(LOW_LATENCY_MAX_REPAINT_WAIT));
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
        let return_view = self
            .state
            .play
            .launch_view
            .unwrap_or(self.state.current_view);
        self.state.play.reset_frontend_shortcut_latches();
        self.prev_keyboard_keys_down.clear();
        if let Err(err) = self.host.unload() {
            self.state.play.set_status(format!("stop failed: {err}"));
        }
        self.state.play.clear_session();
        self.assets.last_frame_texture = None;
        self.reset_play_clock();
        if self.state.current_view != return_view {
            self.state.current_view = return_view;
        }
        if let Some(source) = self.current_browse_grid_source() {
            self.repair_grid_selection(source);
            self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
        } else {
            self.state
                .menu_nav
                .focus_top_nav_for_view(self.state.current_view);
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

fn n64_smooth_pacing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("ARCADE_N64_SMOOTH_PACING") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(queue_frames: usize, target_frames: usize) -> AudioQueueSnapshot {
        AudioQueueSnapshot {
            queue_frames,
            target_frames,
            max_frames: target_frames.saturating_mul(2).max(target_frames + 1),
            source_rate: 44_100.0,
            output_rate: 44_100.0,
            trimmed_total_frames: 0,
        }
    }

    #[test]
    fn audio_master_prefers_drain_when_queue_is_high() {
        let target = 10_000;
        let queue = (target as f64 * 1.25).ceil() as usize;
        assert_eq!(
            audio_master_frames_to_run(Some(snapshot(queue, target))),
            Some(0)
        );
    }

    #[test]
    fn audio_master_prefers_refill_when_queue_is_low() {
        let target = 10_000;
        let queue = (target as f64 * 0.85).floor() as usize;
        assert_eq!(
            audio_master_frames_to_run(Some(snapshot(queue, target))),
            Some(2)
        );
    }

    #[test]
    fn audio_master_holds_steady_in_mid_band() {
        let target = 10_000;
        assert_eq!(
            audio_master_frames_to_run(Some(snapshot(10_000, target))),
            Some(1)
        );
    }

    #[test]
    fn audio_master_returns_none_without_snapshot() {
        assert_eq!(audio_master_frames_to_run(None), None);
    }

    #[test]
    fn audio_master_pacing_cores_include_n64_psx_and_ps2() {
        assert!(is_audio_master_pacing_core("mupen64plus_next"));
        assert!(is_audio_master_pacing_core("mednafen_psx_hw"));
        assert!(is_audio_master_pacing_core("pcsx2"));
        assert!(is_audio_master_pacing_core("play"));
        assert!(!is_audio_master_pacing_core("fceumm"));
    }
}
