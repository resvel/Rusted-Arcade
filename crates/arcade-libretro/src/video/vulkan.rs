use super::*;

pub(super) struct VulkanBackend;

impl VideoBackend for VulkanBackend {
    fn prepare_frame(
        &mut self,
        runtime: &HostRuntime,
        _target_size: Option<(u32, u32)>,
    ) -> Result<()> {
        if runtime.hw_render_state.lock().context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
            prepare_vulkan_sync_for_frame(runtime)?;
        }
        Ok(())
    }

    fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery> {
        let debug_step = vulkan_debug_enabled()
            .then(|| VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed));
        let pending = runtime.callback_state.lock().latest_hw_frame.take();
        let Some(pending) = pending else {
            return Ok(match runtime.callback_state.lock().latest_frame.take() {
                Some(frame) => FrameDelivery::CpuFrame(frame),
                None => {
                    if let Some(step) = debug_step.filter(|step| *step < 16) {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "consume_frame step={} delivery=no_frame reason=missing_hw_and_cpu_frame",
                            step
                        );
                    }
                    FrameDelivery::NoFrame
                }
            });
        };

        match take_vulkan_render_frame(runtime, pending) {
            Ok(Some(frame)) => Ok(FrameDelivery::CpuFrame(frame)),
            Ok(None) => {
                let (
                    present_configured,
                    pending_images,
                    acquired_image_index,
                    queue_present_attempts,
                    queue_present_successes,
                ) = {
                    let state = runtime.hw_render_state.lock();
                    let vulkan = state.vulkan.as_ref();
                    let metrics = runtime.vulkan_present_metrics.lock();
                    (
                        vulkan.and_then(|vulkan| vulkan.present.as_ref()).is_some(),
                        vulkan.map(|vulkan| vulkan.pending_images.len()).unwrap_or(0),
                        vulkan
                            .and_then(|vulkan| vulkan.present.as_ref())
                            .and_then(|present| present.acquired_image_index),
                        metrics.queue_present_attempts,
                        metrics.queue_present_successes,
                    )
                };
                if present_configured {
                    if let Some(step) = debug_step.filter(|step| *step < 16) {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "consume_frame step={} delivery=external_present pending_images={} acquired_image_index={:?} queue_present_attempts={} queue_present_successes={}",
                            step,
                            pending_images,
                            acquired_image_index,
                            queue_present_attempts,
                            queue_present_successes
                        );
                    }
                    Ok(FrameDelivery::ExternalPresent)
                } else {
                    if let Some(step) = debug_step.filter(|step| *step < 16) {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "consume_frame step={} delivery=no_frame reason=present_state_unavailable pending_images={} queue_present_attempts={} queue_present_successes={}",
                            step,
                            pending_images,
                            queue_present_attempts,
                            queue_present_successes
                        );
                    }
                    Ok(FrameDelivery::NoFrame)
                }
            }
            Err(err) => Ok(FrameDelivery::Error(err.to_string())),
        }
    }

    fn handle_geometry_update(&mut self, runtime: &HostRuntime, geometry: RetroGameGeometry) {
        apply_runtime_geometry_update(runtime, geometry);
    }

    fn using_external_present(&self, runtime: &HostRuntime) -> bool {
        runtime
            .hw_render_state
            .lock()
            .external_vulkan_present_active
    }

    fn has_external_present_window(&self, runtime: &HostRuntime) -> bool {
        runtime
            .hw_render_state
            .lock()
            .external_vulkan_window
            .is_some()
    }

    fn set_overlay_message(&mut self, runtime: &HostRuntime, message: Option<&str>) {
        let mut state = runtime.hw_render_state.lock();
        if let Some(window) = state.external_vulkan_window.as_mut() {
            window.set_overlay_message(message);
        }
    }
}
