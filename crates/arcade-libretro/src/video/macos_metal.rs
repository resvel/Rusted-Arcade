use super::*;

pub(super) struct MacosMetalViewBackend;

impl VideoBackend for MacosMetalViewBackend {
    fn begin_session(
        &mut self,
        runtime: &HostRuntime,
        session_info: &VideoSessionInfo,
    ) -> Result<()> {
        let title = {
            let state = runtime.hw_render_state.lock();
            if state.external_vulkan_window_title.is_empty() {
                format!("Arcade - {}", session_info.core_name)
            } else {
                state.external_vulkan_window_title.clone()
            }
        };

        let mut state = runtime.hw_render_state.lock();
        if state.external_macos_metal_window.is_none() {
            state.external_macos_metal_window =
                Some(ExternalMacosMetalWindow::create(&title).map_err(anyhow::Error::msg)?);
        }
        if let Some(window) = state.external_macos_metal_window.as_mut() {
            window.set_title(&title);
            window.set_visible(true);
        }
        state.macos_metal_present_active = false;
        state.macos_metal_visibility_pending = None;
        Ok(())
    }

    fn end_session(&mut self, runtime: &HostRuntime) {
        let window = {
            let mut state = runtime.hw_render_state.lock();
            state.macos_metal_present_active = false;
            state.macos_metal_visibility_pending = None;
            state.external_macos_metal_window.take()
        };
        if let Some(window) = window {
            destroy_external_macos_metal_window(window);
        }
    }

    fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery> {
        let deliveries = runtime
            .vulkan_present_metrics
            .lock()
            .external_present_deliveries;
        if deliveries > 0 {
            Ok(FrameDelivery::ExternalPresent)
        } else {
            Ok(FrameDelivery::NoFrame)
        }
    }

    fn using_external_present(&self, _runtime: &HostRuntime) -> bool {
        true
    }

    fn has_external_present_window(&self, runtime: &HostRuntime) -> bool {
        runtime
            .hw_render_state
            .lock()
            .external_macos_metal_window
            .is_some()
    }
}
