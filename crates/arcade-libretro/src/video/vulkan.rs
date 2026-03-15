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
        let pending = runtime.callback_state.lock().latest_hw_frame.take();
        let Some(pending) = pending else {
            return Ok(runtime
                .callback_state
                .lock()
                .latest_frame
                .take()
                .map(FrameDelivery::CpuFrame)
                .unwrap_or(FrameDelivery::NoFrame));
        };

        match take_vulkan_render_frame(runtime, pending) {
            Ok(Some(frame)) => Ok(FrameDelivery::CpuFrame(frame)),
            Ok(None)
                if runtime
                    .hw_render_state
                    .lock()
                    .vulkan
                    .as_ref()
                    .and_then(|vulkan| vulkan.present.as_ref())
                    .is_some() =>
            {
                Ok(FrameDelivery::ExternalPresent)
            }
            Ok(None) => Ok(FrameDelivery::NoFrame),
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
