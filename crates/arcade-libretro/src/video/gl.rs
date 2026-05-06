use super::*;

pub(super) struct OpenGlBackend;

impl VideoBackend for OpenGlBackend {
    fn prepare_frame(
        &mut self,
        runtime: &HostRuntime,
        target_size: Option<(u32, u32)>,
    ) -> Result<()> {
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!(
                "gl backend prepare_frame target_size={target_size:?} context_type={:?}",
                runtime.hw_render_state.lock().context_type
            );
        }
        if runtime.hw_render_state.lock().context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
            return Ok(());
        }
        if let Some(target_size) = target_size {
            ensure_hw_render_target(target_size)?;
        }
        Ok(())
    }

    fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery> {
        let pending = runtime.callback_state.lock().latest_hw_frame.take();
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!("gl backend consume_frame pending_hw={}", pending.is_some());
        }
        let Some(pending) = pending else {
            let fallback = runtime
                .callback_state
                .lock()
                .latest_frame
                .take()
                .map(FrameDelivery::CpuFrame)
                .unwrap_or(FrameDelivery::NoFrame);
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!("gl backend consume_frame fallback={fallback:?}");
            }
            return Ok(fallback);
        };

        if should_use_play_direct_gl_texture(runtime) {
            #[cfg(target_os = "macos")]
            if using_private_play_gl_bridge(runtime) {
                return take_play_macos_iosurface_frame(runtime, pending).map(|frame| {
                    frame
                        .map(FrameDelivery::MacosIosurface)
                        .unwrap_or(FrameDelivery::NoFrame)
                });
            }
            return take_play_direct_gl_texture_frame(runtime, pending).map(|frame| {
                frame
                    .map(FrameDelivery::GlTexture)
                    .unwrap_or(FrameDelivery::NoFrame)
            });
        }

        let frame = read_opengl_render_frame(runtime, pending).map(FrameDelivery::CpuFrame);
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            match &frame {
                Ok(FrameDelivery::CpuFrame(frame)) => {
                    eprintln!(
                        "gl backend consume_frame readback={}x{}",
                        frame.width, frame.height
                    );
                }
                Ok(other) => eprintln!("gl backend consume_frame result={other:?}"),
                Err(err) => eprintln!("gl backend consume_frame error={err:#}"),
            }
        }
        frame
    }
}
