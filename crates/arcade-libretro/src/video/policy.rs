use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoBackendKind {
    Software,
    OpenGl,
    Vulkan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostPlatform {
    MacOs,
}

pub(super) struct BackendPolicyInput<'a> {
    pub host_platform: HostPlatform,
    pub core_name: &'a str,
    pub requires_hw_render: bool,
    pub requested_hw_context_type: Option<u32>,
    pub frontend_capabilities: &'a FrontendCapabilities,
    pub explicit_parallel_n64_fallback: bool,
    pub macos_experimental_vulkan: bool,
}

pub(super) fn current_host_platform() -> HostPlatform {
    HostPlatform::MacOs
}

pub(super) fn select_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    if input.core_name == "parallel_n64" {
        return select_parallel_n64_backend(input);
    }

    if input.core_name == "mupen64plus_next" {
        return select_mupen64plus_next_backend(input);
    }

    if input.requires_hw_render
        && input.frontend_capabilities.supports_gl_backend()
        && requested_context_is_gl_or_unspecified(input.requested_hw_context_type)
    {
        return BackendSelection {
            chosen: VideoBackendKind::OpenGl,
            fallbacks: vec![VideoBackendKind::Software],
            allows_external_present: false,
        };
    }

    BackendSelection {
        chosen: VideoBackendKind::Software,
        fallbacks: vec![],
        allows_external_present: false,
    }
}

fn select_mupen64plus_next_backend(_input: BackendPolicyInput<'_>) -> BackendSelection {
    BackendSelection {
        chosen: VideoBackendKind::Vulkan,
        fallbacks: vec![VideoBackendKind::Software],
        allows_external_present: false,
    }
}

fn select_parallel_n64_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    let _ = input.host_platform;
    if input.explicit_parallel_n64_fallback {
        return BackendSelection {
            chosen: VideoBackendKind::Software,
            fallbacks: vec![],
            allows_external_present: false,
        };
    }

    if input.macos_experimental_vulkan {
        return BackendSelection {
            chosen: VideoBackendKind::Vulkan,
            fallbacks: vec![VideoBackendKind::Software],
            allows_external_present: true,
        };
    }

    if input.frontend_capabilities.supports_gl_backend() {
        return BackendSelection {
            chosen: VideoBackendKind::OpenGl,
            fallbacks: vec![VideoBackendKind::Software],
            allows_external_present: false,
        };
    }

    BackendSelection {
        chosen: VideoBackendKind::Software,
        fallbacks: vec![],
        allows_external_present: false,
    }
}

fn requested_context_is_gl_or_unspecified(requested_hw_context_type: Option<u32>) -> bool {
    match requested_hw_context_type {
        None => true,
        Some(context_type) => hw_context_type_supported(context_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frontend(gl: bool, windowing: bool) -> FrontendCapabilities {
        FrontendCapabilities {
            renderer_name: Some(String::from(if gl {
                "eframe_glow"
            } else {
                "eframe_non_gl"
            })),
            gl_context: None,
            window_handle_kind: windowing.then(|| String::from("AppKit")),
            display_handle_kind: windowing.then(|| String::from("AppKit")),
        }
    }

    #[test]
    fn gl_hardware_core_uses_opengl_when_frontend_gl_exists() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "beetle_psx_hw",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(true, false),
            explicit_parallel_n64_fallback: false,
            macos_experimental_vulkan: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
    }

    #[test]
    fn gl_hardware_core_falls_back_to_software_without_gl_context() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "beetle_psx_hw",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(false, false),
            explicit_parallel_n64_fallback: false,
            macos_experimental_vulkan: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::Software);
    }

    #[test]
    fn macos_parallel_n64_can_opt_into_vulkan_experiment() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "parallel_n64",
            requires_hw_render: true,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(false, true),
            explicit_parallel_n64_fallback: false,
            macos_experimental_vulkan: true,
        });

        assert_eq!(selection.chosen, VideoBackendKind::Vulkan);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(selection.allows_external_present);
    }

    #[test]
    fn macos_parallel_n64_prefers_opengl_when_gl_frontend_exists() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "parallel_n64",
            requires_hw_render: true,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(true, false),
            explicit_parallel_n64_fallback: false,
            macos_experimental_vulkan: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(!selection.allows_external_present);
    }

    #[test]
    fn macos_mupen64plus_next_is_locked_to_vulkan() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "mupen64plus_next",
            requires_hw_render: false,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(true, false),
            explicit_parallel_n64_fallback: false,
            macos_experimental_vulkan: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::Vulkan);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(!selection.allows_external_present);
    }
}
