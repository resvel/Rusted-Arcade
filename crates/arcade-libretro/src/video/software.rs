use super::*;

pub(super) struct SoftwareBackend;

impl VideoBackend for SoftwareBackend {
    fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery> {
        Ok(runtime
            .callback_state
            .lock()
            .latest_frame
            .take()
            .map(FrameDelivery::CpuFrame)
            .unwrap_or(FrameDelivery::NoFrame))
    }
}
