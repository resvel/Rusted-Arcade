use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoBackendKind {
    Software,
    OpenGl,
    Vulkan,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostPlatform {
    Linux,
    Windows,
    MacOs,
    Other,
}

pub(super) struct BackendPolicyInput<'a> {
    pub host_platform: HostPlatform,
    pub core_name: &'a str,
    pub requires_hw_render: bool,
    pub requested_hw_context_type: Option<u32>,
    pub frontend_capabilities: &'a FrontendCapabilities,
    pub explicit_parallel_n64_fallback: bool,
    pub windows_external_vulkan_present: bool,
}

pub(super) fn current_host_platform() -> HostPlatform {
    #[cfg(target_os = "linux")]
    {
        return HostPlatform::Linux;
    }
    #[cfg(target_os = "windows")]
    {
        return HostPlatform::Windows;
    }
    #[cfg(target_os = "macos")]
    {
        return HostPlatform::MacOs;
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        HostPlatform::Other
    }
}

pub(super) fn select_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    if input.core_name == "parallel_n64" {
        return select_parallel_n64_backend(input);
    }

    if input.requires_hw_render {
        if input.frontend_capabilities.supports_gl_backend()
            && requested_context_is_gl_or_unspecified(input.requested_hw_context_type)
        {
            return BackendSelection {
                chosen: VideoBackendKind::OpenGl,
                fallbacks: vec![VideoBackendKind::Software],
                allows_external_present: false,
            };
        }
    }

    BackendSelection {
        chosen: VideoBackendKind::Software,
        fallbacks: vec![],
        allows_external_present: false,
    }
}

fn select_parallel_n64_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    if input.explicit_parallel_n64_fallback {
        return BackendSelection {
            chosen: VideoBackendKind::Software,
            fallbacks: vec![],
            allows_external_present: false,
        };
    }

    match input.host_platform {
        HostPlatform::MacOs if input.frontend_capabilities.supports_gl_backend() => {
            BackendSelection {
                chosen: VideoBackendKind::OpenGl,
                fallbacks: vec![VideoBackendKind::Software],
                allows_external_present: false,
            }
        }
        HostPlatform::Linux if input.frontend_capabilities.has_windowing_probe() => {
            BackendSelection {
                chosen: VideoBackendKind::Vulkan,
                fallbacks: vec![VideoBackendKind::Software],
                allows_external_present: true,
            }
        }
        HostPlatform::Windows
            if input.windows_external_vulkan_present
                && input.frontend_capabilities.has_windowing_probe() =>
        {
            BackendSelection {
                chosen: VideoBackendKind::Vulkan,
                fallbacks: vec![VideoBackendKind::Software],
                allows_external_present: true,
            }
        }
        _ => BackendSelection {
            chosen: VideoBackendKind::Software,
            fallbacks: vec![],
            allows_external_present: false,
        },
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
            window_handle_kind: windowing.then(|| String::from("Xlib")),
            display_handle_kind: windowing.then(|| String::from("Xlib")),
        }
    }

    #[test]
    fn linux_parallel_n64_prefers_vulkan_when_windowing_is_available() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::Linux,
            core_name: "parallel_n64",
            requires_hw_render: true,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(true, true),
            explicit_parallel_n64_fallback: false,
            windows_external_vulkan_present: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::Vulkan);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(selection.allows_external_present);
    }

    #[test]
    fn windows_parallel_n64_requires_explicit_vulkan_gate() {
        let capabilities = frontend(true, true);
        let disabled = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::Windows,
            core_name: "parallel_n64",
            requires_hw_render: true,
            requested_hw_context_type: None,
            frontend_capabilities: &capabilities,
            explicit_parallel_n64_fallback: false,
            windows_external_vulkan_present: false,
        });
        let enabled = select_backend(BackendPolicyInput {
            windows_external_vulkan_present: true,
            ..BackendPolicyInput {
                host_platform: HostPlatform::Windows,
                core_name: "parallel_n64",
                requires_hw_render: true,
                requested_hw_context_type: None,
                frontend_capabilities: &capabilities,
                explicit_parallel_n64_fallback: false,
                windows_external_vulkan_present: false,
            }
        });

        assert_eq!(disabled.chosen, VideoBackendKind::Software);
        assert_eq!(enabled.chosen, VideoBackendKind::Vulkan);
    }

    #[test]
    fn gl_hardware_core_uses_opengl_when_frontend_gl_exists() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::Linux,
            core_name: "beetle_psx_hw",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(true, false),
            explicit_parallel_n64_fallback: false,
            windows_external_vulkan_present: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
    }

    #[test]
    fn gl_hardware_core_falls_back_to_software_without_gl_context() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::Linux,
            core_name: "beetle_psx_hw",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(false, false),
            explicit_parallel_n64_fallback: false,
            windows_external_vulkan_present: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::Software);
    }

    #[test]
    fn macos_policy_shape_is_opengl_then_software_for_gl_cores() {
        let selection = select_backend(BackendPolicyInput {
            host_platform: HostPlatform::MacOs,
            core_name: "beetle_psx_hw",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(true, false),
            explicit_parallel_n64_fallback: false,
            windows_external_vulkan_present: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
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
            windows_external_vulkan_present: false,
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(!selection.allows_external_present);
    }
}
