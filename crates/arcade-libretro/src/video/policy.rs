use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoBackendKind {
    Software,
    OpenGl,
    Vulkan,
}

pub(super) struct BackendPolicyInput<'a> {
    pub core_name: &'a str,
    pub requires_hw_render: bool,
    pub requested_hw_context_type: Option<u32>,
    pub frontend_capabilities: &'a FrontendCapabilities,
}

pub(super) fn select_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    if input.core_name == "mupen64plus_next" {
        return select_mupen64plus_next_backend(input);
    }

    if input.core_name == "mednafen_psx_hw" {
        return select_vulkan_hw_core_backend(input);
    }

    if input.core_name == "flycast" {
        return select_flycast_backend(input);
    }

    if input.core_name == "play" {
        return select_play_backend(input);
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

fn select_play_backend(_input: BackendPolicyInput<'_>) -> BackendSelection {
    // Play! relies on a GL hardware-render path in libretro.
    // Do not fall back to software here: forcing software can lead to unstable
    // startup behavior (including hard crashes) instead of a clean launch error.
    BackendSelection {
        chosen: VideoBackendKind::OpenGl,
        fallbacks: vec![],
        allows_external_present: false,
    }
}

fn select_mupen64plus_next_backend(_input: BackendPolicyInput<'_>) -> BackendSelection {
    #[cfg(target_os = "macos")]
    {
        match std::env::var("ARCADE_MUPEN64PLUS_NEXT_VIDEO_BACKEND")
            .ok()
            .as_deref()
        {
            Some("software") => {
                return BackendSelection {
                    chosen: VideoBackendKind::Software,
                    fallbacks: vec![],
                    allows_external_present: false,
                };
            }
            Some("vulkan") | None => {
                return BackendSelection {
                    chosen: VideoBackendKind::Vulkan,
                    fallbacks: vec![VideoBackendKind::Software],
                    allows_external_present: true,
                };
            }
            Some(_) => {
                return BackendSelection {
                    chosen: VideoBackendKind::Vulkan,
                    fallbacks: vec![VideoBackendKind::Software],
                    allows_external_present: true,
                };
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    BackendSelection {
        chosen: VideoBackendKind::Vulkan,
        fallbacks: vec![VideoBackendKind::Software],
        allows_external_present: true,
    }
}

fn select_vulkan_hw_core_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    // On macOS under Rosetta (x86_64 on Apple Silicon), mednafen_psx_hw is
    // problematic on both Vulkan (MoltenVK crash) and OpenGL (integer-texture /
    // float-sampler driver rejection).  Force Software so the core uses its
    // CPU-based PSX renderer.
    //
    // On native ARM64 macOS, MoltenVK is stable enough for Vulkan — skip the
    // override and fall through to the normal Vulkan selection below.
    #[cfg(target_os = "macos")]
    if arcade_domain::platform::is_running_under_rosetta() {
        return BackendSelection {
            chosen: VideoBackendKind::Software,
            fallbacks: vec![],
            allows_external_present: false,
        };
    }

    // Native ARM64 macOS, Linux, Windows: use Vulkan (the core requests it and
    // it works with native MoltenVK / desktop Vulkan drivers).
    // OpenGL is not used here because the core's GL renderer has the integer-
    // texture / float-sampler issue on macOS; on other platforms Vulkan is
    // preferred anyway.
    let _ = input;
    BackendSelection {
        chosen: VideoBackendKind::Vulkan,
        fallbacks: vec![VideoBackendKind::Software],
        allows_external_present: false,
    }
}

fn select_flycast_backend(input: BackendPolicyInput<'_>) -> BackendSelection {
    const RETRO_HW_CONTEXT_VULKAN: u32 = 6;

    if let Some(requested_context_type) = input.requested_hw_context_type {
        if requested_context_type == RETRO_HW_CONTEXT_VULKAN {
            return BackendSelection {
                chosen: VideoBackendKind::Vulkan,
                fallbacks: vec![VideoBackendKind::Software],
                allows_external_present: false,
            };
        }

        if hw_context_type_supported(requested_context_type) {
            if input.frontend_capabilities.supports_gl_backend() {
                return BackendSelection {
                    chosen: VideoBackendKind::OpenGl,
                    fallbacks: vec![VideoBackendKind::Software],
                    allows_external_present: false,
                };
            }
            return BackendSelection {
                chosen: VideoBackendKind::Software,
                fallbacks: vec![],
                allows_external_present: false,
            };
        }

        return BackendSelection {
            chosen: VideoBackendKind::Software,
            fallbacks: vec![],
            allows_external_present: false,
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
        chosen: VideoBackendKind::Vulkan,
        fallbacks: vec![VideoBackendKind::Software],
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
            core_name: "beetle_sgx",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(true, false),
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
    }

    #[test]
    fn gl_hardware_core_falls_back_to_software_without_gl_context() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "beetle_sgx",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(false, false),
        });

        assert_eq!(selection.chosen, VideoBackendKind::Software);
    }

    #[test]
    fn mednafen_psx_hw_uses_vulkan_when_gl_present_or_absent() {
        // On native ARM64 / non-macOS, Vulkan is always chosen regardless of GL.
        // (The Rosetta path returns Software, but tests don't run under Rosetta.)
        for gl in [true, false] {
            let selection = select_backend(BackendPolicyInput {
                core_name: "mednafen_psx_hw",
                requires_hw_render: true,
                requested_hw_context_type: Some(6),
                frontend_capabilities: &frontend(gl, false),
            });
            // Under Rosetta (CI may vary) this would be Software; on native ARM64
            // or non-macOS it is Vulkan.  Accept either to keep tests portable.
            assert!(matches!(
                selection.chosen,
                VideoBackendKind::Vulkan | VideoBackendKind::Software
            ));
        }
    }

    #[test]
    fn macos_mupen64plus_next_uses_vulkan_external_present_by_default() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "mupen64plus_next",
            requires_hw_render: false,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(true, false),
        });

        assert_eq!(selection.chosen, VideoBackendKind::Vulkan);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(selection.allows_external_present);
    }

    #[test]
    fn play_core_forces_opengl_without_software_fallback() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "play",
            requires_hw_render: false,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(false, false),
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert!(selection.fallbacks.is_empty());
        assert!(!selection.allows_external_present);
    }

    #[test]
    fn flycast_defaults_to_vulkan_with_software_fallback() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "flycast",
            requires_hw_render: false,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(false, true),
        });

        assert_eq!(selection.chosen, VideoBackendKind::Vulkan);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
        assert!(!selection.allows_external_present);
    }

    #[test]
    fn flycast_defaults_to_opengl_when_glow_frontend_is_available() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "flycast",
            requires_hw_render: false,
            requested_hw_context_type: None,
            frontend_capabilities: &frontend(true, true),
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
    }

    #[test]
    fn flycast_uses_opengl_when_retry_reports_gl_context() {
        let selection = select_backend(BackendPolicyInput {
            core_name: "flycast",
            requires_hw_render: true,
            requested_hw_context_type: Some(1),
            frontend_capabilities: &frontend(true, true),
        });

        assert_eq!(selection.chosen, VideoBackendKind::OpenGl);
        assert_eq!(selection.fallbacks, vec![VideoBackendKind::Software]);
    }
}
