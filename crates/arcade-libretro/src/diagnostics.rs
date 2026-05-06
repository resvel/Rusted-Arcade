use super::*;

pub(super) fn summarize_rgba_debug_pixels(pixels: &[u8]) -> (u64, usize, [u8; 4]) {
    let mut checksum = 0_u64;
    let mut non_black_pixels = 0_usize;
    let mut first_rgba = [0_u8; 4];

    for (index, pixel) in pixels.chunks_exact(4).enumerate().take(64) {
        if index == 0 {
            first_rgba.copy_from_slice(pixel);
        }
        if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
            non_black_pixels += 1;
        }
        checksum = checksum
            .wrapping_mul(16_777_619)
            .wrapping_add(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]) as u64);
    }

    (checksum, non_black_pixels, first_rgba)
}

pub(super) fn summarize_rgba_debug_pixels_grid(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> (u64, usize, [u8; 4], [u8; 4]) {
    let mut checksum = 0_u64;
    let mut non_black_samples = 0_usize;
    let mut first_rgba = [0_u8; 4];
    let mut center_rgba = [0_u8; 4];

    if width == 0 || height == 0 {
        return (checksum, non_black_samples, first_rgba, center_rgba);
    }

    let sample_width = width.min(8);
    let sample_height = height.min(8);
    for sample_y in 0..sample_height {
        let y = ((sample_y as usize * height as usize) / sample_height as usize)
            .min(height.saturating_sub(1) as usize);
        for sample_x in 0..sample_width {
            let x = ((sample_x as usize * width as usize) / sample_width as usize)
                .min(width.saturating_sub(1) as usize);
            let offset = (y * width as usize + x) * 4;
            if offset + 4 > pixels.len() {
                continue;
            }
            let pixel = &pixels[offset..offset + 4];
            if sample_x == 0 && sample_y == 0 {
                first_rgba.copy_from_slice(pixel);
            }
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                non_black_samples += 1;
            }
            checksum = checksum
                .wrapping_mul(16_777_619)
                .wrapping_add(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]) as u64);
        }
    }

    let center_x = (width / 2) as usize;
    let center_y = (height / 2) as usize;
    let center_offset = (center_y * width as usize + center_x) * 4;
    if center_offset + 4 <= pixels.len() {
        center_rgba.copy_from_slice(&pixels[center_offset..center_offset + 4]);
    }

    (checksum, non_black_samples, first_rgba, center_rgba)
}

pub(super) fn normalize_display_fps(fps: f64) -> f64 {
    if !fps.is_finite() || fps <= 0.0 {
        return fps;
    }

    // Normalize only near the most common stable presentation rates. This keeps console-style
    // systems from feeling slightly fast while preserving unusual native arcade timings.
    for target in COMMON_DISPLAY_FPS {
        if (fps - target).abs() <= DISPLAY_FPS_TOLERANCE {
            return *target;
        }
    }

    fps
}

pub(super) fn reset_vulkan_present_metrics(runtime: &HostRuntime) {
    *runtime.vulkan_present_metrics.lock() = VulkanPresentMetricsState::default();
}

pub(super) fn record_vulkan_queue_present_attempt(runtime: &HostRuntime) {
    let mut state = runtime.vulkan_present_metrics.lock();
    state.queue_present_attempts = state.queue_present_attempts.saturating_add(1);
}

pub(super) fn record_vulkan_queue_present_success(runtime: &HostRuntime) {
    let mut state = runtime.vulkan_present_metrics.lock();
    state.queue_present_successes = state.queue_present_successes.saturating_add(1);
}

pub(super) fn record_vulkan_source_non_black_sample(
    runtime: &HostRuntime,
    source_non_black_seen: bool,
) {
    let mut state = runtime.vulkan_present_metrics.lock();
    state.source_sample_checks = state.source_sample_checks.saturating_add(1);
    if source_non_black_seen {
        state.source_non_black_seen = true;
        // If output recovered from all-black, clear stale black fail-fast diagnostics.
        state.black_fail_fast_error = None;
    }
}

pub(super) fn record_vulkan_swapchain_non_black_sample(
    runtime: &HostRuntime,
    swapchain_non_black_seen: bool,
) {
    let mut state = runtime.vulkan_present_metrics.lock();
    state.swapchain_sample_checks = state.swapchain_sample_checks.saturating_add(1);
    if swapchain_non_black_seen {
        state.swapchain_non_black_seen = true;
        // If output recovered from all-black, clear stale black fail-fast diagnostics.
        state.black_fail_fast_error = None;
    }
}

pub(super) fn record_vulkan_source_frame_size(runtime: &HostRuntime, width: u32, height: u32) {
    let mut state = runtime.vulkan_present_metrics.lock();
    if width > 1 && height > 1 {
        state.non_tiny_source_frame_seen = true;
        state.consecutive_tiny_source_frames = 0;
        // If the source recovered from tiny startup frames, clear stale tiny-frame diagnostics.
        state.tiny_frame_fail_fast_error = None;
        return;
    }

    state.consecutive_tiny_source_frames = state.consecutive_tiny_source_frames.saturating_add(1);
    state.max_consecutive_tiny_source_frames = state
        .max_consecutive_tiny_source_frames
        .max(state.consecutive_tiny_source_frames);
}

pub(super) fn has_vulkan_present_fail_fast(state: &VulkanPresentMetricsState) -> bool {
    state.black_fail_fast_error.is_some() || state.tiny_frame_fail_fast_error.is_some()
}

fn maybe_mark_vulkan_black_fail_fast(state: &mut VulkanPresentMetricsState) {
    if has_vulkan_present_fail_fast(state) {
        return;
    }
    let threshold = vulkan_black_fail_fast_threshold_frames();
    if threshold == 0
        || state.external_present_deliveries < threshold
        || state.queue_present_successes == 0
        || state.source_sample_checks == 0
        || state.swapchain_sample_checks == 0
        || state.source_non_black_seen
        || state.swapchain_non_black_seen
    {
        return;
    }

    state.black_fail_fast_error = Some(format!(
        "Vulkan external-present fail-fast: output remained black for {} external-present frames (queue_present_attempts={}, queue_present_successes={}, source_non_black_seen={}, swapchain_non_black_seen={}, consecutive_tiny_source_frames={}, max_consecutive_tiny_source_frames={}); no automatic fallback will be attempted",
        state.external_present_deliveries,
        state.queue_present_attempts,
        state.queue_present_successes,
        state.source_non_black_seen,
        state.swapchain_non_black_seen,
        state.consecutive_tiny_source_frames,
        state.max_consecutive_tiny_source_frames
    ));
}

fn maybe_mark_vulkan_tiny_frame_fail_fast(state: &mut VulkanPresentMetricsState) {
    if has_vulkan_present_fail_fast(state) {
        return;
    }
    let threshold = vulkan_tiny_frame_fail_fast_threshold_frames();
    if threshold == 0
        || state.external_present_deliveries < threshold
        || state.queue_present_successes == 0
        || state.non_tiny_source_frame_seen
        || state.consecutive_tiny_source_frames < threshold
    {
        return;
    }

    state.tiny_frame_fail_fast_error = Some(format!(
        "Vulkan external-present fail-fast: source frame size remained tiny (<=1x1) for {} consecutive frames (external_present_deliveries={}, queue_present_attempts={}, queue_present_successes={}, source_non_black_seen={}, swapchain_non_black_seen={}, max_consecutive_tiny_source_frames={}); no automatic fallback will be attempted",
        state.consecutive_tiny_source_frames,
        state.external_present_deliveries,
        state.queue_present_attempts,
        state.queue_present_successes,
        state.source_non_black_seen,
        state.swapchain_non_black_seen,
        state.max_consecutive_tiny_source_frames
    ));
}

pub(super) fn record_frame_delivery_metrics(
    runtime: &HostRuntime,
    using_vulkan_hw_render: bool,
    delivery: &FrameDelivery,
) {
    let mut state = runtime.vulkan_present_metrics.lock();
    match delivery {
        FrameDelivery::ExternalPresent => {
            if !using_vulkan_hw_render {
                return;
            }
            state.external_present_deliveries = state.external_present_deliveries.saturating_add(1);
            maybe_mark_vulkan_tiny_frame_fail_fast(&mut state);
            maybe_mark_vulkan_black_fail_fast(&mut state);
        }
        FrameDelivery::CpuFrame(_) => {
            state.cpu_frame_deliveries = state.cpu_frame_deliveries.saturating_add(1);
        }
        FrameDelivery::GlTexture(_) => {
            state.gl_texture_deliveries = state.gl_texture_deliveries.saturating_add(1);
        }
        #[cfg(target_os = "macos")]
        FrameDelivery::MacosIosurface(_) => {
            state.gl_texture_deliveries = state.gl_texture_deliveries.saturating_add(1);
        }
        FrameDelivery::NoFrame | FrameDelivery::Error(_) => {}
    }
}

pub(super) fn record_frame_timing(
    runtime: &HostRuntime,
    run_duration: std::time::Duration,
    frame_delivery_duration: std::time::Duration,
) {
    let run_us = duration_to_us(run_duration);
    let frame_delivery_us = duration_to_us(frame_delivery_duration);
    let mut timing = runtime.frame_timing.lock();
    timing.frames = timing.frames.saturating_add(1);
    timing.run_total_us = timing.run_total_us.saturating_add(run_us);
    timing.frame_delivery_total_us = timing
        .frame_delivery_total_us
        .saturating_add(frame_delivery_us);
    timing.last_run_us = run_us;
    timing.last_frame_delivery_us = frame_delivery_us;
}

fn duration_to_us(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

pub(super) fn vulkan_present_fail_fast_error(runtime: &HostRuntime) -> Option<String> {
    let _ = runtime;
    // Fail-fast gating is intentionally disabled for now so unhealthy runs
    // continue and expose full diagnostic behavior instead of terminating early.
    None
}

pub(super) fn log_vulkan_present_metrics_summary(
    runtime: &HostRuntime,
    core_label: &str,
    core_uses_hw_render: bool,
    phase: &str,
) {
    let backend_kind = runtime.video_coordinator.lock().current_backend_kind();
    let external_window_created = runtime
        .hw_render_state
        .lock()
        .external_vulkan_window
        .is_some();
    let snapshot = runtime.vulkan_present_metrics.lock().clone();
    let fail_fast_error = snapshot
        .black_fail_fast_error
        .as_deref()
        .or(snapshot.tiny_frame_fail_fast_error.as_deref())
        .unwrap_or("none");

    let should_log = core_uses_hw_render
        || backend_kind == VideoBackendKind::Vulkan
        || external_window_created
        || snapshot.queue_present_attempts > 0
        || snapshot.queue_present_successes > 0
        || snapshot.external_present_deliveries > 0
        || snapshot.cpu_frame_deliveries > 0
        || snapshot.gl_texture_deliveries > 0
        || snapshot.source_sample_checks > 0
        || snapshot.swapchain_sample_checks > 0
        || vulkan_test_metrics_enabled();
    if !should_log {
        return;
    }

    info!(
        target: "arcade_libretro::vulkan_metrics",
        phase,
        core = core_label,
        backend = ?backend_kind,
        core_uses_hw_render,
        external_window_created,
        queue_present_attempts = snapshot.queue_present_attempts,
        queue_present_successes = snapshot.queue_present_successes,
        external_present_deliveries = snapshot.external_present_deliveries,
        cpu_frame_deliveries = snapshot.cpu_frame_deliveries,
        gl_texture_deliveries = snapshot.gl_texture_deliveries,
        source_non_black_seen = snapshot.source_non_black_seen,
        swapchain_non_black_seen = snapshot.swapchain_non_black_seen,
        non_tiny_source_frame_seen = snapshot.non_tiny_source_frame_seen,
        consecutive_tiny_source_frames = snapshot.consecutive_tiny_source_frames,
        max_consecutive_tiny_source_frames = snapshot.max_consecutive_tiny_source_frames,
        source_sample_checks = snapshot.source_sample_checks,
        swapchain_sample_checks = snapshot.swapchain_sample_checks,
        fail_fast_error,
        "vulkan_present_metrics"
    );
}
