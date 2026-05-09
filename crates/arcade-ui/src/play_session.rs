use eframe::egui;
use tracing::{info, warn};

use arcade_libretro::{AudioQueueSnapshot, FrameBuffer, FrameOutput, PixelFormat};
use arcade_services::SaveOperationError;
use std::sync::OnceLock;

use crate::{
    app::NativeArcadeUiApp,
    play_runner::{PlayRunner, PlayRunnerEvent, PlayRunnerExecutorMode},
    state::{HoldAction, MenuFocusRegion, PlayLaunchPhase},
};

const AUDIO_MASTER_DRAIN_WATERMARK: f64 = 1.25;
const AUDIO_MASTER_REFILL_WATERMARK: f64 = 0.85;
const PLAY_DEFAULT_MAX_CATCH_UP_FRAMES: u32 = 2;
const PLAY_PCSX2_EXTERNAL_MAX_CATCH_UP_FRAMES: u32 = 3;
const PLAY_CATCH_UP_DEBT_CAP: f64 = 0.75;
const PLAY_REPAINT_WAKE_AHEAD: std::time::Duration = std::time::Duration::from_millis(2);
const PLAY_REPAINT_IMMEDIATE_THRESHOLD: std::time::Duration = std::time::Duration::from_micros(500);

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
        || core_name.eq_ignore_ascii_case("dolphin")
        || core_name.eq_ignore_ascii_case("mupen64plus_next")
        || core_name.eq_ignore_ascii_case("mednafen_psx_hw")
        || core_name.eq_ignore_ascii_case("mednafen_saturn")
}

fn is_tight_frame_clock_core(core_name: &str) -> bool {
    core_name.eq_ignore_ascii_case("pcsx2")
}

fn play_frames_to_run_for_elapsed(
    elapsed: std::time::Duration,
    frame_interval: std::time::Duration,
    catch_up_debt: f64,
    max_catch_up_frames: u32,
) -> (u32, f64, std::time::Duration) {
    if frame_interval.is_zero() {
        return (1, 0.0, std::time::Duration::ZERO);
    }

    let elapsed_frames = elapsed.as_secs_f64() / frame_interval.as_secs_f64();
    let max_catch_up_frames = max_catch_up_frames.max(1);
    let frame_budget = (catch_up_debt + elapsed_frames).clamp(1.0, max_catch_up_frames as f64);
    let frames_to_run = frame_budget.floor().clamp(1.0, max_catch_up_frames as f64) as u32;
    let catch_up_debt = (frame_budget - frames_to_run as f64).clamp(0.0, PLAY_CATCH_UP_DEBT_CAP);
    let advance = frame_interval.saturating_mul(frames_to_run);
    let leftover = elapsed.saturating_sub(advance);

    (frames_to_run, catch_up_debt, leftover)
}

fn tight_frame_clock_max_catch_up_frames(
    core_name: Option<&str>,
    external_present_session: bool,
) -> u32 {
    if external_present_session && core_name.is_some_and(|core| core.eq_ignore_ascii_case("pcsx2"))
    {
        PLAY_PCSX2_EXTERNAL_MAX_CATCH_UP_FRAMES
    } else {
        PLAY_DEFAULT_MAX_CATCH_UP_FRAMES
    }
}

fn tight_frame_clock_post_tick_debt_cap(max_catch_up_frames: u32) -> f64 {
    if max_catch_up_frames > PLAY_DEFAULT_MAX_CATCH_UP_FRAMES {
        PLAY_CATCH_UP_DEBT_CAP
    } else {
        0.25
    }
}

fn play_repaint_delay_for_remaining(remaining: std::time::Duration) -> Option<std::time::Duration> {
    if remaining <= PLAY_REPAINT_IMMEDIATE_THRESHOLD {
        return None;
    }

    let delay = remaining.saturating_sub(PLAY_REPAINT_WAKE_AHEAD);
    if delay <= PLAY_REPAINT_IMMEDIATE_THRESHOLD {
        None
    } else {
        Some(delay)
    }
}

fn request_play_runner_repaint(ctx: &egui::Context, remaining: std::time::Duration) {
    if let Some(delay) = play_repaint_delay_for_remaining(remaining) {
        ctx.request_repaint_after(delay);
    } else {
        ctx.request_repaint();
    }
}

fn env_flag_disabled(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        ),
        Err(_) => false,
    }
}

fn external_present_worker_disabled_for_core(core_name: &str) -> bool {
    if std::env::var_os("ARCADE_DISABLE_PLAY_WORKER_RUNNER").is_some()
        || env_flag_disabled("ARCADE_EXTERNAL_PRESENT_RUNNER")
    {
        return true;
    }

    if core_name.eq_ignore_ascii_case("pcsx2") {
        env_flag_disabled("ARCADE_PCSX2_EXTERNAL_RUNNER")
    } else if core_name.eq_ignore_ascii_case("mupen64plus_next") {
        env_flag_disabled("ARCADE_N64_EXTERNAL_RUNNER")
    } else if core_name.eq_ignore_ascii_case("dolphin") {
        env_flag_disabled("ARCADE_DOLPHIN_EXTERNAL_RUNNER")
    } else {
        false
    }
}

fn play_runner_executor_mode_for_policy(
    external_present_session: bool,
    force_fallback: bool,
    worker_disabled: bool,
) -> PlayRunnerExecutorMode {
    if force_fallback {
        PlayRunnerExecutorMode::FallbackEguiPump
    } else if external_present_session && !worker_disabled {
        PlayRunnerExecutorMode::WorkerThread
    } else {
        PlayRunnerExecutorMode::MainThreadExecutor
    }
}

pub(crate) fn is_dolphin_core(core_name: Option<&str>) -> bool {
    core_name.is_some_and(|core| core.eq_ignore_ascii_case("dolphin"))
}

pub(crate) fn frame_has_sampled_luma(frame: &FrameBuffer) -> bool {
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 || frame.pitch == 0 || frame.data.is_empty() {
        return false;
    }

    let samples_x = width.min(8);
    let samples_y = height.min(8);
    for sy in 0..samples_y {
        let y = if samples_y <= 1 {
            0
        } else {
            sy * (height - 1) / (samples_y - 1)
        };
        for sx in 0..samples_x {
            let x = if samples_x <= 1 {
                0
            } else {
                sx * (width - 1) / (samples_x - 1)
            };
            let Some(offset) = pixel_offset(frame, x, y) else {
                continue;
            };
            if pixel_has_luma(frame, offset) {
                return true;
            }
        }
    }

    false
}

fn pixel_offset(frame: &FrameBuffer, x: usize, y: usize) -> Option<usize> {
    let bytes_per_pixel = match frame.pixel_format {
        PixelFormat::Argb1555 | PixelFormat::Rgb565 => 2,
        PixelFormat::Xrgb8888 | PixelFormat::Rgba8888 => 4,
    };
    y.checked_mul(frame.pitch)?
        .checked_add(x.checked_mul(bytes_per_pixel)?)?
        .checked_add(bytes_per_pixel)
        .filter(|end| *end <= frame.data.len())
        .map(|end| end - bytes_per_pixel)
}

fn pixel_has_luma(frame: &FrameBuffer, offset: usize) -> bool {
    match frame.pixel_format {
        PixelFormat::Rgba8888 => {
            frame.data[offset] != 0 || frame.data[offset + 1] != 0 || frame.data[offset + 2] != 0
        }
        PixelFormat::Xrgb8888 => {
            frame.data[offset + 1] != 0
                || frame.data[offset + 2] != 0
                || frame.data[offset + 3] != 0
        }
        PixelFormat::Rgb565 | PixelFormat::Argb1555 => {
            frame.data[offset] != 0 || frame.data[offset + 1] != 0
        }
    }
}

impl NativeArcadeUiApp {
    const PLAY_OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_millis(1800);
    const PLAY_BAR_DURATION: std::time::Duration = std::time::Duration::from_secs(2);
    const PLAY_RETURN_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);
    const PLAY_RESET_HOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(800);

    pub(crate) fn start_play_runner_for_loaded_session(
        &mut self,
        ctx: &egui::Context,
        core_name: &str,
    ) {
        self.stop_play_runner_only();
        let frame_interval = self
            .host
            .frame_interval()
            .unwrap_or_else(|| std::time::Duration::from_secs_f64(1.0 / 60.0));
        let executor_mode = self.play_runner_executor_mode(core_name);
        self.play_runner = Some(PlayRunner::start(
            executor_mode,
            self.host.run_handle(),
            frame_interval,
            ctx,
        ));
        self.state
            .play
            .set_runner_started(executor_mode.label().to_owned());
        info!(
            target: "arcade_ui::perf",
            core = core_name,
            executor_mode = executor_mode.label(),
            target_fps = 1.0 / frame_interval.as_secs_f64(),
            "play_runner session started"
        );
    }

    fn play_runner_executor_mode(&self, core_name: &str) -> PlayRunnerExecutorMode {
        play_runner_executor_mode_for_policy(
            self.host.expects_external_vulkan_present_window()
                || self.host.using_external_vulkan_present_window(),
            std::env::var_os("ARCADE_FORCE_FALLBACK_EGUI_PUMP").is_some(),
            external_present_worker_disabled_for_core(core_name),
        )
    }

    fn stop_play_runner_only(&mut self) {
        if let Some(mut runner) = self.play_runner.take() {
            runner.stop();
        }
        self.state.play.clear_runner_state();
    }

    pub(crate) fn pump_play_runner(&mut self, ctx: &egui::Context) {
        if self.play_runner.is_none() && self.host.is_loaded() {
            let core = self
                .state
                .play
                .active_core
                .clone()
                .unwrap_or_else(|| String::from("unknown"));
            self.start_play_runner_for_loaded_session(ctx, &core);
        }

        if self
            .play_runner
            .as_ref()
            .is_some_and(|runner| runner.executor_mode() == PlayRunnerExecutorMode::WorkerThread)
        {
            self.drain_worker_play_runner(ctx);
        } else {
            self.pump_main_thread_play_runner(ctx);
        }

        self.host
            .sync_external_vulkan_window_visibility_on_main_thread();

        if let Some(runner) = self.play_runner.as_ref() {
            self.state.play.update_runner_status(
                runner.last_event_label(),
                runner.missed_deadlines(),
                runner.is_stopping(),
            );
        }
    }

    fn drain_worker_play_runner(&mut self, ctx: &egui::Context) {
        let events = self
            .play_runner
            .as_mut()
            .map(PlayRunner::drain_events)
            .unwrap_or_default();

        let mut frames_advanced = 0_u32;
        for event in events {
            match event {
                PlayRunnerEvent::FrameDelivered(frame) => {
                    self.handle_runner_frame_output(ctx, frame);
                }
                PlayRunnerEvent::PresentationReady => {
                    self.mark_launch_presentation_ready();
                }
                PlayRunnerEvent::FrameStepped(_stats) => {
                    frames_advanced = frames_advanced.saturating_add(1);
                }
                PlayRunnerEvent::Failed(message) => {
                    warn!("play runner failed: {message}");
                    self.state.play.set_status(message);
                    self.stop_play_session();
                    ctx.request_repaint();
                    return;
                }
                PlayRunnerEvent::Stopped => {}
            }
        }

        if frames_advanced > 0 {
            self.mark_retro_keyboard_frame_advanced();
            self.flush_deferred_retro_keyboard_releases();
        }

        if self.external_present_ready_for_launch() {
            self.mark_launch_presentation_ready();
        } else {
            self.maybe_warn_launch_waiting_for_presentation();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn handle_runner_frame_output(&mut self, ctx: &egui::Context, frame: FrameOutput) {
        match frame {
            FrameOutput::Cpu(frame) => self.update_frame_texture(ctx, frame),
            FrameOutput::GlTexture(frame) => self.update_gl_texture_frame(frame),
            #[cfg(target_os = "macos")]
            FrameOutput::MacosIosurface(frame) => self.update_macos_iosurface_frame(frame),
        }
        self.mark_launch_presentation_ready();
    }

    pub(crate) fn pump_main_thread_play_runner(&mut self, ctx: &egui::Context) {
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
        let prefer_visible_cpu_frame = is_dolphin_core(active_core);
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
        let tight_frame_clock_pacing = active_core.is_some_and(is_tight_frame_clock_core);
        let tight_frame_clock_max_catch_up_frames = tight_frame_clock_max_catch_up_frames(
            active_core,
            self.host.expects_external_vulkan_present_window()
                || self.host.using_external_vulkan_present_window(),
        );
        let audio_snapshot = self.host.audio_queue_snapshot();
        let audio_master_frames = audio_master_frames_to_run(
            active_core
                .is_some_and(is_audio_master_pacing_core)
                .then_some(audio_snapshot)
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
        } else if tight_frame_clock_pacing {
            if elapsed < frame_interval {
                let remaining = frame_interval - elapsed;
                request_play_runner_repaint(ctx, remaining);
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
        } else if tight_frame_clock_pacing {
            // These cores should follow the core-reported frame clock, not the audio queue.
            // Allow small wall-clock catch-up when the UI timer fires late so audio
            // production does not starve, but cap it tightly to avoid FMV overspeed.
            let (frames_to_run, catch_up_debt, leftover) = play_frames_to_run_for_elapsed(
                elapsed,
                frame_interval,
                self.state.play.catch_up_frame_debt,
                tight_frame_clock_max_catch_up_frames,
            );
            self.state
                .play
                .set_last_frame_run_at(now.checked_sub(leftover).unwrap_or(now));
            self.state.play.catch_up_frame_debt = catch_up_debt;
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
        let frame_timing_before = self.host.frame_timing_snapshot();
        let mut frames_executed = 0;
        let mut terminal_frame_error = None;
        for _ in 0..frames_to_run {
            frames_executed += 1;
            match self.host.run_frame() {
                Ok(Some(frame)) => {
                    if prefer_visible_cpu_frame {
                        match (&frame, &latest_frame) {
                            (FrameOutput::Cpu(frame), Some(FrameOutput::Cpu(previous)))
                                if !frame_has_sampled_luma(frame)
                                    && frame_has_sampled_luma(previous) => {}
                            _ => latest_frame = Some(frame),
                        }
                    } else {
                        latest_frame = Some(frame);
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    let message = err.to_string();
                    warn!("frame loop failed: {message}");
                    terminal_frame_error = Some(message);
                    break;
                }
            }
        }
        let frame_timing_after = self.host.frame_timing_snapshot();
        let timed_frames = frame_timing_after
            .frames
            .saturating_sub(frame_timing_before.frames);
        let core_run_work = std::time::Duration::from_micros(
            frame_timing_after
                .run_total_us
                .saturating_sub(frame_timing_before.run_total_us),
        );
        let frame_delivery_work = std::time::Duration::from_micros(
            frame_timing_after
                .frame_delivery_total_us
                .saturating_sub(frame_timing_before.frame_delivery_total_us),
        );
        if frames_executed > 0 {
            self.mark_retro_keyboard_frame_advanced();
            self.flush_deferred_retro_keyboard_releases();
        }
        if let Some(message) = terminal_frame_error {
            self.state.play.set_status(message);
            self.stop_play_session();
            ctx.request_repaint();
            return;
        }
        if let Some(frame) = latest_frame {
            self.handle_runner_frame_output(ctx, frame);
        } else if self.external_present_ready_for_launch() {
            self.mark_launch_presentation_ready();
        } else {
            self.maybe_warn_launch_waiting_for_presentation();
        }
        let tick_work = tick_started_at.elapsed();
        if let Some(sample) = self.state.play.record_perf_tick(
            elapsed,
            tick_work,
            core_run_work,
            frame_delivery_work,
            timed_frames,
            frames_executed,
        ) {
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
                "play_tick system={} core={} ticks={} tick_hz={:.1} avg_gap_ms={:.2} avg_work_ms={:.2} avg_core_run_ms={:.2} avg_frame_delivery_ms={:.2} avg_frames_per_tick={:.2} effective_run_fps={:.2} target_fps={:.2} speed_ratio={:.3} audio_queue_frames={} audio_target_frames={} audio_source_rate={:.1} audio_output_rate={:.1}",
                self.state.play.active_system.as_deref().unwrap_or("unknown"),
                self.state.play.active_core.as_deref().unwrap_or("unknown"),
                sample.ticks,
                sample.tick_hz,
                sample.avg_gap_ms,
                sample.avg_work_ms,
                sample.avg_core_run_ms,
                sample.avg_frame_delivery_ms,
                sample.avg_frames_per_tick,
                effective_run_fps,
                target_fps,
                speed_ratio,
                audio_snapshot.map(|snapshot| snapshot.queue_frames).unwrap_or(0),
                audio_snapshot.map(|snapshot| snapshot.target_frames).unwrap_or(0),
                audio_snapshot
                    .map(|snapshot| snapshot.source_rate)
                    .unwrap_or(0.0),
                audio_snapshot
                    .map(|snapshot| snapshot.output_rate)
                    .unwrap_or(0.0),
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
        } else if tight_frame_clock_pacing {
            let target_interval = frame_interval;
            let post_tick_elapsed =
                std::time::Instant::now().duration_since(self.state.play.last_frame_run_at);
            if post_tick_elapsed < target_interval {
                let remaining = target_interval - post_tick_elapsed;
                request_play_runner_repaint(ctx, remaining);
            } else {
                self.state.play.catch_up_frame_debt =
                    self.state
                        .play
                        .catch_up_frame_debt
                        .min(tight_frame_clock_post_tick_debt_cap(
                            tight_frame_clock_max_catch_up_frames,
                        ));
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
        let host = self.host.clone();
        let reset_result = if let Some(runner) = self.play_runner.as_mut() {
            runner.reset(|| host.reset())
        } else {
            self.host.reset().map_err(|err| err.to_string())
        };
        if let Err(err) = reset_result {
            self.state.play.set_status(format!("reset failed: {err}"));
        } else {
            self.reset_play_clock();
        }
    }

    fn mark_launch_presentation_ready(&mut self) {
        if matches!(
            self.state.play.launch_phase,
            PlayLaunchPhase::WaitingForPresentation | PlayLaunchPhase::Warning
        ) {
            self.state.play.launch_friendly_message = Some(String::from("Playing"));
            self.state.play.launch_detail_message = None;
            self.state.play.set_launch_phase(PlayLaunchPhase::Playing);
        }
    }

    fn external_present_ready_for_launch(&self) -> bool {
        let status = self.host.presentation_status();
        status.external_present_active || status.external_present_deliveries > 0
    }

    fn maybe_warn_launch_waiting_for_presentation(&mut self) {
        if self.state.play.launch_phase != PlayLaunchPhase::WaitingForPresentation {
            return;
        }
        if self.state.play.launch_phase_started_at.elapsed() < std::time::Duration::from_secs(8) {
            return;
        }
        let detail = if self.host.expects_external_vulkan_present_window() {
            let status = self.host.presentation_status();
            format!(
                "Core loaded, but no external-present frame has appeared yet. external_window_created={}, queue_present_successes={}, external_present_deliveries={}",
                status.external_window_created,
                self.host.vulkan_present_test_metrics().queue_present_successes,
                status.external_present_deliveries
            )
        } else {
            String::from("Core loaded, but no embedded video frame has appeared yet.")
        };
        self.state.play.warn_launch("Still starting...", detail);
    }

    pub(crate) fn stop_play_session(&mut self) {
        let return_view = self
            .state
            .play
            .launch_view
            .unwrap_or(self.state.current_view);
        self.state.play.reset_frontend_shortcut_latches();
        self.prev_keyboard_keys_down.clear();
        self.retro_keys_pressed_since_frame.clear();
        self.pending_retro_key_releases.clear();
        self.stop_play_runner_only();
        if let Err(err) = self.host.unload() {
            self.state.play.set_status(format!("stop failed: {err}"));
        }
        self.state.play.clear_session();
        self.assets.last_frame_texture = None;
        self.assets.last_gl_texture_frame = None;
        #[cfg(target_os = "macos")]
        {
            self.assets.last_macos_iosurface_frame = None;
        }
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
        let host = self.host.clone();
        let serialized = if let Some(runner) = self.play_runner.as_mut() {
            runner.serialize_state(|| host.serialize_state())
        } else {
            self.host.serialize_state().map_err(|err| err.to_string())
        };
        match serialized {
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
            Ok(Some(slot_data)) => {
                let host = self.host.clone();
                let loaded = if let Some(runner) = self.play_runner.as_mut() {
                    runner.unserialize_state(slot_data.bytes.clone(), |bytes| {
                        host.unserialize_state(bytes)
                    })
                } else {
                    self.host
                        .unserialize_state(&slot_data.bytes)
                        .map_err(|err| err.to_string())
                };
                match loaded {
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
                }
            }
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
    fn audio_master_pacing_cores_include_external_present_and_heavy_hw_cores() {
        assert!(is_audio_master_pacing_core("dolphin"));
        assert!(is_audio_master_pacing_core("mupen64plus_next"));
        assert!(is_audio_master_pacing_core("mednafen_psx_hw"));
        assert!(is_audio_master_pacing_core("mednafen_saturn"));
        assert!(!is_audio_master_pacing_core("pcsx2"));
        assert!(!is_audio_master_pacing_core("fceumm"));
    }

    #[test]
    fn external_present_policy_uses_worker_runner_for_all_external_present_cores() {
        assert_eq!(
            play_runner_executor_mode_for_policy(true, false, false),
            PlayRunnerExecutorMode::WorkerThread
        );
    }

    #[test]
    fn external_present_policy_respects_fallback_and_worker_disable() {
        assert_eq!(
            play_runner_executor_mode_for_policy(true, true, false),
            PlayRunnerExecutorMode::FallbackEguiPump
        );
        assert_eq!(
            play_runner_executor_mode_for_policy(true, false, true),
            PlayRunnerExecutorMode::MainThreadExecutor
        );
        assert_eq!(
            play_runner_executor_mode_for_policy(false, false, false),
            PlayRunnerExecutorMode::MainThreadExecutor
        );
    }

    #[test]
    fn tight_frame_clock_cores_include_pcsx2() {
        assert!(is_tight_frame_clock_core("pcsx2"));
        assert!(!is_tight_frame_clock_core("flycast"));
    }

    #[test]
    fn pcsx2_external_present_allows_three_frame_catch_up() {
        assert_eq!(
            tight_frame_clock_max_catch_up_frames(Some("pcsx2"), true),
            3
        );
        assert_eq!(
            tight_frame_clock_max_catch_up_frames(Some("pcsx2"), false),
            2
        );
        assert_eq!(
            tight_frame_clock_max_catch_up_frames(Some("pcarmsx2"), true),
            2
        );
    }

    #[test]
    fn three_frame_tight_clock_keeps_enough_debt_to_trigger_catch_up() {
        assert_eq!(tight_frame_clock_post_tick_debt_cap(2), 0.25);
        assert_eq!(
            tight_frame_clock_post_tick_debt_cap(3),
            PLAY_CATCH_UP_DEBT_CAP
        );
    }

    #[test]
    fn sampled_luma_rejects_blank_rgba_frame() {
        let frame = FrameBuffer {
            width: 4,
            height: 4,
            pitch: 16,
            data: vec![0; 4 * 4 * 4],
            pixel_format: PixelFormat::Rgba8888,
        };

        assert!(!frame_has_sampled_luma(&frame));
    }

    #[test]
    fn sampled_luma_accepts_non_black_rgba_frame() {
        let mut data = vec![0; 4 * 4 * 4];
        data[(2 * 16) + (2 * 4)] = 7;
        let frame = FrameBuffer {
            width: 4,
            height: 4,
            pitch: 16,
            data,
            pixel_format: PixelFormat::Rgba8888,
        };

        assert!(frame_has_sampled_luma(&frame));
    }

    #[test]
    fn play_frame_pacing_runs_one_frame_for_small_jitter() {
        let frame_interval = std::time::Duration::from_micros(16_667);
        let elapsed = frame_interval + std::time::Duration::from_millis(3);
        let (frames_to_run, debt, leftover) =
            play_frames_to_run_for_elapsed(elapsed, frame_interval, 0.0, 2);

        assert_eq!(frames_to_run, 1);
        assert!(debt > 0.0);
        assert_eq!(leftover, std::time::Duration::from_millis(3));
    }

    #[test]
    fn play_frame_pacing_catches_up_when_wall_clock_is_two_frames_late() {
        let frame_interval = std::time::Duration::from_micros(16_667);
        let elapsed = frame_interval.saturating_mul(2) + std::time::Duration::from_millis(2);
        let (frames_to_run, debt, leftover) =
            play_frames_to_run_for_elapsed(elapsed, frame_interval, 0.0, 2);

        assert_eq!(frames_to_run, 2);
        assert_eq!(debt, 0.0);
        assert_eq!(leftover, std::time::Duration::from_millis(2));
    }

    #[test]
    fn play_frame_pacing_can_repay_debt_with_three_frame_cap() {
        let frame_interval = std::time::Duration::from_micros(16_667);
        let elapsed = frame_interval.saturating_mul(2) + std::time::Duration::from_millis(9);
        let (frames_to_run, debt, _leftover) =
            play_frames_to_run_for_elapsed(elapsed, frame_interval, 0.0, 3);

        assert_eq!(frames_to_run, 2);
        assert!(debt > 0.5);

        let (frames_to_run, debt, _leftover) =
            play_frames_to_run_for_elapsed(elapsed, frame_interval, debt, 3);
        assert_eq!(frames_to_run, 3);
        assert!(debt < 0.25);
    }

    #[test]
    fn play_repaint_delay_wakes_ahead_of_frame_deadline() {
        assert_eq!(
            play_repaint_delay_for_remaining(std::time::Duration::from_millis(10)),
            Some(std::time::Duration::from_millis(8))
        );
    }

    #[test]
    fn play_repaint_delay_requests_immediate_near_deadline() {
        assert_eq!(
            play_repaint_delay_for_remaining(std::time::Duration::from_millis(2)),
            None
        );
        assert_eq!(
            play_repaint_delay_for_remaining(std::time::Duration::from_micros(400)),
            None
        );
    }
}
