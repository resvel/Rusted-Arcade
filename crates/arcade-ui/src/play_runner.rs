use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arcade_libretro::{FrameOutput, FrameTimingSnapshot, LibretroRunHandle};
use eframe::egui;
use tracing::{info, warn};

const WORKER_MAX_CATCH_UP_FRAMES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayRunnerExecutorMode {
    WorkerThread,
    MainThreadExecutor,
    FallbackEguiPump,
}

impl PlayRunnerExecutorMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::WorkerThread => "WorkerThread",
            Self::MainThreadExecutor => "MainThreadExecutor",
            Self::FallbackEguiPump => "FallbackEguiPump",
        }
    }
}

pub(crate) enum PlayRunnerEvent {
    FrameDelivered(FrameOutput),
    PresentationReady,
    FrameStepped(PlayRunnerFrameStats),
    Failed(String),
    Stopped,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PlayRunnerFrameStats {
    pub(crate) frames: u64,
    pub(crate) missed_deadlines: u64,
    pub(crate) run_work: Duration,
    pub(crate) frame_delivery_work: Duration,
}

enum PlayRunnerCommand {
    Stop,
    Reset(Sender<Result<(), String>>),
    Serialize(Sender<Result<Option<Vec<u8>>, String>>),
    Unserialize(Vec<u8>, Sender<Result<bool, String>>),
}

pub(crate) struct PlayRunner {
    mode: PlayRunnerExecutorMode,
    command_tx: Option<Sender<PlayRunnerCommand>>,
    event_rx: Option<Receiver<PlayRunnerEvent>>,
    worker: Option<JoinHandle<()>>,
    missed_deadlines: u64,
    last_event: Option<String>,
    stopping: bool,
}

impl PlayRunner {
    pub(crate) fn start(
        mode: PlayRunnerExecutorMode,
        run_handle: LibretroRunHandle,
        frame_interval: Duration,
        ctx: &egui::Context,
    ) -> Self {
        if mode != PlayRunnerExecutorMode::WorkerThread {
            return Self {
                mode,
                command_tx: None,
                event_rx: None,
                worker: None,
                missed_deadlines: 0,
                last_event: Some(String::from("Started")),
                stopping: false,
            };
        }

        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let worker = thread::Builder::new()
            .name(String::from("arcade-play-runner"))
            .spawn(move || {
                run_worker_loop(
                    run_handle,
                    frame_interval,
                    command_rx,
                    event_tx,
                    repaint_ctx,
                );
            })
            .expect("failed to spawn play runner thread");

        Self {
            mode,
            command_tx: Some(command_tx),
            event_rx: Some(event_rx),
            worker: Some(worker),
            missed_deadlines: 0,
            last_event: Some(String::from("Started")),
            stopping: false,
        }
    }

    pub(crate) fn executor_mode(&self) -> PlayRunnerExecutorMode {
        self.mode
    }

    pub(crate) fn missed_deadlines(&self) -> u64 {
        self.missed_deadlines
    }

    pub(crate) fn last_event_label(&self) -> Option<&str> {
        self.last_event.as_deref()
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping
    }

    pub(crate) fn drain_events(&mut self) -> Vec<PlayRunnerEvent> {
        let Some(rx) = self.event_rx.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match &event {
                PlayRunnerEvent::FrameDelivered(_) => {
                    self.last_event = Some(String::from("FrameDelivered"));
                }
                PlayRunnerEvent::PresentationReady => {
                    self.last_event = Some(String::from("PresentationReady"));
                }
                PlayRunnerEvent::FrameStepped(stats) => {
                    self.missed_deadlines = stats.missed_deadlines;
                    self.last_event = Some(String::from("FrameStepped"));
                }
                PlayRunnerEvent::Failed(_) => {
                    self.last_event = Some(String::from("Failed"));
                }
                PlayRunnerEvent::Stopped => {
                    self.last_event = Some(String::from("Stopped"));
                    self.stopping = false;
                }
            }
            events.push(event);
        }
        events
    }

    pub(crate) fn stop(&mut self) {
        self.stopping = true;
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(PlayRunnerCommand::Stop);
        }
        if let Some(worker) = self.worker.take() {
            if let Err(err) = worker.join() {
                warn!("play runner worker panicked: {err:?}");
            }
        }
        self.last_event = Some(String::from("Stopped"));
        self.stopping = false;
    }

    pub(crate) fn reset(
        &mut self,
        host_reset: impl FnOnce() -> anyhow::Result<()>,
    ) -> Result<(), String> {
        if let Some(tx) = self.command_tx.as_ref() {
            let (reply_tx, reply_rx) = mpsc::channel();
            tx.send(PlayRunnerCommand::Reset(reply_tx))
                .map_err(|err| format!("runner reset command failed: {err}"))?;
            return reply_rx
                .recv()
                .map_err(|err| format!("runner reset reply failed: {err}"))?;
        }
        host_reset().map_err(|err| err.to_string())
    }

    pub(crate) fn serialize_state(
        &mut self,
        host_serialize: impl FnOnce() -> anyhow::Result<Option<Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>, String> {
        if let Some(tx) = self.command_tx.as_ref() {
            let (reply_tx, reply_rx) = mpsc::channel();
            tx.send(PlayRunnerCommand::Serialize(reply_tx))
                .map_err(|err| format!("runner serialize command failed: {err}"))?;
            return reply_rx
                .recv()
                .map_err(|err| format!("runner serialize reply failed: {err}"))?;
        }
        host_serialize().map_err(|err| err.to_string())
    }

    pub(crate) fn unserialize_state(
        &mut self,
        bytes: Vec<u8>,
        host_unserialize: impl FnOnce(&[u8]) -> anyhow::Result<bool>,
    ) -> Result<bool, String> {
        if let Some(tx) = self.command_tx.as_ref() {
            let (reply_tx, reply_rx) = mpsc::channel();
            tx.send(PlayRunnerCommand::Unserialize(bytes, reply_tx))
                .map_err(|err| format!("runner unserialize command failed: {err}"))?;
            return reply_rx
                .recv()
                .map_err(|err| format!("runner unserialize reply failed: {err}"))?;
        }
        host_unserialize(&bytes).map_err(|err| err.to_string())
    }
}

impl Drop for PlayRunner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_worker_loop(
    run_handle: LibretroRunHandle,
    frame_interval: Duration,
    command_rx: Receiver<PlayRunnerCommand>,
    event_tx: Sender<PlayRunnerEvent>,
    repaint_ctx: egui::Context,
) {
    let mut clock = PlayRunnerClock::new(frame_interval);
    info!(
        target: "arcade_ui::perf",
        executor_mode = PlayRunnerExecutorMode::WorkerThread.label(),
        target_fps = clock.target_fps(),
        "play_runner started"
    );

    loop {
        if handle_pending_commands(&run_handle, &command_rx, &event_tx) {
            let _ = event_tx.send(PlayRunnerEvent::Stopped);
            repaint_ctx.request_repaint();
            return;
        }

        let wait = clock.time_until_next_frame(Instant::now());
        if !wait.is_zero() {
            match command_rx.recv_timeout(wait) {
                Ok(command) => {
                    if handle_command(&run_handle, command, &event_tx) {
                        let _ = event_tx.send(PlayRunnerEvent::Stopped);
                        repaint_ctx.request_repaint();
                        return;
                    }
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = event_tx.send(PlayRunnerEvent::Stopped);
                    repaint_ctx.request_repaint();
                    return;
                }
            }
        }

        let stats = run_one_worker_frame(&run_handle, &mut clock);
        match stats {
            Ok((frame, stats, presentation_ready)) => {
                if let Some(frame) = frame {
                    let _ = event_tx.send(PlayRunnerEvent::FrameDelivered(frame));
                }
                if presentation_ready {
                    let _ = event_tx.send(PlayRunnerEvent::PresentationReady);
                }
                let _ = event_tx.send(PlayRunnerEvent::FrameStepped(stats));
                if stats.frames % 60 == 0 {
                    info!(
                        target: "arcade_ui::perf",
                        executor_mode = PlayRunnerExecutorMode::WorkerThread.label(),
                        frames = stats.frames,
                        missed_deadlines = stats.missed_deadlines,
                        run_work_ms = stats.run_work.as_secs_f64() * 1000.0,
                        frame_delivery_ms = stats.frame_delivery_work.as_secs_f64() * 1000.0,
                        "play_runner tick"
                    );
                }
                repaint_ctx.request_repaint();
            }
            Err(err) => {
                let _ = event_tx.send(PlayRunnerEvent::Failed(err));
                repaint_ctx.request_repaint();
                return;
            }
        }
    }
}

fn handle_pending_commands(
    run_handle: &LibretroRunHandle,
    command_rx: &Receiver<PlayRunnerCommand>,
    event_tx: &Sender<PlayRunnerEvent>,
) -> bool {
    while let Ok(command) = command_rx.try_recv() {
        if handle_command(run_handle, command, event_tx) {
            return true;
        }
    }
    false
}

fn handle_command(
    run_handle: &LibretroRunHandle,
    command: PlayRunnerCommand,
    _event_tx: &Sender<PlayRunnerEvent>,
) -> bool {
    match command {
        PlayRunnerCommand::Stop => true,
        PlayRunnerCommand::Reset(reply_tx) => {
            let _ = reply_tx.send(run_handle.reset().map_err(|err| err.to_string()));
            false
        }
        PlayRunnerCommand::Serialize(reply_tx) => {
            let _ = reply_tx.send(run_handle.serialize_state().map_err(|err| err.to_string()));
            false
        }
        PlayRunnerCommand::Unserialize(bytes, reply_tx) => {
            let _ = reply_tx.send(
                run_handle
                    .unserialize_state(&bytes)
                    .map_err(|err| err.to_string()),
            );
            false
        }
    }
}

fn run_one_worker_frame(
    run_handle: &LibretroRunHandle,
    clock: &mut PlayRunnerClock,
) -> Result<(Option<FrameOutput>, PlayRunnerFrameStats, bool), String> {
    let before = run_handle.frame_timing_snapshot();
    let frame = run_handle.run_frame().map_err(|err| err.to_string())?;
    let after = run_handle.frame_timing_snapshot();
    let stats = clock.finish_frame(before, after);
    let status = run_handle.presentation_status();
    let presentation_ready = status.external_present_active
        || status.external_present_deliveries > 0
        || status.cpu_frame_deliveries > 0
        || status.gl_texture_deliveries > 0;
    Ok((frame, stats, presentation_ready))
}

#[derive(Debug, Clone)]
pub(crate) struct PlayRunnerClock {
    frame_interval: Duration,
    next_deadline: Instant,
    frames: u64,
    missed_deadlines: u64,
    catch_up_frames: u32,
}

impl PlayRunnerClock {
    pub(crate) fn new(frame_interval: Duration) -> Self {
        let frame_interval = if frame_interval.is_zero() {
            Duration::from_secs_f64(1.0 / 60.0)
        } else {
            frame_interval
        };
        Self {
            frame_interval,
            next_deadline: Instant::now() + frame_interval,
            frames: 0,
            missed_deadlines: 0,
            catch_up_frames: 0,
        }
    }

    pub(crate) fn target_fps(&self) -> f64 {
        1.0 / self.frame_interval.as_secs_f64()
    }

    pub(crate) fn time_until_next_frame(&self, now: Instant) -> Duration {
        self.next_deadline.saturating_duration_since(now)
    }

    fn finish_frame(
        &mut self,
        before: FrameTimingSnapshot,
        after: FrameTimingSnapshot,
    ) -> PlayRunnerFrameStats {
        self.frames = self.frames.saturating_add(1);
        let now = Instant::now();
        self.next_deadline += self.frame_interval;
        if now > self.next_deadline {
            let late = now.duration_since(self.next_deadline);
            let missed = (late.as_nanos() / self.frame_interval.as_nanos().max(1)) as u64 + 1;
            self.missed_deadlines = self.missed_deadlines.saturating_add(missed);
            self.catch_up_frames = self.catch_up_frames.saturating_add(1);
            if self.catch_up_frames >= WORKER_MAX_CATCH_UP_FRAMES {
                self.next_deadline = now + self.frame_interval;
                self.catch_up_frames = 0;
            }
        } else {
            self.catch_up_frames = 0;
        }

        PlayRunnerFrameStats {
            frames: self.frames,
            missed_deadlines: self.missed_deadlines,
            run_work: Duration::from_micros(after.run_total_us.saturating_sub(before.run_total_us)),
            frame_delivery_work: Duration::from_micros(
                after
                    .frame_delivery_total_us
                    .saturating_sub(before.frame_delivery_total_us),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_clock_waits_until_first_deadline() {
        let clock = PlayRunnerClock::new(Duration::from_millis(16));
        assert!(clock.time_until_next_frame(Instant::now()) <= Duration::from_millis(16));
    }

    #[test]
    fn worker_clock_records_missed_deadlines_without_bursting() {
        let mut clock = PlayRunnerClock::new(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(3));
        let stats = clock.finish_frame(
            FrameTimingSnapshot::default(),
            FrameTimingSnapshot {
                frames: 1,
                run_total_us: 100,
                frame_delivery_total_us: 50,
                last_run_us: 100,
                last_frame_delivery_us: 50,
            },
        );

        assert_eq!(stats.frames, 1);
        assert!(stats.missed_deadlines >= 1);
        assert_eq!(stats.run_work, Duration::from_micros(100));
        assert_eq!(stats.frame_delivery_work, Duration::from_micros(50));
    }

    #[test]
    fn worker_clock_allows_bounded_catch_up_before_resetting_deadline() {
        let mut clock = PlayRunnerClock::new(Duration::from_millis(10));
        clock.next_deadline = Instant::now() - Duration::from_millis(40);

        let _ = clock.finish_frame(
            FrameTimingSnapshot::default(),
            FrameTimingSnapshot::default(),
        );
        assert!(clock.time_until_next_frame(Instant::now()).is_zero());

        let _ = clock.finish_frame(
            FrameTimingSnapshot::default(),
            FrameTimingSnapshot::default(),
        );
        assert!(clock.time_until_next_frame(Instant::now()).is_zero());

        let _ = clock.finish_frame(
            FrameTimingSnapshot::default(),
            FrameTimingSnapshot::default(),
        );
        assert!(!clock.time_until_next_frame(Instant::now()).is_zero());
    }
}
