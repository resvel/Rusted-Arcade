use super::*;

pub(super) fn default_core_variables_for(
    core_name: &str,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) -> HashMap<String, CString> {
    let mut variables = HashMap::new();

    // Force the embedded N64 path onto the software RDP so frames come back via the normal
    // video callback instead of requiring an OpenGL context the host does not expose yet.
    if core_name == "mupen64plus_next" && backend == VideoBackendKind::Software {
        let value =
            CString::new("angrylion").expect("static libretro core variable should be valid");
        variables.insert(String::from("mupen64plus-rdp-plugin"), value.clone());
        variables.insert(String::from("@mupen64plus-rdp-plugin"), value);
        variables.insert(
            String::from("mupen64plus-rsp-plugin"),
            CString::new("parallel").expect("static libretro core variable should be valid"),
        );
        variables.insert(
            String::from("mupen64plus-angrylion-sync"),
            CString::new("Medium").expect("static libretro core variable should be valid"),
        );
        variables.insert(
            String::from("mupen64plus-angrylion-multithread"),
            CString::new("all threads").expect("static libretro core variable should be valid"),
        );
    }

    if core_name == "parallel_n64" {
        match backend {
            VideoBackendKind::Software => {
                insert_core_variable(&mut variables, "parallel-n64-gfxplugin", "angrylion");
                #[cfg(target_os = "macos")]
                insert_core_variable(&mut variables, "parallel-n64-cpucore", "cached_interpreter");
                #[cfg(target_os = "windows")]
                insert_core_variable(&mut variables, "parallel-n64-screensize", "640x480");
            }
            VideoBackendKind::OpenGl | VideoBackendKind::Vulkan => {
                #[cfg(target_os = "windows")]
                insert_core_variable(&mut variables, "parallel-n64-screensize", "640x480");
                #[cfg(target_os = "macos")]
                let gfx_plugin = if backend == VideoBackendKind::OpenGl {
                    // macOS safety default: keep the OpenGL backend on angrylion until the
                    // rice/gln64 handoff path is fully stable across launch/exit cycles.
                    "angrylion"
                } else {
                    "parallel"
                };
                #[cfg(target_os = "macos")]
                if backend == VideoBackendKind::OpenGl {
                    insert_core_variable(
                        &mut variables,
                        "parallel-n64-cpucore",
                        "cached_interpreter",
                    );
                }
                #[cfg(not(target_os = "macos"))]
                let gfx_plugin = "parallel";
                insert_core_variable(&mut variables, "parallel-n64-gfxplugin", gfx_plugin);
                insert_core_variable(
                    &mut variables,
                    "parallel-n64-parallel-rdp-upscaling",
                    emulation.n64.parallel_rdp_upscaling.as_core_value(),
                );
            }
        }
        if let Some(cpucore_override) = parallel_n64_cpucore_override() {
            insert_core_variable(&mut variables, "parallel-n64-cpucore", &cpucore_override);
        }
        insert_core_variable(&mut variables, "parallel-n64-virefresh", "Auto");
    }

    variables
}

#[cfg(test)]
pub(super) fn default_core_variables(core_name: &str) -> HashMap<String, CString> {
    default_core_variables_for(
        core_name,
        VideoBackendKind::Software,
        &EmulationConfig::default(),
    )
}

pub(super) fn apply_core_runtime_env_defaults(core_name: &str, backend: VideoBackendKind) {
    #[cfg(target_os = "windows")]
    {
        if core_name == "parallel_n64" && backend == VideoBackendKind::Vulkan {
            apply_runtime_env_default(
                "PARALLEL_RDP_SMALL_TYPES",
                "0",
                "forcing conservative paraLLEl-RDP small-type path on Windows",
            );
            apply_runtime_env_default(
                "PARALLEL_RDP_SUBGROUP",
                "0",
                "forcing conservative paraLLEl-RDP subgroup path on Windows",
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        if core_name == "parallel_n64" && backend == VideoBackendKind::OpenGl {
            apply_runtime_env_default(
                "ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER",
                "1",
                "forcing default framebuffer path for parallel_n64 OpenGL on macOS",
            );
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (core_name, backend);
    }
}

fn insert_core_variable(variables: &mut HashMap<String, CString>, key: &str, value: &str) {
    let value = CString::new(value).expect("static libretro core variable should be valid");
    variables.insert(String::from(key), value);
}

fn parallel_n64_cpucore_override() -> Option<String> {
    std::env::var("ARCADE_PARALLEL_N64_CPUCORE")
        .ok()
        .filter(|value| !value.is_empty())
}

fn apply_runtime_env_default(key: &str, value: &str, log_message: &str) {
    if std::env::var_os(key).is_some() {
        return;
    }

    unsafe {
        std::env::set_var(key, value);
    }

    info!("{log_message}: {key}={value}");
}

pub(super) fn store_default_variable(context: &mut EnvironmentContext, key: &str, spec: &str) {
    if context.variables.contains_key(key) {
        return;
    }

    let Some(default_value) = parse_default_variable_value(spec) else {
        return;
    };
    let Ok(default_cstring) = CString::new(default_value) else {
        return;
    };
    context.variables.insert(key.to_string(), default_cstring);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_core_variables_force_software_n64_renderer() {
        let variables = default_core_variables("mupen64plus_next");
        let value = variables
            .get("mupen64plus-rdp-plugin")
            .expect("n64 renderer override");
        let at_value = variables
            .get("@mupen64plus-rdp-plugin")
            .expect("legacy n64 renderer override");
        let multithread = variables
            .get("mupen64plus-angrylion-multithread")
            .expect("n64 multithread override");
        let rsp = variables
            .get("mupen64plus-rsp-plugin")
            .expect("n64 rsp override");
        let sync = variables
            .get("mupen64plus-angrylion-sync")
            .expect("n64 angrylion sync override");

        assert_eq!(value.to_str().expect("utf8"), "angrylion");
        assert_eq!(at_value.to_str().expect("utf8"), "angrylion");
        assert_eq!(rsp.to_str().expect("utf8"), "parallel");
        assert_eq!(sync.to_str().expect("utf8"), "Medium");
        assert_eq!(multithread.to_str().expect("utf8"), "all threads");
    }

    #[test]
    fn default_core_variables_enable_parallel_n64_vulkan_renderer_defaults() {
        let variables = default_core_variables_for(
            "parallel_n64",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );
        let plugin = variables
            .get("parallel-n64-gfxplugin")
            .expect("parallel n64 plugin override");
        let vi_refresh = variables
            .get("parallel-n64-virefresh")
            .expect("parallel n64 vi refresh override");
        let upscale = variables
            .get("parallel-n64-parallel-rdp-upscaling")
            .expect("parallel rdp upscaling override");

        assert_eq!(plugin.to_str().expect("utf8"), "parallel");
        assert_eq!(upscale.to_str().expect("utf8"), "1x");
        assert_eq!(vi_refresh.to_str().expect("utf8"), "Auto");
    }

    #[test]
    fn default_core_variables_force_parallel_n64_software_renderer_when_selected() {
        let variables = default_core_variables_for(
            "parallel_n64",
            VideoBackendKind::Software,
            &EmulationConfig::default(),
        );
        let plugin = variables
            .get("parallel-n64-gfxplugin")
            .expect("parallel n64 plugin override");

        assert_eq!(plugin.to_str().expect("utf8"), "angrylion");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_core_variables_force_parallel_n64_macos_opengl_to_angrylion() {
        let variables = default_core_variables_for(
            "parallel_n64",
            VideoBackendKind::OpenGl,
            &EmulationConfig::default(),
        );
        let plugin = variables
            .get("parallel-n64-gfxplugin")
            .expect("parallel n64 plugin override");

        assert_eq!(plugin.to_str().expect("utf8"), "angrylion");
    }

    #[test]
    fn default_core_variables_honor_parallel_n64_upscaling_override() {
        let mut emulation = EmulationConfig::default();
        emulation.n64.parallel_rdp_upscaling = arcade_domain::N64ParallelRdpUpscaling::X2;

        let variables =
            default_core_variables_for("parallel_n64", VideoBackendKind::Vulkan, &emulation);
        let upscale = variables
            .get("parallel-n64-parallel-rdp-upscaling")
            .expect("parallel rdp upscaling override");

        assert_eq!(upscale.to_str().expect("utf8"), "2x");
    }

    #[test]
    fn store_default_variable_preserves_existing_override() {
        let mut context = EnvironmentContext {
            variables: default_core_variables("mupen64plus_next"),
            ..EnvironmentContext::default()
        };

        store_default_variable(
            &mut context,
            "mupen64plus-rdp-plugin",
            "RDP Plugin; gliden64|angrylion|parallel",
        );

        let value = context
            .variables
            .get("mupen64plus-rdp-plugin")
            .expect("n64 renderer override");
        assert_eq!(value.to_str().expect("utf8"), "angrylion");
    }
}
