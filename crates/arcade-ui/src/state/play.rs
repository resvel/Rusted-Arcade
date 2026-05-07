use std::time::{Duration, Instant};

use arcade_services::LaunchPlan;

use crate::app::AppView;

const PLAY_PERF_SAMPLE_TICKS: u32 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldAction {
    None,
    Started,
    Triggered,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlayPerfSample {
    pub(crate) ticks: u32,
    pub(crate) tick_hz: f64,
    pub(crate) avg_gap_ms: f64,
    pub(crate) avg_work_ms: f64,
    pub(crate) avg_core_run_ms: f64,
    pub(crate) avg_frame_delivery_ms: f64,
    pub(crate) avg_frames_per_tick: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayLaunchPhase {
    Idle,
    Queued,
    LoadingCore,
    WaitingForPresentation,
    Playing,
    Warning,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingLaunch {
    pub(crate) plan: LaunchPlan,
    pub(crate) launch_view: AppView,
    pub(crate) shell_painted: bool,
}

pub(crate) struct PlaySessionState {
    pub(crate) status: String,
    pub(crate) launch_phase: PlayLaunchPhase,
    pub(crate) launch_phase_started_at: Instant,
    pub(crate) launch_title: Option<String>,
    pub(crate) launch_system: Option<String>,
    pub(crate) launch_core: Option<String>,
    pub(crate) launch_cover_path: Option<String>,
    pub(crate) launch_preview_poster_path: Option<String>,
    pub(crate) launch_friendly_message: Option<String>,
    pub(crate) launch_detail_message: Option<String>,
    pub(crate) launch_note_message: Option<String>,
    pub(crate) pending_launch: Option<PendingLaunch>,
    pub(crate) overlay_message: String,
    pub(crate) overlay_visible_until: Option<Instant>,
    pub(crate) bar_visible_until: Option<Instant>,
    pub(crate) return_pressed_at: Option<Instant>,
    pub(crate) reset_pressed_at: Option<Instant>,
    pub(crate) save_slot: i32,
    pub(crate) reset_input_held: bool,
    pub(crate) quick_save_held: bool,
    pub(crate) quick_load_held: bool,
    pub(crate) next_save_slot_held: bool,
    pub(crate) return_input_held: bool,
    pub(crate) block_frontend_shortcuts_until_release: bool,
    pub(crate) last_frame_size: Option<(u32, u32)>,
    pub(crate) last_frame_run_at: Instant,
    pub(crate) viewport_immersive_applied: bool,
    pub(crate) viewport_enter_stage: u8,
    pub(crate) viewport_restore_stage: u8,
    pub(crate) viewport_restore_frames: u8,
    pub(crate) restore_maximized: bool,
    pub(crate) viewport_hidden_for_external_present: bool,
    pub(crate) restore_outer_position: Option<(f32, f32)>,
    pub(crate) active_rom_id: Option<String>,
    pub(crate) active_system: Option<String>,
    pub(crate) active_core: Option<String>,
    pub(crate) launch_view: Option<AppView>,
    pub(crate) catch_up_frame_debt: f64,
    pub(crate) perf_tick_count: u32,
    pub(crate) perf_gap_ns: u64,
    pub(crate) perf_work_ns: u64,
    pub(crate) perf_core_run_ns: u64,
    pub(crate) perf_frame_delivery_ns: u64,
    pub(crate) perf_timed_frames: u64,
    pub(crate) perf_frames_run: u32,
    pub(crate) runner_active: bool,
    pub(crate) runner_executor_mode: Option<String>,
    pub(crate) runner_last_event: Option<String>,
    pub(crate) runner_missed_deadlines: u64,
    pub(crate) runner_shutdown_status: Option<String>,
}

impl Default for PlaySessionState {
    fn default() -> Self {
        Self {
            status: String::from("Ready"),
            launch_phase: PlayLaunchPhase::Idle,
            launch_phase_started_at: Instant::now(),
            launch_title: None,
            launch_system: None,
            launch_core: None,
            launch_cover_path: None,
            launch_preview_poster_path: None,
            launch_friendly_message: None,
            launch_detail_message: None,
            launch_note_message: None,
            pending_launch: None,
            overlay_message: String::new(),
            overlay_visible_until: None,
            bar_visible_until: None,
            return_pressed_at: None,
            reset_pressed_at: None,
            save_slot: 1,
            reset_input_held: false,
            quick_save_held: false,
            quick_load_held: false,
            next_save_slot_held: false,
            return_input_held: false,
            block_frontend_shortcuts_until_release: false,
            last_frame_size: None,
            last_frame_run_at: Instant::now(),
            viewport_immersive_applied: false,
            viewport_enter_stage: 0,
            viewport_restore_stage: 0,
            viewport_restore_frames: 0,
            restore_maximized: false,
            viewport_hidden_for_external_present: false,
            restore_outer_position: None,
            active_rom_id: None,
            active_system: None,
            active_core: None,
            launch_view: None,
            catch_up_frame_debt: 0.0,
            perf_tick_count: 0,
            perf_gap_ns: 0,
            perf_work_ns: 0,
            perf_core_run_ns: 0,
            perf_frame_delivery_ns: 0,
            perf_timed_frames: 0,
            perf_frames_run: 0,
            runner_active: false,
            runner_executor_mode: None,
            runner_last_event: None,
            runner_missed_deadlines: 0,
            runner_shutdown_status: None,
        }
    }
}

impl PlaySessionState {
    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    pub(crate) fn launch_shell_active(&self) -> bool {
        self.launch_phase != PlayLaunchPhase::Idle
    }

    pub(crate) fn set_launch_phase(&mut self, phase: PlayLaunchPhase) {
        self.launch_phase = phase;
        self.launch_phase_started_at = Instant::now();
    }

    pub(crate) fn queue_launch(&mut self, plan: LaunchPlan, launch_view: AppView) {
        self.status = String::from("Starting...");
        self.launch_title = Some(plan.display_title.clone());
        self.launch_system = Some(plan.system.clone());
        self.launch_core = Some(plan.resolved_core_name.clone());
        self.launch_cover_path = plan.cover_path.clone();
        self.launch_preview_poster_path = plan.preview_poster_path.clone();
        self.launch_friendly_message = Some(String::from("Starting..."));
        self.launch_detail_message = None;
        self.launch_note_message = plan.active_core_note.map(String::from);
        self.pending_launch = Some(PendingLaunch {
            plan,
            launch_view,
            shell_painted: false,
        });
        self.set_launch_phase(PlayLaunchPhase::Queued);
    }

    pub(crate) fn mark_launch_shell_painted(&mut self) {
        if let Some(pending) = self.pending_launch.as_mut() {
            pending.shell_painted = true;
        }
    }

    pub(crate) fn pending_launch_ready_to_load(&self) -> bool {
        self.launch_phase == PlayLaunchPhase::Queued
            && self
                .pending_launch
                .as_ref()
                .is_some_and(|pending| pending.shell_painted)
    }

    pub(crate) fn take_ready_pending_launch(&mut self) -> Option<PendingLaunch> {
        if self.pending_launch_ready_to_load() {
            self.pending_launch.take()
        } else {
            None
        }
    }

    pub(crate) fn fail_launch(
        &mut self,
        title: Option<String>,
        system: Option<String>,
        core: Option<String>,
        cover_path: Option<String>,
        preview_poster_path: Option<String>,
        friendly: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.launch_title = title;
        self.launch_system = system;
        self.launch_core = core;
        self.launch_cover_path = cover_path;
        self.launch_preview_poster_path = preview_poster_path;
        let friendly = friendly.into();
        let detail = detail.into();
        self.status = friendly.clone();
        self.launch_friendly_message = Some(friendly);
        self.launch_detail_message = Some(detail);
        self.launch_note_message = None;
        self.pending_launch = None;
        self.set_launch_phase(PlayLaunchPhase::Failed);
    }

    pub(crate) fn warn_launch(&mut self, friendly: impl Into<String>, detail: impl Into<String>) {
        let friendly = friendly.into();
        self.status = friendly.clone();
        self.launch_friendly_message = Some(friendly);
        self.launch_detail_message = Some(detail.into());
        self.set_launch_phase(PlayLaunchPhase::Warning);
    }

    pub(crate) fn dismiss_launch_shell(&mut self) {
        self.pending_launch = None;
        self.launch_title = None;
        self.launch_system = None;
        self.launch_core = None;
        self.launch_cover_path = None;
        self.launch_preview_poster_path = None;
        self.launch_friendly_message = None;
        self.launch_detail_message = None;
        self.launch_note_message = None;
        self.set_launch_phase(PlayLaunchPhase::Idle);
    }

    pub(crate) fn begin_session(
        &mut self,
        status_message: String,
        rom_id: String,
        system: String,
        core: String,
        launch_view: AppView,
    ) {
        self.status = status_message;
        self.launch_title.get_or_insert_with(|| rom_id.clone());
        self.launch_system = Some(system.clone());
        self.launch_core = Some(core.clone());
        self.launch_friendly_message = Some(String::from("Starting..."));
        self.launch_detail_message = None;
        self.pending_launch = None;
        self.set_launch_phase(PlayLaunchPhase::WaitingForPresentation);
        self.active_rom_id = Some(rom_id);
        self.active_system = Some(system);
        self.active_core = Some(core);
        self.launch_view = Some(launch_view);
        self.last_frame_size = None;
        self.return_pressed_at = None;
        self.return_input_held = false;
        self.reset_pressed_at = None;
        self.reset_input_held = false;
        self.quick_save_held = false;
        self.quick_load_held = false;
        self.next_save_slot_held = false;
        self.block_frontend_shortcuts_until_release = true;
        self.catch_up_frame_debt = 0.0;
        self.viewport_hidden_for_external_present = false;
        self.restore_outer_position = None;
        self.reset_perf_counters();
        self.reset_clock();
        self.clear_runner_state();
    }

    pub(crate) fn clear_active_launch(&mut self) {
        self.active_rom_id = None;
        self.active_system = None;
        self.active_core = None;
        self.launch_view = None;
    }

    pub(crate) fn clear_session(&mut self) {
        self.clear_active_launch();
        self.dismiss_launch_shell();
        self.bar_visible_until = None;
        self.return_pressed_at = None;
        self.return_input_held = false;
        self.reset_pressed_at = None;
        self.reset_input_held = false;
        self.quick_save_held = false;
        self.quick_load_held = false;
        self.next_save_slot_held = false;
        self.block_frontend_shortcuts_until_release = false;
        self.last_frame_size = None;
        self.catch_up_frame_debt = 0.0;
        self.viewport_hidden_for_external_present = false;
        self.restore_outer_position = None;
        self.reset_perf_counters();
        self.clear_runner_state();
    }

    pub(crate) fn set_runner_started(&mut self, executor_mode: impl Into<String>) {
        self.runner_active = true;
        self.runner_executor_mode = Some(executor_mode.into());
        self.runner_last_event = Some(String::from("Started"));
        self.runner_missed_deadlines = 0;
        self.runner_shutdown_status = None;
    }

    pub(crate) fn update_runner_status(
        &mut self,
        last_event: Option<&str>,
        missed_deadlines: u64,
        stopping: bool,
    ) {
        self.runner_active = true;
        self.runner_last_event = last_event.map(String::from);
        self.runner_missed_deadlines = missed_deadlines;
        self.runner_shutdown_status = stopping.then_some(String::from("Stopping"));
    }

    pub(crate) fn clear_runner_state(&mut self) {
        self.runner_active = false;
        self.runner_executor_mode = None;
        self.runner_last_event = None;
        self.runner_missed_deadlines = 0;
        self.runner_shutdown_status = None;
    }

    pub(crate) fn reset_clock(&mut self) {
        self.last_frame_run_at = Instant::now();
        self.catch_up_frame_debt = 0.0;
    }

    pub(crate) fn set_last_frame_run_at(&mut self, at: Instant) {
        self.last_frame_run_at = at;
    }

    pub(crate) fn record_perf_tick(
        &mut self,
        gap: Duration,
        work: Duration,
        core_run_work: Duration,
        frame_delivery_work: Duration,
        timed_frames: u64,
        frames_run: u32,
    ) -> Option<PlayPerfSample> {
        self.perf_tick_count = self.perf_tick_count.saturating_add(1);
        self.perf_gap_ns = self.perf_gap_ns.saturating_add(duration_to_ns(gap));
        self.perf_work_ns = self.perf_work_ns.saturating_add(duration_to_ns(work));
        self.perf_core_run_ns = self
            .perf_core_run_ns
            .saturating_add(duration_to_ns(core_run_work));
        self.perf_frame_delivery_ns = self
            .perf_frame_delivery_ns
            .saturating_add(duration_to_ns(frame_delivery_work));
        self.perf_timed_frames = self.perf_timed_frames.saturating_add(timed_frames);
        self.perf_frames_run = self.perf_frames_run.saturating_add(frames_run);

        if self.perf_tick_count < PLAY_PERF_SAMPLE_TICKS {
            return None;
        }

        let ticks = self.perf_tick_count;
        let total_gap_ns = self.perf_gap_ns.max(1);
        let total_work_ns = self.perf_work_ns;
        let total_core_run_ns = self.perf_core_run_ns;
        let total_frame_delivery_ns = self.perf_frame_delivery_ns;
        let total_timed_frames = self.perf_timed_frames.max(1);
        let total_frames_run = self.perf_frames_run;

        self.reset_perf_counters();

        Some(PlayPerfSample {
            ticks,
            tick_hz: ticks as f64 * 1_000_000_000.0 / total_gap_ns as f64,
            avg_gap_ms: total_gap_ns as f64 / ticks as f64 / 1_000_000.0,
            avg_work_ms: total_work_ns as f64 / ticks as f64 / 1_000_000.0,
            avg_core_run_ms: total_core_run_ns as f64 / total_timed_frames as f64 / 1_000_000.0,
            avg_frame_delivery_ms: total_frame_delivery_ns as f64
                / total_timed_frames as f64
                / 1_000_000.0,
            avg_frames_per_tick: total_frames_run as f64 / ticks as f64,
        })
    }

    pub(crate) fn advance_save_slot(&mut self, slot_count: i32) -> i32 {
        let slot_count = slot_count.max(1);
        self.save_slot = (self.save_slot + 1).rem_euclid(slot_count);
        self.save_slot
    }

    pub(crate) fn reset_frontend_shortcut_latches(&mut self) {
        self.reset_input_held = false;
        self.reset_pressed_at = None;
        self.return_input_held = false;
        self.return_pressed_at = None;
        self.quick_save_held = false;
        self.quick_load_held = false;
        self.next_save_slot_held = false;
    }

    pub(crate) fn should_block_frontend_shortcuts_until_release(
        &mut self,
        any_shortcut_pressed: bool,
    ) -> bool {
        if !self.block_frontend_shortcuts_until_release {
            return false;
        }
        if any_shortcut_pressed {
            return true;
        }
        self.block_frontend_shortcuts_until_release = false;
        false
    }

    pub(crate) fn handle_reset_input(
        &mut self,
        now: Instant,
        reset_pressed: bool,
        hold_duration: Duration,
    ) -> HoldAction {
        consume_hold_action(
            now,
            &mut self.reset_input_held,
            reset_pressed,
            &mut self.reset_pressed_at,
            hold_duration,
        )
    }

    pub(crate) fn consume_quick_save_press(&mut self, is_held: bool) -> bool {
        consume_rising_edge(&mut self.quick_save_held, is_held)
    }

    pub(crate) fn consume_quick_load_press(&mut self, is_held: bool) -> bool {
        consume_rising_edge(&mut self.quick_load_held, is_held)
    }

    pub(crate) fn consume_next_save_slot_press(&mut self, is_held: bool) -> bool {
        consume_rising_edge(&mut self.next_save_slot_held, is_held)
    }

    pub(crate) fn show_bar(&mut self, now: Instant, duration: Duration) {
        self.bar_visible_until = Some(now + duration);
    }

    pub(crate) fn bar_visible(&self, now: Instant) -> bool {
        self.bar_visible_until.is_some_and(|until| now <= until)
    }

    pub(crate) fn active_overlay(&mut self, now: Instant) -> Option<String> {
        let until = self.overlay_visible_until?;
        if now <= until {
            return Some(self.overlay_message.clone());
        }

        self.overlay_visible_until = None;
        self.overlay_message.clear();
        None
    }

    pub(crate) fn set_feedback(
        &mut self,
        now: Instant,
        overlay_duration: Duration,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.status = message.clone();
        self.overlay_message = message;
        self.overlay_visible_until = Some(now + overlay_duration);
    }

    pub(crate) fn handle_return_input(
        &mut self,
        now: Instant,
        return_pressed: bool,
        hold_duration: Duration,
    ) -> HoldAction {
        consume_hold_action(
            now,
            &mut self.return_input_held,
            return_pressed,
            &mut self.return_pressed_at,
            hold_duration,
        )
    }

    fn reset_perf_counters(&mut self) {
        self.perf_tick_count = 0;
        self.perf_gap_ns = 0;
        self.perf_work_ns = 0;
        self.perf_core_run_ns = 0;
        self.perf_frame_delivery_ns = 0;
        self.perf_timed_frames = 0;
        self.perf_frames_run = 0;
    }
}

fn duration_to_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn consume_rising_edge(was_held: &mut bool, is_held: bool) -> bool {
    let triggered = is_held && !*was_held;
    *was_held = is_held;
    triggered
}

fn consume_hold_action(
    now: Instant,
    was_held: &mut bool,
    is_held: bool,
    pressed_at: &mut Option<Instant>,
    hold_duration: Duration,
) -> HoldAction {
    if !is_held {
        *was_held = false;
        *pressed_at = None;
        return HoldAction::None;
    }

    if !*was_held {
        *was_held = true;
        *pressed_at = Some(now);
        return HoldAction::Started;
    }

    if pressed_at
        .is_some_and(|started_at| now.saturating_duration_since(started_at) >= hold_duration)
    {
        *pressed_at = None;
        return HoldAction::Triggered;
    }

    HoldAction::None
}

#[cfg(test)]
mod tests {
    use super::{HoldAction, PlayLaunchPhase, PlaySessionState};
    use crate::app::AppView;
    use arcade_services::LaunchPlan;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn launch_plan() -> LaunchPlan {
        LaunchPlan {
            rom_id: String::from("rom-1"),
            display_title: String::from("Test Game"),
            system: String::from("NES"),
            rom_path: PathBuf::from("roms/nes/test.nes"),
            effective_core: None,
            resolved_core_name: String::from("fceumm"),
            status_message: String::from("Playing Test Game (core: fceumm)"),
            active_core_note: None,
            cover_path: Some(String::from("/covers/nes/test.png")),
            preview_poster_path: None,
        }
    }

    #[test]
    fn queued_launch_waits_for_shell_paint_before_loading() {
        let mut state = PlaySessionState::default();
        state.queue_launch(launch_plan(), AppView::Library);

        assert_eq!(state.launch_phase, PlayLaunchPhase::Queued);
        assert!(!state.pending_launch_ready_to_load());

        state.mark_launch_shell_painted();

        assert!(state.pending_launch_ready_to_load());
        assert!(state.take_ready_pending_launch().is_some());
        assert!(!state.pending_launch_ready_to_load());
    }

    #[test]
    fn successful_load_waits_for_presentation_before_playing() {
        let mut state = PlaySessionState::default();

        state.begin_session(
            String::from("Playing Test Game (core: fceumm)"),
            String::from("rom-1"),
            String::from("NES"),
            String::from("fceumm"),
            AppView::Library,
        );

        assert_eq!(state.launch_phase, PlayLaunchPhase::WaitingForPresentation);
    }

    #[test]
    fn load_failure_keeps_friendly_and_technical_messages() {
        let mut state = PlaySessionState::default();

        state.fail_launch(
            Some(String::from("Test Game")),
            Some(String::from("NES")),
            Some(String::from("fceumm")),
            None,
            None,
            "Couldn’t start this game.",
            "failed to load dylib",
        );

        assert_eq!(state.launch_phase, PlayLaunchPhase::Failed);
        assert_eq!(
            state.launch_friendly_message.as_deref(),
            Some("Couldn’t start this game.")
        );
        assert_eq!(
            state.launch_detail_message.as_deref(),
            Some("failed to load dylib")
        );
    }

    #[test]
    fn presentation_timeout_warning_keeps_launch_active() {
        let mut state = PlaySessionState::default();

        state.warn_launch("Still starting...", "no frame yet");

        assert_eq!(state.launch_phase, PlayLaunchPhase::Warning);
        assert!(state.launch_shell_active());
        assert_eq!(state.launch_detail_message.as_deref(), Some("no frame yet"));
    }

    #[test]
    fn first_press_starts_hold_tracking() {
        let now = Instant::now();
        let mut state = PlaySessionState::default();

        let action = state.handle_return_input(now, true, Duration::from_millis(800));

        assert_eq!(action, HoldAction::Started);
        assert_eq!(state.return_pressed_at, Some(now));
    }

    #[test]
    fn hold_past_threshold_triggers_return() {
        let now = Instant::now();
        let mut state = PlaySessionState::default();

        let _ = state.handle_return_input(now, true, Duration::from_millis(800));
        let action = state.handle_return_input(
            now + Duration::from_millis(900),
            true,
            Duration::from_millis(800),
        );

        assert_eq!(action, HoldAction::Triggered);
        assert_eq!(state.return_pressed_at, None);
    }

    #[test]
    fn release_clears_pending_hold() {
        let now = Instant::now();
        let mut state = PlaySessionState::default();

        let _ = state.handle_return_input(now, true, Duration::from_millis(800));
        let action = state.handle_return_input(
            now + Duration::from_millis(10),
            false,
            Duration::from_millis(800),
        );

        assert_eq!(action, HoldAction::None);
        assert!(!state.return_input_held);
        assert_eq!(state.return_pressed_at, None);
    }

    #[test]
    fn held_button_does_not_retrigger_without_release() {
        let now = Instant::now();
        let mut state = PlaySessionState::default();

        let _ = state.handle_return_input(now, true, Duration::from_millis(800));
        let _ = state.handle_return_input(
            now + Duration::from_millis(900),
            true,
            Duration::from_millis(800),
        );
        let action = state.handle_return_input(
            now + Duration::from_millis(950),
            true,
            Duration::from_millis(800),
        );

        assert_eq!(action, HoldAction::None);
    }

    #[test]
    fn quick_save_rising_edge_only_triggers_once_until_release() {
        let mut state = PlaySessionState::default();

        assert!(state.consume_quick_save_press(true));
        assert!(!state.consume_quick_save_press(true));
        assert!(!state.consume_quick_save_press(false));
        assert!(state.consume_quick_save_press(true));
    }

    #[test]
    fn reset_hold_past_threshold_triggers_action() {
        let mut state = PlaySessionState::default();
        let now = Instant::now();

        let started = state.handle_reset_input(now, true, Duration::from_millis(800));
        let triggered = state.handle_reset_input(
            now + Duration::from_millis(900),
            true,
            Duration::from_millis(800),
        );

        assert_eq!(started, HoldAction::Started);
        assert_eq!(triggered, HoldAction::Triggered);
    }

    #[test]
    fn active_overlay_returns_message_until_expired_then_clears_it() {
        let now = Instant::now();
        let mut state = PlaySessionState::default();
        state.set_feedback(now, Duration::from_secs(2), "Saved");

        assert_eq!(
            state.active_overlay(now + Duration::from_secs(1)),
            Some("Saved".into())
        );
        assert_eq!(state.active_overlay(now + Duration::from_secs(3)), None);
        assert!(state.overlay_message.is_empty());
        assert_eq!(state.overlay_visible_until, None);
    }
}
