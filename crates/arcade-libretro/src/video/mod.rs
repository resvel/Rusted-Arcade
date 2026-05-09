mod gl;
#[cfg(target_os = "macos")]
mod macos_metal;
mod policy;
mod software;
mod vulkan;

use super::*;

pub use self::policy::VideoBackendKind;

#[derive(Clone, Default)]
pub struct FrontendCapabilities {
    pub renderer_name: Option<String>,
    pub gl_context: Option<Arc<glow::Context>>,
    pub window_handle_kind: Option<String>,
    pub display_handle_kind: Option<String>,
}

impl FrontendCapabilities {
    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.renderer_name == other.renderer_name
            && self.window_handle_kind == other.window_handle_kind
            && self.display_handle_kind == other.display_handle_kind
            && match (&self.gl_context, &other.gl_context) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }

    pub fn has_gl_context(&self) -> bool {
        self.gl_context.is_some()
    }

    pub fn supports_gl_backend(&self) -> bool {
        self.has_gl_context() || self.renderer_name.as_deref() == Some("eframe_glow")
    }

    pub fn has_windowing_probe(&self) -> bool {
        self.window_handle_kind.is_some() && self.display_handle_kind.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSelection {
    pub chosen: VideoBackendKind,
    pub fallbacks: Vec<VideoBackendKind>,
    pub allows_external_present: bool,
}

#[derive(Debug)]
pub(super) enum FrameDelivery {
    CpuFrame(FrameBuffer),
    GlTexture(GlTextureFrame),
    #[cfg(target_os = "macos")]
    MacosIosurface(MacosIosurfaceFrame),
    ExternalPresent,
    NoFrame,
    Error(String),
}

#[derive(Debug, Clone)]
pub(super) struct VideoSessionInfo {
    pub core_name: String,
    pub requires_hw_render: bool,
    pub requested_hw_context_type: Option<u32>,
    pub force_macos_metal_view: bool,
}

pub(super) trait VideoBackend {
    fn apply_frontend_capabilities(
        &mut self,
        _runtime: &HostRuntime,
        _capabilities: &FrontendCapabilities,
    ) {
    }

    fn begin_session(
        &mut self,
        _runtime: &HostRuntime,
        _session_info: &VideoSessionInfo,
    ) -> Result<()> {
        Ok(())
    }

    fn end_session(&mut self, _runtime: &HostRuntime) {}

    fn prepare_frame(
        &mut self,
        _runtime: &HostRuntime,
        _target_size: Option<(u32, u32)>,
    ) -> Result<()> {
        Ok(())
    }

    fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery>;

    fn handle_geometry_update(&mut self, _runtime: &HostRuntime, _geometry: RetroGameGeometry) {}

    fn using_external_present(&self, _runtime: &HostRuntime) -> bool {
        false
    }

    fn has_external_present_window(&self, _runtime: &HostRuntime) -> bool {
        false
    }

    fn set_overlay_message(&mut self, _runtime: &HostRuntime, _message: Option<&str>) {}
}

#[derive(Default)]
pub(super) struct VideoCoordinator {
    frontend_capabilities: FrontendCapabilities,
    session: Option<ResolvedVideoSession>,
}

#[derive(Debug, Clone)]
struct ResolvedVideoSession {
    info: VideoSessionInfo,
    selection: BackendSelection,
}

impl VideoCoordinator {
    pub(super) fn frontend_capabilities(&self) -> &FrontendCapabilities {
        &self.frontend_capabilities
    }

    pub(super) fn apply_frontend_capabilities(
        &mut self,
        runtime: &HostRuntime,
        capabilities: FrontendCapabilities,
    ) -> bool {
        if self.frontend_capabilities.equivalent_to(&capabilities) {
            return false;
        }
        self.frontend_capabilities = capabilities.clone();
        self.with_each_backend(runtime, |backend| {
            backend.apply_frontend_capabilities(runtime, &capabilities);
            Ok(())
        })
        .expect("backend capability application should be infallible");
        true
    }

    pub(super) fn plan_session(&mut self, session_info: VideoSessionInfo) -> BackendSelection {
        let selection = policy::select_backend(policy::BackendPolicyInput {
            core_name: &session_info.core_name,
            requires_hw_render: session_info.requires_hw_render,
            requested_hw_context_type: session_info.requested_hw_context_type,
            force_macos_metal_view: session_info.force_macos_metal_view,
            frontend_capabilities: &self.frontend_capabilities,
        });
        self.session = Some(ResolvedVideoSession {
            info: session_info,
            selection: selection.clone(),
        });
        selection
    }

    pub(super) fn current_selection(&self) -> Option<&BackendSelection> {
        self.session.as_ref().map(|session| &session.selection)
    }

    pub(super) fn current_backend_kind(&self) -> VideoBackendKind {
        self.current_selection()
            .map(|selection| selection.chosen)
            .unwrap_or(VideoBackendKind::Software)
    }

    pub(super) fn begin_session(&mut self, runtime: &HostRuntime) -> Result<()> {
        let Some(session) = self.session.clone() else {
            return Ok(());
        };
        self.with_backend_mut(runtime, session.selection.chosen, |backend| {
            backend.begin_session(runtime, &session.info)
        })
    }

    pub(super) fn end_session(&mut self, runtime: &HostRuntime) {
        let kind = self.current_backend_kind();
        let _ = self.with_backend_mut(runtime, kind, |backend| {
            backend.end_session(runtime);
            Ok(())
        });
        self.session = None;
    }

    pub(super) fn prepare_frame(
        &mut self,
        runtime: &HostRuntime,
        target_size: Option<(u32, u32)>,
    ) -> Result<()> {
        let kind = self.current_backend_kind();
        self.with_backend_mut(runtime, kind, |backend| {
            backend.prepare_frame(runtime, target_size)
        })
    }

    pub(super) fn consume_frame(&mut self, runtime: &HostRuntime) -> Result<FrameDelivery> {
        let kind = self.current_backend_kind();
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!("video_coordinator consume_frame backend={kind:?}");
        }
        self.with_backend_mut(runtime, kind, |backend| backend.consume_frame(runtime))
    }

    pub(super) fn handle_geometry_update(
        &mut self,
        runtime: &HostRuntime,
        geometry: RetroGameGeometry,
    ) {
        let kind = self.current_backend_kind();
        let _ = self.with_backend_mut(runtime, kind, |backend| {
            backend.handle_geometry_update(runtime, geometry);
            Ok(())
        });
    }

    pub(super) fn using_external_present(&self, runtime: &HostRuntime) -> bool {
        self.with_backend(self.current_backend_kind(), |backend| {
            backend.using_external_present(runtime)
        })
    }

    pub(super) fn has_external_present_window(&self, runtime: &HostRuntime) -> bool {
        self.with_backend(self.current_backend_kind(), |backend| {
            backend.has_external_present_window(runtime)
        })
    }

    pub(super) fn set_overlay_message(&mut self, runtime: &HostRuntime, message: Option<&str>) {
        let kind = self.current_backend_kind();
        let _ = self.with_backend_mut(runtime, kind, |backend| {
            backend.set_overlay_message(runtime, message);
            Ok(())
        });
    }

    fn with_each_backend<T>(
        &mut self,
        runtime: &HostRuntime,
        mut f: impl FnMut(&mut dyn VideoBackend) -> Result<T>,
    ) -> Result<()> {
        let mut software_backend = software::SoftwareBackend;
        let mut gl_backend = gl::OpenGlBackend;
        let mut vulkan_backend = vulkan::VulkanBackend;
        #[cfg(target_os = "macos")]
        let mut macos_metal_backend = macos_metal::MacosMetalViewBackend;
        let _ = runtime;
        f(&mut software_backend)?;
        f(&mut gl_backend)?;
        f(&mut vulkan_backend)?;
        #[cfg(target_os = "macos")]
        f(&mut macos_metal_backend)?;
        Ok(())
    }

    fn with_backend<T>(&self, kind: VideoBackendKind, f: impl FnOnce(&dyn VideoBackend) -> T) -> T {
        match kind {
            VideoBackendKind::Software => f(&software::SoftwareBackend),
            VideoBackendKind::OpenGl => f(&gl::OpenGlBackend),
            VideoBackendKind::Vulkan => f(&vulkan::VulkanBackend),
            #[cfg(target_os = "macos")]
            VideoBackendKind::MacosMetalView => f(&macos_metal::MacosMetalViewBackend),
        }
    }

    fn with_backend_mut<T>(
        &mut self,
        _runtime: &HostRuntime,
        kind: VideoBackendKind,
        f: impl FnOnce(&mut dyn VideoBackend) -> Result<T>,
    ) -> Result<T> {
        match kind {
            VideoBackendKind::Software => {
                let mut backend = software::SoftwareBackend;
                f(&mut backend)
            }
            VideoBackendKind::OpenGl => {
                let mut backend = gl::OpenGlBackend;
                f(&mut backend)
            }
            VideoBackendKind::Vulkan => {
                let mut backend = vulkan::VulkanBackend;
                f(&mut backend)
            }
            #[cfg(target_os = "macos")]
            VideoBackendKind::MacosMetalView => {
                let mut backend = macos_metal::MacosMetalViewBackend;
                f(&mut backend)
            }
        }
    }
}
