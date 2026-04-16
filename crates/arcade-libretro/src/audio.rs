use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

// Limit the number of silence warnings to avoid log spam.
static SILENCE_WARN_COUNT: AtomicUsize = AtomicUsize::new(0);
static UNDERFLOW_WARN_COUNT: AtomicUsize = AtomicUsize::new(0);
// Limit "runtime not available" warnings (fires after core unload while the
// cpal stream is still draining).
static NO_RUNTIME_WARN_COUNT: AtomicUsize = AtomicUsize::new(0);

fn audio_debug_enabled() -> bool {
    match std::env::var("ARCADE_AUDIO_DEBUG") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

#[cfg(feature = "audio")]
pub(super) struct AudioOutput {
    pub(super) _stream: cpal::Stream,
    pub(super) sample_rate_hz: u32,
}

#[cfg(feature = "audio")]
impl AudioOutput {
    pub(super) fn new(preferred_sample_rate_hz: Option<u32>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output device found"))?;
        let supported_config = select_output_config(&device, preferred_sample_rate_hz)?;
        let channels = supported_config.channels() as usize;
        let mut stream_config: cpal::StreamConfig = supported_config.config();
        let sample_format = supported_config.sample_format();
        let sample_rate_hz = stream_config.sample_rate.0;
        let custom_buffer_size = pick_buffer_size(&supported_config, sample_rate_hz);
        if let Some(buffer_size) = custom_buffer_size {
            stream_config.buffer_size = buffer_size;
        }
        set_audio_output_sample_rate(sample_rate_hz as f64);
        NO_RUNTIME_WARN_COUNT.store(0, Ordering::Relaxed);
        SILENCE_WARN_COUNT.store(0, Ordering::Relaxed);
        let stream = match build_output_stream(&device, sample_format, &stream_config, channels) {
            Ok(stream) => stream,
            Err(err) if custom_buffer_size.is_some() => {
                eprintln!(
                    "Custom audio buffer size rejected ({err}); retrying with backend default."
                );
                stream_config.buffer_size = cpal::BufferSize::Default;
                build_output_stream(&device, sample_format, &stream_config, channels)?
            }
            Err(err) => return Err(err),
        };

        stream.play()?;
        Ok(Self {
            _stream: stream,
            sample_rate_hz,
        })
    }
}

#[cfg(feature = "audio")]
fn pick_buffer_size(
    supported_config: &cpal::SupportedStreamConfig,
    sample_rate_hz: u32,
) -> Option<cpal::BufferSize> {
    match supported_config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            // Prefer a buffer size based on the new AUDIO_TARGET_LATENCY_SECS.
            let target =
                (sample_rate_hz as f64 * AUDIO_TARGET_LATENCY_SECS).clamp(256.0, 2048.0) as u32;
            Some(cpal::BufferSize::Fixed(target.clamp(*min, *max)))
        }
        _ => None,
    }
}

#[cfg(feature = "audio")]
fn build_output_stream(
    device: &cpal::Device,
    sample_format: cpal::SampleFormat,
    stream_config: &cpal::StreamConfig,
    channels: usize,
) -> Result<cpal::Stream> {
    fn err_fn(err: cpal::StreamError) {
        eprintln!("cpal output stream error: {err}");
    }

    let stream = match sample_format {
        cpal::SampleFormat::I16 => device.build_output_stream(
            stream_config,
            move |data: &mut [i16], _| write_output_i16(data, channels),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            stream_config,
            move |data: &mut [u16], _| write_output_u16(data, channels),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _| write_output_f32(data, channels),
            err_fn,
            None,
        )?,
        other => {
            return Err(anyhow!("unsupported cpal sample format: {other:?}"));
        }
    };

    Ok(stream)
}

#[cfg(feature = "audio")]
fn select_output_config(
    device: &cpal::Device,
    preferred_sample_rate_hz: Option<u32>,
) -> Result<cpal::SupportedStreamConfig> {
    let default_config = device.default_output_config()?;
    let Some(target_rate) = preferred_sample_rate_hz.filter(|rate| *rate > 0) else {
        return Ok(default_config);
    };

    let default_channels = default_config.channels();
    let mut best: Option<(u64, cpal::SupportedStreamConfig)> = None;

    for range in device.supported_output_configs()? {
        let min_rate = range.min_sample_rate().0;
        let max_rate = range.max_sample_rate().0;
        let selected_rate = target_rate.clamp(min_rate, max_rate);
        let config = range.with_sample_rate(cpal::SampleRate(selected_rate));

        let rate_penalty = u32::abs_diff(selected_rate, target_rate) as u64 * 1000;
        let channel_penalty = u16::abs_diff(config.channels(), default_channels) as u64 * 50;
        let format_penalty = if config.sample_format() == cpal::SampleFormat::I16 {
            0
        } else if config.sample_format() == default_config.sample_format() {
            500_000
        } else {
            10_000_000
        };
        let score = rate_penalty + channel_penalty + format_penalty;

        if best
            .as_ref()
            .map(|(best_score, _)| score < *best_score)
            .unwrap_or(true)
        {
            best = Some((score, config));
            if score == 0 {
                break;
            }
        }
    }

    Ok(best.map(|(_, config)| config).unwrap_or(default_config))
}

#[cfg(not(feature = "audio"))]
pub(super) struct AudioOutput {
    pub(super) sample_rate_hz: u32,
}

#[cfg(not(feature = "audio"))]
impl AudioOutput {
    pub(super) fn new(_preferred_sample_rate_hz: Option<u32>) -> Result<Self> {
        Err(anyhow!("audio feature is disabled at compile time"))
    }
}

#[cfg(feature = "audio")]
pub(super) fn set_audio_source_sample_rate(sample_rate: f64) {
    let _ = with_active_runtime(|runtime| {
        let mut state = runtime.audio_state.lock();
        state.source_sample_rate = sample_rate;
        state.resample_phase = 0.0;
        state.current_frame = None;
        state.next_frame = None;
    });
}

#[cfg(not(feature = "audio"))]
pub(super) fn set_audio_source_sample_rate(_sample_rate: f64) {}

#[cfg(feature = "audio")]
pub(super) fn set_audio_output_sample_rate(sample_rate: f64) {
    let _ = with_active_runtime(|runtime| {
        let mut state = runtime.audio_state.lock();
        state.output_sample_rate = sample_rate;
        state.resample_phase = 0.0;
        state.current_frame = None;
        state.next_frame = None;
    });
}

pub(super) unsafe extern "C" fn retro_audio_sample(left: i16, right: i16) {
    push_audio_samples(&[left, right]);
}

pub(super) unsafe extern "C" fn retro_audio_sample_batch(data: *const i16, frames: usize) -> usize {
    if data.is_null() || frames == 0 {
        return 0;
    }

    let sample_count = frames.saturating_mul(2);
    let samples = unsafe { std::slice::from_raw_parts(data, sample_count) };
    push_audio_samples(samples);
    frames
}

fn push_audio_samples(samples: &[i16]) {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static PUSH_COUNT: AtomicU64 = AtomicU64::new(0);
    static LOGGED_FIRST_NONZERO: AtomicBool = AtomicBool::new(false);
    static LAST_PUSH_SILENT: AtomicBool = AtomicBool::new(true);

    if samples.is_empty() {
        warn!(
            target: "arcade_libretro::audio",
            "push_audio_samples called with empty slice"
        );
        return;
    }

    let Some(runtime) = active_runtime() else {
        return;
    };

    // Diagnostic: log first batch with non-zero samples so we can confirm
    // the core is actually producing audio data.
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let nonzero_count = samples.iter().filter(|s| **s != 0).count();
    let is_silent = nonzero_count == 0;
    let n = PUSH_COUNT.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        info!(
            target: "arcade_libretro::audio",
            call_index = n,
            sample_len = samples.len(),
            peak,
            nonzero_count,
            silent = is_silent,
            "first audio input batch"
        );
    }
    if peak > 0 && !LOGGED_FIRST_NONZERO.swap(true, Ordering::Relaxed) {
        info!(
            target: "arcade_libretro::audio",
            call_index = n,
            sample_len = samples.len(),
            peak,
            nonzero_count,
            "audio input became non-silent"
        );
    }

    let mut state = runtime.audio_state.lock();
    let queue_before = state.samples.len();
    let queue = &mut state.samples;
    let overflow = queue
        .len()
        .saturating_add(samples.len())
        .saturating_sub(MAX_AUDIO_SAMPLES);
    for _ in 0..overflow {
        let _ = queue.pop_front();
    }
    queue.extend(samples.iter().copied());
    let queue_after = queue.len();

    let previous_silent = LAST_PUSH_SILENT.swap(is_silent, Ordering::Relaxed);
    let debug_enabled = audio_debug_enabled();
    if overflow > 0 || (debug_enabled && (previous_silent != is_silent || n % 500 == 0)) {
        info!(
            target: "arcade_libretro::audio",
            call_index = n,
            sample_len = samples.len(),
            peak,
            nonzero_count,
            silent = is_silent,
            queue_before,
            queue_after,
            overflow,
            "audio input batch"
        );
    }
}

#[cfg(feature = "audio")]
fn audio_frames_for_latency(sample_rate_hz: f64, latency_secs: f64) -> usize {
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 || latency_secs <= 0.0 {
        return 0;
    }

    (sample_rate_hz * latency_secs).round() as usize
}

#[cfg(feature = "audio")]
fn trim_audio_queue_for_latency(state: &mut AudioState) {
    let output_rate = state.output_sample_rate;
    if output_rate <= 0.0 {
        warn!(
            target: "arcade_libretro::audio",
            "output_rate <= 0.0 in trim_audio_queue_for_latency"
        );
        return;
    }

    let target_frames = audio_frames_for_latency(output_rate, AUDIO_TARGET_LATENCY_SECS);
    let max_frames =
        audio_frames_for_latency(output_rate, AUDIO_MAX_LATENCY_SECS).max(target_frames + 1);
    let queued_frames = state.samples.len() / 2;
    if queued_frames <= max_frames {
        return;
    }

    let frames_to_drop = queued_frames.saturating_sub(target_frames);
    let samples_to_drop = frames_to_drop.saturating_mul(2).min(state.samples.len());
    let queue_before = state.samples.len();
    state.samples.drain(..samples_to_drop);
    state.current_frame = None;
    state.next_frame = None;
    state.resample_phase = 0.0;
    if audio_debug_enabled() || samples_to_drop > 0 {
        info!(
            target: "arcade_libretro::audio",
            queue_before,
            queue_after = state.samples.len(),
            dropped_samples = samples_to_drop,
            dropped_frames = frames_to_drop,
            target_frames,
            max_frames,
            "trimmed audio queue for latency"
        );
    }
}

#[cfg(feature = "audio")]
fn pop_stereo_frame(state: &mut AudioState) -> (i16, i16) {
    let queue_before = state.samples.len();
    let underflow = queue_before < 2;
    let left = state.samples.pop_front().unwrap_or(0);
    let right = state.samples.pop_front().unwrap_or(0);
    if underflow {
        if UNDERFLOW_WARN_COUNT.fetch_add(1, Ordering::Relaxed) < 20 {
            eprintln!(
                "[AUDIO-RS] underflow: q_before={queue_before} produced=({}, {}) q_after={}",
                left,
                right,
                state.samples.len(),
            );
        }
    }
    if left == 0 && right == 0 {
        if SILENCE_WARN_COUNT.fetch_add(1, Ordering::Relaxed) < 10 {
            eprintln!(
                "[AUDIO-RS] warning: pop_stereo_frame returned silence (underflow={underflow} q_before={queue_before} q_after={})",
                state.samples.len(),
            );
        }
    }
    (left, right)
}

#[cfg(feature = "audio")]
fn pop_stereo_i16_with_state(state: &mut AudioState) -> (i16, i16) {
    let source_rate = state.source_sample_rate;
    let output_rate = state.output_sample_rate;

    if source_rate <= 0.0 || output_rate <= 0.0 {
        state.current_frame = None;
        state.next_frame = None;
        state.resample_phase = 0.0;
        warn!(
            target: "arcade_libretro::audio",
            source_rate,
            output_rate,
            "source_rate or output_rate <= 0.0 in pop_stereo_i16_with_state"
        );
        return pop_stereo_frame(state);
    }

    let target_queue_frames =
        audio_frames_for_latency(output_rate, AUDIO_TARGET_LATENCY_SECS) as f64;
    let queued_frames = (state.samples.len() / 2) as f64;
    let queue_error = if target_queue_frames > 0.0 {
        ((queued_frames - target_queue_frames) / target_queue_frames).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let base_ratio = source_rate / output_rate;
    let mut ratio = base_ratio * (1.0 + queue_error * AUDIO_RESAMPLE_QUEUE_CORRECTION);
    let ratio_min = base_ratio * (1.0 - AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT);
    let ratio_max = base_ratio * (1.0 + AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT);
    ratio = ratio.clamp(ratio_min, ratio_max);

    let near_unity_ratio = (ratio - 1.0).abs() < 0.0005;
    if near_unity_ratio && state.current_frame.is_none() && state.next_frame.is_none() {
        state.resample_phase = 0.0;
        if audio_debug_enabled() {
            info!(
                target: "arcade_libretro::audio",
                ratio,
                base_ratio,
                "near_unity_ratio with empty interpolation state"
            );
        }
        return pop_stereo_frame(state);
    }

    if state.current_frame.is_none() {
        state.current_frame = Some(pop_stereo_frame(state));
    }
    if state.next_frame.is_none() {
        state.next_frame = Some(pop_stereo_frame(state));
    }

    let (current_left, current_right) = state.current_frame.unwrap_or((0, 0));
    let (next_left, next_right) = state.next_frame.unwrap_or((0, 0));
    let frac = state.resample_phase as f32;

    let left = current_left as f32 + (next_left as f32 - current_left as f32) * frac;
    let right = current_right as f32 + (next_right as f32 - current_right as f32) * frac;

    state.resample_phase += ratio;
    while state.resample_phase >= 1.0 {
        state.current_frame = state.next_frame.take();
        state.next_frame = Some(pop_stereo_frame(state));
        state.resample_phase -= 1.0;
    }

    (left.round() as i16, right.round() as i16)
}

#[cfg(feature = "audio")]
fn write_output_i16(output: &mut [i16], channels: usize) {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static LOGGED_FORMAT: AtomicBool = AtomicBool::new(false);
    static CB_COUNT: AtomicU64 = AtomicU64::new(0);
    static LAST_CB_SILENT: AtomicBool = AtomicBool::new(true);

    if channels == 0 {
        return;
    }

    if !LOGGED_FORMAT.swap(true, Ordering::Relaxed) {
        info!(
            target: "arcade_libretro::audio",
            format = "i16",
            channels,
            buffer_len = output.len(),
            "audio output callback initialized"
        );
    }

    let Some(runtime) = active_runtime() else {
        output.fill(0);
        if NO_RUNTIME_WARN_COUNT.fetch_add(1, Ordering::Relaxed) < 3 {
            warn!(
                target: "arcade_libretro::audio",
                "audio runtime not available in i16 callback; filling silence"
            );
        }
        return;
    };
    let mut state = runtime.audio_state.lock();
    let queue_before = state.samples.len();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            frame[0] = ((left as i32 + right as i32) / 2) as i16;
            continue;
        }

        frame[0] = left;
        frame[1] = right;
        for sample in &mut frame[2..] {
            *sample = 0;
        }
    }

    let n = CB_COUNT.fetch_add(1, Ordering::Relaxed);
    let out_peak: u16 = output.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let nonzero_out = output.iter().filter(|sample| **sample != 0).count();
    let is_silent = nonzero_out == 0;
    let previous_silent = LAST_CB_SILENT.swap(is_silent, Ordering::Relaxed);
    let debug_enabled = audio_debug_enabled();
    if n == 0 || (debug_enabled && (previous_silent != is_silent || n % 100 == 0)) {
        info!(
            target: "arcade_libretro::audio",
            callback_index = n,
            queue_before,
            queue_after = state.samples.len(),
            out_peak,
            nonzero_out,
            silent = is_silent,
            source_rate = state.source_sample_rate,
            output_rate = state.output_sample_rate,
            "audio output callback (i16)"
        );
    }
}

#[cfg(feature = "audio")]
fn write_output_u16(output: &mut [u16], channels: usize) {
    if channels == 0 {
        return;
    }

    let Some(runtime) = active_runtime() else {
        output.fill(i16::MAX as u16);
        if NO_RUNTIME_WARN_COUNT.fetch_add(1, Ordering::Relaxed) < 3 {
            warn!(
                target: "arcade_libretro::audio",
                "audio runtime not available in u16 callback; filling silence"
            );
        }
        return;
    };
    let mut state = runtime.audio_state.lock();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            let mixed = ((left as i32 + right as i32) / 2) as i16;
            frame[0] = (mixed as i32 + i16::MAX as i32 + 1) as u16;
            continue;
        }

        frame[0] = (left as i32 + i16::MAX as i32 + 1) as u16;
        frame[1] = (right as i32 + i16::MAX as i32 + 1) as u16;
        for sample in &mut frame[2..] {
            *sample = i16::MAX as u16;
        }
    }
}

#[cfg(feature = "audio")]
fn write_output_f32(output: &mut [f32], channels: usize) {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static LOGGED_FORMAT: AtomicBool = AtomicBool::new(false);
    static CB_COUNT: AtomicU64 = AtomicU64::new(0);
    static LAST_CB_SILENT: AtomicBool = AtomicBool::new(true);

    if channels == 0 {
        return;
    }

    if !LOGGED_FORMAT.swap(true, Ordering::Relaxed) {
        info!(
            target: "arcade_libretro::audio",
            format = "f32",
            channels,
            buffer_len = output.len(),
            "audio output callback initialized"
        );
    }

    let Some(runtime) = active_runtime() else {
        output.fill(0.0);
        if NO_RUNTIME_WARN_COUNT.fetch_add(1, Ordering::Relaxed) < 3 {
            warn!(
                target: "arcade_libretro::audio",
                "audio runtime not available in f32 callback; filling silence"
            );
        }
        return;
    };
    let mut state = runtime.audio_state.lock();
    let queue_before = state.samples.len();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            frame[0] = (left as f32 + right as f32) / 65536.0;
            continue;
        }

        frame[0] = left as f32 / 32768.0;
        frame[1] = right as f32 / 32768.0;
        for sample in &mut frame[2..] {
            *sample = 0.0;
        }
    }

    let n = CB_COUNT.fetch_add(1, Ordering::Relaxed);
    let out_peak: f32 = output.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let nonzero_out = output
        .iter()
        .filter(|sample| sample.abs() > f32::EPSILON)
        .count();
    let is_silent = nonzero_out == 0;
    let previous_silent = LAST_CB_SILENT.swap(is_silent, Ordering::Relaxed);
    let debug_enabled = audio_debug_enabled();
    if n == 0 || (debug_enabled && (previous_silent != is_silent || n % 100 == 0)) {
        info!(
            target: "arcade_libretro::audio",
            callback_index = n,
            queue_before,
            queue_after = state.samples.len(),
            out_peak,
            nonzero_out,
            silent = is_silent,
            source_rate = state.source_sample_rate,
            output_rate = state.output_sample_rate,
            "audio output callback (f32)"
        );
    }
}
