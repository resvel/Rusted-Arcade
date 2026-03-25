use super::*;
use arcade_domain::N64ParallelProfile;

pub(super) fn default_core_variables_for(
    core_name: &str,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) -> HashMap<String, CString> {
    let mut variables = HashMap::new();

    if core_name == "mupen64plus_next" {
        insert_core_variable(&mut variables, "mupen64plus-cpucore", "dynamic_recompiler");
        insert_core_variable(&mut variables, "mupen64plus-rsp-plugin", "hle");
        insert_core_variable(&mut variables, "mupen64plus-aspect", emulation.n64.aspect_ratio.as_core_value());
        insert_core_variable(&mut variables, "mupen64plus-BilinearMode", "3point");
        insert_core_variable(&mut variables, "mupen64plus-MultiSampling", "0");
        insert_core_variable(&mut variables, "mupen64plus-EnableFBEmulation", "True");
        insert_core_variable(
            &mut variables,
            "mupen64plus-EnableCopyColorToRDRAM",
            "Async",
        );
        let (resolution_43, resolution_169) =
            mupen64plus_next_internal_resolutions(emulation.n64.parallel_rdp_upscaling);
        insert_core_variable(&mut variables, "mupen64plus-43screensize", resolution_43);
        insert_core_variable(&mut variables, "mupen64plus-169screensize", resolution_169);

        // Software backend: keep angrylion defaults so frames come back via CPU readback.
        if backend == VideoBackendKind::Software {
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

        // Vulkan backend: run LLE ParaLLEl by default.
        if backend == VideoBackendKind::Vulkan {
            insert_core_variable(&mut variables, "mupen64plus-rdp-plugin", "parallel");
            insert_core_variable(&mut variables, "@mupen64plus-rdp-plugin", "parallel");
            // ParaLLEl-RDP requires the Parallel RSP (LLE); HLE is incompatible.
            insert_core_variable(&mut variables, "mupen64plus-rsp-plugin", "parallel");
            #[cfg(target_os = "macos")]
            {
                // Upstream macOS builds disable dynarec, so the only remaining CPU-side
                // throughput knobs are the documented timing shortcuts. Fullspeed forces
                // CountPerOp=1 inside the core and frame duping helps smooth low-end cadence.
                insert_core_variable(&mut variables, "mupen64plus-Framerate", "Fullspeed");
                insert_core_variable(&mut variables, "mupen64plus-FrameDuping", "True");
                insert_core_variable(&mut variables, "mupen64plus-virefresh", "1500");
            }
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-upscaling",
                emulation.n64.parallel_rdp_upscaling.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-synchronous",
                emulation.n64.parallel_rdp_synchronous.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-super-sampled-read-back",
                emulation.n64.parallel_rdp_super_sampled_read_back.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-vi-aa",
                emulation.n64.parallel_rdp_vi_aa.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-vi-bilinear",
                emulation.n64.parallel_rdp_vi_bilinear.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-dither-filter",
                emulation.n64.parallel_rdp_dither_filter.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-divot-filter",
                emulation.n64.parallel_rdp_divot_filter.as_core_value(),
            );
            insert_core_variable(
                &mut variables,
                "mupen64plus-parallel-rdp-gamma-dither",
                emulation.n64.parallel_rdp_gamma_dither.as_core_value(),
            );
        }

        apply_mupen64plus_next_env_overrides(&mut variables);
    }

    if core_name == "parallel_n64" {
        let rosetta = arcade_domain::is_running_under_rosetta();

        match backend {
            VideoBackendKind::Software => {
                insert_core_variable(&mut variables, "parallel-n64-gfxplugin", "angrylion");
                #[cfg(target_os = "macos")]
                if !rosetta {
                    insert_core_variable(
                        &mut variables,
                        "parallel-n64-cpucore",
                        "cached_interpreter",
                    );
                }
            }
            VideoBackendKind::OpenGl | VideoBackendKind::Vulkan => {
                #[cfg(target_os = "macos")]
                let gfx_plugin = if rosetta {
                    // Under Rosetta the x86_64 parallel backend works via Vulkan/MoltenVK.
                    "parallel"
                } else if backend == VideoBackendKind::OpenGl {
                    // Native arm64 safety default: keep the OpenGL backend on angrylion until the
                    // rice/gln64 handoff path is fully stable across launch/exit cycles.
                    "angrylion"
                } else {
                    "parallel"
                };
                #[cfg(target_os = "macos")]
                if !rosetta && backend == VideoBackendKind::OpenGl {
                    insert_core_variable(
                        &mut variables,
                        "parallel-n64-cpucore",
                        "cached_interpreter",
                    );
                }
                #[cfg(not(target_os = "macos"))]
                let gfx_plugin = "parallel";
                insert_core_variable(&mut variables, "parallel-n64-gfxplugin", gfx_plugin);
                if backend == VideoBackendKind::OpenGl {
                    insert_core_variable(&mut variables, "parallel-n64-rspplugin", "parallel");
                }
                if backend == VideoBackendKind::Vulkan {
                    #[cfg(target_os = "macos")]
                    if rosetta {
                        insert_core_variable(
                            &mut variables,
                            "parallel-n64-rspplugin",
                            "parallel",
                        );
                    }
                    #[cfg(not(target_os = "macos"))]
                    insert_core_variable(&mut variables, "parallel-n64-rspplugin", "parallel");
                }
                let upscaling = emulation.n64.parallel_rdp_upscaling.as_core_value();
                insert_core_variable(
                    &mut variables,
                    "parallel-n64-parallel-rdp-upscaling",
                    upscaling,
                );
                #[cfg(target_os = "macos")]
                let can_apply_performance_preset = rosetta;
                #[cfg(not(target_os = "macos"))]
                let can_apply_performance_preset = true;

                if can_apply_performance_preset
                    && backend == VideoBackendKind::Vulkan
                    && emulation.n64.parallel_profile == N64ParallelProfile::Performance
                {
                    apply_parallel_n64_performance_preset(&mut variables, upscaling);
                }
            }
        }
        // Under Rosetta the x86_64 core can use dynarec; explicitly request it so
        // the choice is visible in core variable logs.
        #[cfg(target_os = "macos")]
        if rosetta && !variables.contains_key("parallel-n64-cpucore") {
            insert_core_variable(&mut variables, "parallel-n64-cpucore", "dynamic_recompiler");
        }
        if let Some(cpucore_override) =
            effective_parallel_n64_cpucore_override(backend, parallel_n64_cpucore_override())
        {
            insert_core_variable(&mut variables, "parallel-n64-cpucore", &cpucore_override);
        }
        apply_parallel_n64_env_overrides(&mut variables);
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
    // Keep host runtime behavior non-intrusive: do not inject process env
    // defaults that may alter core behavior. Explicit user overrides are still
    // respected via ARCADE_* env vars and core-variable overrides.
    let _ = (core_name, backend);
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

fn effective_parallel_n64_cpucore_override(
    backend: VideoBackendKind,
    requested: Option<String>,
) -> Option<String> {
    let _ = backend;
    requested
}

fn mupen64plus_next_internal_resolutions(
    scale: arcade_domain::N64ParallelRdpUpscaling,
) -> (&'static str, &'static str) {
    match scale {
        arcade_domain::N64ParallelRdpUpscaling::X1 => ("640x480", "1280x720"),
        arcade_domain::N64ParallelRdpUpscaling::X2 => ("960x720", "1920x1080"),
        arcade_domain::N64ParallelRdpUpscaling::X4 => ("1280x960", "2560x1440"),
        arcade_domain::N64ParallelRdpUpscaling::X8 => ("1920x1440", "3840x2160"),
    }
}

fn apply_mupen64plus_next_env_overrides(variables: &mut HashMap<String, CString>) {
    if let Some(value) = std::env::var("ARCADE_MUPEN64PLUS_NEXT_RDP_PLUGIN")
        .ok()
        .filter(|value| !value.is_empty())
    {
        insert_core_variable(variables, "mupen64plus-rdp-plugin", &value);
        insert_core_variable(variables, "@mupen64plus-rdp-plugin", &value);
    }

    if let Some(value) = std::env::var("ARCADE_MUPEN64PLUS_NEXT_RSP_PLUGIN")
        .ok()
        .filter(|value| !value.is_empty())
    {
        insert_core_variable(variables, "mupen64plus-rsp-plugin", &value);
    }

    if let Some(value) = std::env::var("ARCADE_MUPEN64PLUS_NEXT_CPUCORE")
        .ok()
        .filter(|value| !value.is_empty())
    {
        insert_core_variable(variables, "mupen64plus-cpucore", &value);
    }
}

fn apply_parallel_n64_env_overrides(variables: &mut HashMap<String, CString>) {
    const OVERRIDES: &[(&str, &str)] = &[
        ("ARCADE_PARALLEL_N64_GFXPLUGIN", "parallel-n64-gfxplugin"),
        ("ARCADE_PARALLEL_N64_RSPPLUGIN", "parallel-n64-rspplugin"),
        (
            "ARCADE_PARALLEL_N64_ACCURACY",
            "parallel-n64-gfxplugin-accuracy",
        ),
        (
            "ARCADE_PARALLEL_N64_UPSCALING",
            "parallel-n64-parallel-rdp-upscaling",
        ),
        (
            "ARCADE_PARALLEL_N64_DISABLE_EXPMEM",
            "parallel-n64-disable_expmem",
        ),
        (
            "ARCADE_PARALLEL_N64_VI_AA",
            "parallel-n64-parallel-rdp-vi-aa",
        ),
        (
            "ARCADE_PARALLEL_N64_VI_BILINEAR",
            "parallel-n64-parallel-rdp-vi-bilinear",
        ),
        (
            "ARCADE_PARALLEL_N64_DITHER_FILTER",
            "parallel-n64-parallel-rdp-dither-filter",
        ),
        (
            "ARCADE_PARALLEL_N64_DIVOT_FILTER",
            "parallel-n64-parallel-rdp-divot-filter",
        ),
        (
            "ARCADE_PARALLEL_N64_GAMMA_DITHER",
            "parallel-n64-parallel-rdp-gamma-dither",
        ),
    ];

    for (env_key, core_key) in OVERRIDES {
        let Some(value) = std::env::var(env_key)
            .ok()
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        insert_core_variable(variables, core_key, &value);
    }
}

fn apply_parallel_n64_performance_preset(
    variables: &mut HashMap<String, CString>,
    upscaling: &str,
) {
    let accuracy = if upscaling == "1x" { "medium" } else { "low" };
    insert_core_variable(variables, "parallel-n64-gfxplugin-accuracy", accuracy);
    insert_core_variable(variables, "parallel-n64-parallel-rdp-vi-aa", "disabled");
    insert_core_variable(
        variables,
        "parallel-n64-parallel-rdp-vi-bilinear",
        "disabled",
    );
    insert_core_variable(
        variables,
        "parallel-n64-parallel-rdp-dither-filter",
        "disabled",
    );
    insert_core_variable(
        variables,
        "parallel-n64-parallel-rdp-divot-filter",
        "disabled",
    );
    insert_core_variable(
        variables,
        "parallel-n64-parallel-rdp-gamma-dither",
        "disabled",
    );
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
    fn default_core_variables_enable_mupen64plus_next_vulkan_parallel_defaults() {
        let variables = default_core_variables_for(
            "mupen64plus_next",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );
        let rdp = variables
            .get("mupen64plus-rdp-plugin")
            .expect("mupen64plus_next rdp override");
        let legacy_rdp = variables
            .get("@mupen64plus-rdp-plugin")
            .expect("mupen64plus_next legacy rdp override");
        let rsp = variables
            .get("mupen64plus-rsp-plugin")
            .expect("mupen64plus_next rsp override");
        let upscaling = variables
            .get("mupen64plus-parallel-rdp-upscaling")
            .expect("mupen64plus_next parallel upscaling override");
        let sync = variables
            .get("mupen64plus-parallel-rdp-synchronous")
            .expect("mupen64plus_next parallel sync override");
        let ssaa = variables
            .get("mupen64plus-parallel-rdp-super-sampled-read-back")
            .expect("mupen64plus_next parallel ssaa override");
        let vi_aa = variables
            .get("mupen64plus-parallel-rdp-vi-aa")
            .expect("mupen64plus_next parallel vi-aa override");
        let vi_bilinear = variables
            .get("mupen64plus-parallel-rdp-vi-bilinear")
            .expect("mupen64plus_next parallel vi-bilinear override");
        let dither = variables
            .get("mupen64plus-parallel-rdp-dither-filter")
            .expect("mupen64plus_next parallel dither override");
        let divot = variables
            .get("mupen64plus-parallel-rdp-divot-filter")
            .expect("mupen64plus_next parallel divot override");
        let gamma = variables
            .get("mupen64plus-parallel-rdp-gamma-dither")
            .expect("mupen64plus_next parallel gamma override");

        assert_eq!(rdp.to_str().expect("utf8"), "parallel");
        assert_eq!(legacy_rdp.to_str().expect("utf8"), "parallel");
        assert_eq!(rsp.to_str().expect("utf8"), "parallel");
        assert_eq!(upscaling.to_str().expect("utf8"), "1x");
        assert_eq!(sync.to_str().expect("utf8"), "false");
        assert_eq!(ssaa.to_str().expect("utf8"), "false");
        assert_eq!(vi_aa.to_str().expect("utf8"), "disabled");
        assert_eq!(vi_bilinear.to_str().expect("utf8"), "disabled");
        assert_eq!(dither.to_str().expect("utf8"), "disabled");
        assert_eq!(divot.to_str().expect("utf8"), "disabled");
        assert_eq!(gamma.to_str().expect("utf8"), "disabled");
        #[cfg(target_os = "macos")]
        {
            let framerate = variables
                .get("mupen64plus-Framerate")
                .expect("mupen64plus_next framerate override");
            let frame_duping = variables
                .get("mupen64plus-FrameDuping")
                .expect("mupen64plus_next frame duping override");
            let vi_refresh = variables
                .get("mupen64plus-virefresh")
                .expect("mupen64plus_next vi refresh override");
            assert_eq!(framerate.to_str().expect("utf8"), "Fullspeed");
            assert_eq!(frame_duping.to_str().expect("utf8"), "True");
            assert_eq!(vi_refresh.to_str().expect("utf8"), "1500");
        }
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
        #[cfg(not(target_os = "macos"))]
        {
            let rsp_plugin = variables
                .get("parallel-n64-rspplugin")
                .expect("parallel n64 rsp plugin override");
            assert_eq!(rsp_plugin.to_str().expect("utf8"), "parallel");
        }
        #[cfg(target_os = "macos")]
        {
            assert!(
                variables.get("parallel-n64-rspplugin").is_none(),
                "macOS Vulkan defaults should not force rspplugin"
            );
            assert!(
                variables.get("parallel-n64-cpucore").is_none(),
                "macOS Vulkan defaults should not force cpucore"
            );
        }
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

    #[cfg(target_os = "macos")]
    #[test]
    fn default_core_variables_do_not_force_parallel_n64_macos_vulkan_tuning() {
        let variables = default_core_variables_for(
            "parallel_n64",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );
        assert!(
            variables.get("parallel-n64-gfxplugin-accuracy").is_none(),
            "macOS Vulkan defaults should not force accuracy"
        );
        assert!(
            variables.get("parallel-n64-parallel-rdp-vi-aa").is_none(),
            "macOS Vulkan defaults should not force vi-aa"
        );
        assert!(
            variables
                .get("parallel-n64-parallel-rdp-vi-bilinear")
                .is_none(),
            "macOS Vulkan defaults should not force vi-bilinear"
        );
        assert!(
            variables
                .get("parallel-n64-parallel-rdp-dither-filter")
                .is_none(),
            "macOS Vulkan defaults should not force dither-filter"
        );
        assert!(
            variables
                .get("parallel-n64-parallel-rdp-divot-filter")
                .is_none(),
            "macOS Vulkan defaults should not force divot-filter"
        );
        assert!(
            variables
                .get("parallel-n64-parallel-rdp-gamma-dither")
                .is_none(),
            "macOS Vulkan defaults should not force gamma-dither"
        );
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

    #[cfg(target_os = "macos")]
    #[test]
    fn default_core_variables_do_not_force_parallel_n64_accuracy_when_upscaled_on_macos_vulkan() {
        let mut emulation = EmulationConfig::default();
        emulation.n64.parallel_rdp_upscaling = arcade_domain::N64ParallelRdpUpscaling::X2;

        let variables =
            default_core_variables_for("parallel_n64", VideoBackendKind::Vulkan, &emulation);
        assert!(
            variables.get("parallel-n64-gfxplugin-accuracy").is_none(),
            "macOS Vulkan defaults should not force accuracy for upscaled modes"
        );
    }

    #[test]
    fn default_core_variables_apply_parallel_n64_performance_profile_defaults() {
        let mut emulation = EmulationConfig::default();
        emulation.n64.parallel_rdp_upscaling = arcade_domain::N64ParallelRdpUpscaling::X2;
        emulation.n64.parallel_profile = arcade_domain::N64ParallelProfile::Performance;

        let variables =
            default_core_variables_for("parallel_n64", VideoBackendKind::Vulkan, &emulation);
        #[cfg(target_os = "macos")]
        {
            assert!(
                variables.get("parallel-n64-gfxplugin-accuracy").is_none(),
                "macOS Vulkan defaults should not force accuracy under performance profile"
            );
            assert!(
                variables.get("parallel-n64-parallel-rdp-vi-aa").is_none(),
                "macOS Vulkan defaults should not force vi-aa under performance profile"
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            let accuracy = variables
                .get("parallel-n64-gfxplugin-accuracy")
                .expect("parallel n64 accuracy override");
            let vi_aa = variables
                .get("parallel-n64-parallel-rdp-vi-aa")
                .expect("parallel n64 vi-aa override");
            assert_eq!(accuracy.to_str().expect("utf8"), "low");
            assert_eq!(vi_aa.to_str().expect("utf8"), "disabled");
        }
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
