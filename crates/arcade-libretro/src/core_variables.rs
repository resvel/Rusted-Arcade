use super::*;

pub(super) fn default_core_variables_for(
    core_name: &str,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) -> HashMap<String, CString> {
    let mut variables = HashMap::new();

    // Step 1: Apply user-configurable settings from the core registry.
    if let Some(profile) = arcade_domain::core_profile_for(core_name) {
        for var_def in &profile.variables {
            let value =
                arcade_domain::resolve_core_variable(&emulation.core_settings, core_name, var_def);
            insert_core_variable(&mut variables, var_def.key, &value);
        }
    }

    // Step 2: Apply non-configurable forced overrides per core/backend.
    if core_name == "mupen64plus_next" {
        apply_mupen64plus_next_forced(&mut variables, backend, emulation);
        apply_mupen64plus_next_env_overrides(&mut variables);
    }

    if core_name == "parallel_n64" {
        apply_parallel_n64_forced(&mut variables, backend, emulation);
        apply_parallel_n64_env_overrides(&mut variables);
    }

    if matches!(core_name, "pcsx2" | "pcarmsx2") {
        apply_pcsx2_forced(&mut variables, backend);
    }

    variables
}

// ---------------------------------------------------------------------------
// mupen64plus_next: forced (non-configurable) overrides
// ---------------------------------------------------------------------------

fn apply_mupen64plus_next_forced(
    variables: &mut HashMap<String, CString>,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) {
    // Non-configurable baseline variables
    let default_cpucore = emulation.n64.cpu_core_mode.as_mupen64plus_core_value();
    insert_mupen64plus_next_cpucore_variable(variables, default_cpucore);
    insert_core_variable(variables, "mupen64plus-rsp-plugin", "hle");
    insert_core_variable(variables, "mupen64plus-BilinearMode", "3point");
    insert_core_variable(variables, "mupen64plus-MultiSampling", "0");

    // Screen sizes derived from upscaling level
    let upscaling = emulation
        .get_core_variable("mupen64plus_next", "mupen64plus-parallel-rdp-upscaling")
        .unwrap_or("1x");
    let (resolution_43, resolution_169) = mupen64plus_next_internal_resolutions_str(upscaling);
    insert_core_variable(variables, "mupen64plus-43screensize", resolution_43);
    insert_core_variable(variables, "mupen64plus-169screensize", resolution_169);

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

    // Vulkan backend: keep ParaLLEl-RDP + ParaLLEl RSP for the most stable
    // external-present behavior on macOS and other desktop platforms.
    if backend == VideoBackendKind::Vulkan {
        insert_core_variable(variables, "mupen64plus-rdp-plugin", "parallel");
        insert_core_variable(variables, "@mupen64plus-rdp-plugin", "parallel");
        #[cfg(target_os = "macos")]
        {
            // Native macOS Vulkan is more stable with ParaLLEl RSP in our external-present path.
            insert_core_variable(variables, "mupen64plus-rsp-plugin", "parallel");
        }
        #[cfg(not(target_os = "macos"))]
        insert_core_variable(variables, "mupen64plus-rsp-plugin", "parallel");
    }
}

// ---------------------------------------------------------------------------
// parallel_n64: forced (non-configurable) overrides
// ---------------------------------------------------------------------------

fn apply_parallel_n64_forced(
    variables: &mut HashMap<String, CString>,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) {
    let rosetta = arcade_domain::is_running_under_rosetta();

    match backend {
        VideoBackendKind::Software => {
            insert_core_variable(variables, "parallel-n64-gfxplugin", "angrylion");
            #[cfg(target_os = "macos")]
            if !rosetta {
                insert_core_variable(variables, "parallel-n64-cpucore", "cached_interpreter");
            }
        }
        VideoBackendKind::OpenGl | VideoBackendKind::Vulkan => {
            #[cfg(target_os = "macos")]
            let gfx_plugin = if rosetta {
                "parallel"
            } else if backend == VideoBackendKind::OpenGl {
                "angrylion"
            } else {
                "parallel"
            };
            #[cfg(target_os = "macos")]
            if !rosetta && backend == VideoBackendKind::OpenGl {
                insert_core_variable(variables, "parallel-n64-cpucore", "cached_interpreter");
            }
            #[cfg(not(target_os = "macos"))]
            let gfx_plugin = "parallel";
            insert_core_variable(variables, "parallel-n64-gfxplugin", gfx_plugin);
            if backend == VideoBackendKind::OpenGl {
                insert_core_variable(variables, "parallel-n64-rspplugin", "parallel");
            }
            if backend == VideoBackendKind::Vulkan {
                #[cfg(target_os = "macos")]
                if rosetta {
                    insert_core_variable(variables, "parallel-n64-rspplugin", "parallel");
                }
                #[cfg(not(target_os = "macos"))]
                insert_core_variable(variables, "parallel-n64-rspplugin", "parallel");
            }
            // Read upscaling from the shared N64 settings
            let upscaling = emulation
                .get_core_variable("mupen64plus_next", "mupen64plus-parallel-rdp-upscaling")
                .unwrap_or("1x");
            insert_core_variable(variables, "parallel-n64-parallel-rdp-upscaling", upscaling);

            // Ensure dynarec is used on macOS Vulkan when not running under Rosetta
            #[cfg(target_os = "macos")]
            if !rosetta && backend == VideoBackendKind::Vulkan {
                insert_core_variable(variables, "parallel-n64-cpucore", "dynamic_recompiler");
            }

            #[cfg(target_os = "macos")]
            let can_apply_performance_preset = rosetta;
            #[cfg(not(target_os = "macos"))]
            let can_apply_performance_preset = true;

            if can_apply_performance_preset && backend == VideoBackendKind::Vulkan {
                // The parallel_n64 performance preset is now applied when the user
                // has not explicitly configured the individual filter variables.
                // (Legacy parallel_profile concept removed — users set filters
                // directly via the settings UI.)
            }
        }
        #[cfg(target_os = "macos")]
        VideoBackendKind::MacosMetalView => {
            insert_core_variable(variables, "parallel-n64-gfxplugin", "parallel");
            insert_core_variable(variables, "parallel-n64-rspplugin", "parallel");
        }
    }
    #[cfg(target_os = "macos")]
    if rosetta && !variables.contains_key("parallel-n64-cpucore") {
        insert_core_variable(variables, "parallel-n64-cpucore", "dynamic_recompiler");
    }
    if let Some(cpucore_override) =
        effective_parallel_n64_cpucore_override(backend, parallel_n64_cpucore_override())
    {
        insert_core_variable(variables, "parallel-n64-cpucore", &cpucore_override);
    }
    insert_core_variable(variables, "parallel-n64-virefresh", "Auto");
}

// ---------------------------------------------------------------------------
// pcsx2: forced (non-configurable) overrides
// ---------------------------------------------------------------------------

fn apply_pcsx2_forced(variables: &mut HashMap<String, CString>, backend: VideoBackendKind) {
    match backend {
        VideoBackendKind::Vulkan => {
            let renderer = std::env::var("ARCADE_PCSX2_RENDERER")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| String::from("Vulkan"));
            insert_core_variable(variables, "pcsx2_renderer", &renderer);
        }
        VideoBackendKind::OpenGl => {
            insert_core_variable(variables, "pcsx2_renderer", "OpenGL");
        }
        #[cfg(target_os = "macos")]
        VideoBackendKind::MacosMetalView => {
            insert_core_variable(variables, "pcsx2_renderer", "Metal");
        }
        VideoBackendKind::Software => {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(super) fn default_core_variables(core_name: &str) -> HashMap<String, CString> {
    default_core_variables_for(
        core_name,
        VideoBackendKind::Software,
        &EmulationConfig::default(),
    )
}

pub(super) fn apply_core_runtime_env_defaults(core_name: &str, backend: VideoBackendKind) {
    if core_name.eq_ignore_ascii_case("pcsx2") && backend == VideoBackendKind::Vulkan {
        apply_pcsx2_vulkan_runtime_env_defaults();
    }
}

pub(super) fn pcarmsx2_metal_host_enabled(core_name: &str, emulation: &EmulationConfig) -> bool {
    core_name.eq_ignore_ascii_case("pcarmsx2")
        && core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_metal_host", true)
}

pub(super) fn apply_core_runtime_env_settings(core_name: &str, emulation: &EmulationConfig) {
    if !core_name.eq_ignore_ascii_case("pcarmsx2") {
        return;
    }

    set_env_flag(
        "PCARMSX2_ENABLE_EE_REC",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_enable_ee_rec", true),
    );
    set_env_flag(
        "PCARMSX2_ENABLE_VU0_REC",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_enable_vu0_rec", true),
    );
    set_env_flag(
        "PCARMSX2_ENABLE_VU1_REC",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_enable_vu1_rec", true),
    );
    set_env_flag(
        "PCARMSX2_ENABLE_IOP_REC",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_enable_iop_rec", true),
    );
    set_env_flag(
        "PCARMSX2_USE_JITA64",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_use_jita64", false),
    );
    set_env_flag(
        "PCARMSX2_ENABLE_XGKICK_HACK",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_enable_xgkick_hack", false),
    );
    set_env_flag(
        "PCARMSX2_PERFORMANCE_OVERLAY",
        core_setting_enabled(emulation, "pcarmsx2", "pcarmsx2_performance_overlay", false),
    );

    set_env_flag(
        "PCARMSX2_DISABLE_MTVU",
        !core_setting_enabled(emulation, "pcarmsx2", "pcsx2_mtvu", true),
    );
    set_env_flag(
        "PCARMSX2_DISABLE_INSTANT_VU1",
        !core_setting_enabled(emulation, "pcarmsx2", "pcsx2_instant_vu1", true),
    );

    let audio_backend =
        core_setting_value(emulation, "pcarmsx2", "pcarmsx2_audio_backend", "Cubeb");
    std::env::set_var("PCARMSX2_AUDIO_BACKEND", audio_backend);
}

fn core_setting_value(
    emulation: &EmulationConfig,
    core_name: &str,
    key: &str,
    default: &str,
) -> String {
    emulation
        .get_core_variable(core_name, key)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
        .to_string()
}

fn core_setting_enabled(
    emulation: &EmulationConfig,
    core_name: &str,
    key: &str,
    default_enabled: bool,
) -> bool {
    let default = if default_enabled {
        "enabled"
    } else {
        "disabled"
    };
    let value = core_setting_value(emulation, core_name, key, default);
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn set_env_flag(name: &str, enabled: bool) {
    std::env::set_var(name, if enabled { "1" } else { "0" });
}

fn apply_pcsx2_vulkan_runtime_env_defaults() {
    if std::env::var_os("GRANITE_VULKAN_LIBRARY").is_some() {
        return;
    }

    for candidate in [
        "/usr/local/lib/libvulkan.1.dylib",
        "/usr/local/lib/libvulkan.dylib",
        "/usr/local/lib/libMoltenVK.dylib",
        "/opt/homebrew/lib/libvulkan.1.dylib",
        "/opt/homebrew/lib/libvulkan.dylib",
        "/opt/homebrew/lib/libMoltenVK.dylib",
    ] {
        if std::path::Path::new(candidate).exists() {
            std::env::set_var("GRANITE_VULKAN_LIBRARY", candidate);
            break;
        }
    }
}

fn insert_core_variable(variables: &mut HashMap<String, CString>, key: &str, value: &str) {
    let value = CString::new(value).expect("static libretro core variable should be valid");
    variables.insert(String::from(key), value);
}

fn insert_mupen64plus_next_cpucore_variable(variables: &mut HashMap<String, CString>, value: &str) {
    insert_core_variable(variables, "mupen64plus-cpu-core", value);
    insert_core_variable(variables, "mupen64plus-cpucore", value);
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

fn mupen64plus_next_internal_resolutions_str(scale: &str) -> (&'static str, &'static str) {
    match scale {
        "2x" => ("960x720", "1920x1080"),
        "4x" => ("1280x960", "2560x1440"),
        "8x" => ("1920x1440", "3840x2160"),
        _ => ("640x480", "1280x720"), // 1x or unknown
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
        insert_mupen64plus_next_cpucore_variable(variables, &value);
    }

    if let Some(value) = std::env::var("ARCADE_MUPEN64PLUS_NEXT_COUNT_PER_OP")
        .ok()
        .filter(|value| !value.is_empty())
    {
        insert_core_variable(variables, "mupen64plus-CountPerOp", &value);
    }

    if let Some(value) = std::env::var("ARCADE_MUPEN64PLUS_NEXT_COUNT_PER_OP_DENOM_POT")
        .ok()
        .filter(|value| !value.is_empty())
    {
        insert_core_variable(variables, "mupen64plus-CountPerOpDenomPot", &value);
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
    use std::collections::HashMap as StdHashMap;
    use std::sync::{Mutex, OnceLock};

    fn pcsx2_renderer_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock")
    }

    fn emulation_with(core: &str, key: &str, value: &str) -> EmulationConfig {
        let mut e = EmulationConfig::default();
        e.core_settings
            .entry(core.into())
            .or_insert_with(StdHashMap::new)
            .insert(key.into(), value.into());
        e
    }

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
        let cpucore = variables
            .get("mupen64plus-cpucore")
            .expect("mupen64plus_next cpucore override");
        let cpu_core = variables
            .get("mupen64plus-cpu-core")
            .expect("mupen64plus_next cpu-core override");
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
        let count_per_op = variables
            .get("mupen64plus-CountPerOp")
            .expect("mupen64plus_next CountPerOp override");
        let count_per_op_denom = variables
            .get("mupen64plus-CountPerOpDenomPot")
            .expect("mupen64plus_next CountPerOpDenomPot override");

        assert_eq!(rdp.to_str().expect("utf8"), "parallel");
        assert_eq!(legacy_rdp.to_str().expect("utf8"), "parallel");
        #[cfg(target_os = "macos")]
        let expected_rsp = if arcade_domain::is_running_under_rosetta() {
            "parallel"
        } else {
            "parallel"
        };
        #[cfg(not(target_os = "macos"))]
        let expected_rsp = "parallel";
        let expected_cpucore = EmulationConfig::default()
            .n64
            .cpu_core_mode
            .as_mupen64plus_core_value();
        let expected_count_per_op = "0";
        assert_eq!(rsp.to_str().expect("utf8"), expected_rsp);
        assert_eq!(cpucore.to_str().expect("utf8"), expected_cpucore);
        assert_eq!(cpu_core.to_str().expect("utf8"), expected_cpucore);
        assert_eq!(count_per_op.to_str().expect("utf8"), expected_count_per_op);
        assert_eq!(count_per_op_denom.to_str().expect("utf8"), "0");
        assert_eq!(upscaling.to_str().expect("utf8"), "1x");
        assert_eq!(sync.to_str().expect("utf8"), "false");
        assert_eq!(ssaa.to_str().expect("utf8"), "false");
        assert_eq!(vi_aa.to_str().expect("utf8"), "disabled");
        assert_eq!(vi_bilinear.to_str().expect("utf8"), "disabled");
        assert_eq!(dither.to_str().expect("utf8"), "disabled");
        assert_eq!(divot.to_str().expect("utf8"), "disabled");
        assert_eq!(gamma.to_str().expect("utf8"), "disabled");
    }

    #[test]
    fn mupen64plus_next_vulkan_reads_upscaling_from_core_settings() {
        let emulation = emulation_with(
            "mupen64plus_next",
            "mupen64plus-parallel-rdp-upscaling",
            "4x",
        );
        let variables =
            default_core_variables_for("mupen64plus_next", VideoBackendKind::Vulkan, &emulation);
        let upscaling = variables
            .get("mupen64plus-parallel-rdp-upscaling")
            .expect("upscaling");
        assert_eq!(upscaling.to_str().expect("utf8"), "4x");
        // Screen sizes should also match 4x
        let res43 = variables
            .get("mupen64plus-43screensize")
            .expect("43 screen size");
        assert_eq!(res43.to_str().expect("utf8"), "1280x960");
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
            let cpucore = variables
                .get("parallel-n64-cpucore")
                .expect("macOS Vulkan defaults should force cpucore");
            assert_eq!(cpucore.to_str().expect("utf8"), "dynamic_recompiler");
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

    #[test]
    fn default_core_variables_apply_flycast_profile_defaults() {
        let variables = default_core_variables_for(
            "flycast",
            VideoBackendKind::OpenGl,
            &EmulationConfig::default(),
        );
        let threaded = variables
            .get("flycast_threaded_rendering")
            .expect("flycast threaded rendering default");
        let boot_to_bios = variables
            .get("flycast_boot_to_bios")
            .expect("flycast boot-to-bios default");
        let cable_type = variables
            .get("flycast_cable_type")
            .expect("flycast cable type default");

        assert_eq!(threaded.to_str().expect("utf8"), "enabled");
        assert_eq!(boot_to_bios.to_str().expect("utf8"), "disabled");
        assert_eq!(cable_type.to_str().expect("utf8"), "VGA(RGB)");
    }

    #[test]
    fn default_core_variables_apply_dolphin_profile_defaults() {
        let variables = default_core_variables_for(
            "dolphin",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );
        let renderer = variables
            .get("dolphin_renderer")
            .expect("dolphin renderer default");
        let efb_scale = variables
            .get("dolphin_efb_scale")
            .expect("dolphin efb scale default");
        let cpu_core = variables
            .get("dolphin_cpu_core")
            .expect("dolphin cpu core default");
        let fastmem = variables
            .get("dolphin_fastmem")
            .expect("dolphin fastmem default");
        let fastmem_arena = variables
            .get("dolphin_fastmem_arena")
            .expect("dolphin fastmem arena default");
        let main_mmu = variables
            .get("dolphin_main_mmu")
            .expect("dolphin main MMU default");
        let skip_gc_bios = variables
            .get("dolphin_skip_gc_bios")
            .expect("dolphin skip GameCube BIOS default");

        assert_eq!(renderer.to_str().expect("utf8"), "Hardware");
        assert_eq!(efb_scale.to_str().expect("utf8"), "1");
        #[cfg(target_arch = "aarch64")]
        assert_eq!(cpu_core.to_str().expect("utf8"), "4");
        #[cfg(target_arch = "x86_64")]
        assert_eq!(cpu_core.to_str().expect("utf8"), "1");
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        assert_eq!(cpu_core.to_str().expect("utf8"), "5");
        assert_eq!(fastmem.to_str().expect("utf8"), "enabled");
        assert_eq!(fastmem_arena.to_str().expect("utf8"), "enabled");
        assert_eq!(main_mmu.to_str().expect("utf8"), "disabled");
        assert_eq!(skip_gc_bios.to_str().expect("utf8"), "enabled");
    }

    #[test]
    fn default_core_variables_force_pcsx2_vulkan_renderer_when_backend_is_vulkan() {
        let _guard = pcsx2_renderer_env_lock();
        let key = "ARCADE_PCSX2_RENDERER";
        let previous = std::env::var_os(key);
        std::env::remove_var(key);

        let emulation = emulation_with("pcsx2", "pcsx2_renderer", "Software");
        let variables = default_core_variables_for("pcsx2", VideoBackendKind::Vulkan, &emulation);
        let renderer = variables
            .get("pcsx2_renderer")
            .expect("pcsx2 renderer override");

        assert_eq!(renderer.to_str().expect("utf8"), "Vulkan");

        if let Some(previous) = previous {
            std::env::set_var(key, previous);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_core_variables_force_pcsx2_metal_renderer_when_backend_is_macos_metal() {
        let emulation = emulation_with("pcsx2", "pcsx2_renderer", "Vulkan");
        let variables =
            default_core_variables_for("pcsx2", VideoBackendKind::MacosMetalView, &emulation);
        let renderer = variables
            .get("pcsx2_renderer")
            .expect("pcsx2 renderer override");

        assert_eq!(renderer.to_str().expect("utf8"), "Metal");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_core_variables_force_pcarmsx2_metal_renderer_when_backend_is_macos_metal() {
        let emulation = emulation_with("pcarmsx2", "pcsx2_renderer", "Vulkan");
        let variables =
            default_core_variables_for("pcarmsx2", VideoBackendKind::MacosMetalView, &emulation);
        let renderer = variables
            .get("pcsx2_renderer")
            .expect("pcsx2 renderer override");

        assert_eq!(renderer.to_str().expect("utf8"), "Metal");
    }

    #[test]
    fn pcarmsx2_metal_host_defaults_to_enabled() {
        assert!(pcarmsx2_metal_host_enabled(
            "pcarmsx2",
            &EmulationConfig::default()
        ));
        assert!(!pcarmsx2_metal_host_enabled(
            "pcsx2",
            &EmulationConfig::default()
        ));
    }

    #[test]
    fn pcarmsx2_runtime_settings_update_env_flags() {
        let _guard = pcsx2_renderer_env_lock();
        let keys = [
            "PCARMSX2_ENABLE_EE_REC",
            "PCARMSX2_ENABLE_VU0_REC",
            "PCARMSX2_ENABLE_VU1_REC",
            "PCARMSX2_ENABLE_IOP_REC",
            "PCARMSX2_USE_JITA64",
            "PCARMSX2_ENABLE_XGKICK_HACK",
            "PCARMSX2_PERFORMANCE_OVERLAY",
            "PCARMSX2_DISABLE_MTVU",
            "PCARMSX2_DISABLE_INSTANT_VU1",
            "PCARMSX2_AUDIO_BACKEND",
        ];
        let previous = keys.map(|key| (key, std::env::var_os(key)));

        let mut emulation = EmulationConfig::default();
        let settings = emulation
            .core_settings
            .entry("pcarmsx2".into())
            .or_insert_with(StdHashMap::new);
        settings.insert("pcarmsx2_enable_ee_rec".into(), "disabled".into());
        settings.insert("pcarmsx2_enable_iop_rec".into(), "enabled".into());
        settings.insert("pcarmsx2_use_jita64".into(), "enabled".into());
        settings.insert("pcarmsx2_enable_xgkick_hack".into(), "enabled".into());
        settings.insert("pcarmsx2_performance_overlay".into(), "enabled".into());
        settings.insert("pcsx2_mtvu".into(), "disabled".into());
        settings.insert("pcsx2_instant_vu1".into(), "enabled".into());
        settings.insert("pcarmsx2_audio_backend".into(), "Null".into());

        apply_core_runtime_env_settings("pcarmsx2", &emulation);

        assert_eq!(std::env::var("PCARMSX2_ENABLE_EE_REC").unwrap(), "0");
        assert_eq!(std::env::var("PCARMSX2_ENABLE_VU0_REC").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_ENABLE_VU1_REC").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_ENABLE_IOP_REC").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_USE_JITA64").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_ENABLE_XGKICK_HACK").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_PERFORMANCE_OVERLAY").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_DISABLE_MTVU").unwrap(), "1");
        assert_eq!(std::env::var("PCARMSX2_DISABLE_INSTANT_VU1").unwrap(), "0");
        assert_eq!(std::env::var("PCARMSX2_AUDIO_BACKEND").unwrap(), "Null");

        for (key, value) in previous {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn default_core_variables_enable_pcsx2_speedhack_baseline() {
        let variables = default_core_variables_for(
            "pcsx2",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );

        assert_eq!(
            variables
                .get("pcsx2_speedhacks_toggles")
                .expect("pcsx2 speedhacks default")
                .to_str()
                .expect("utf8"),
            "enabled"
        );
        assert_eq!(
            variables
                .get("pcsx2_mtvu")
                .expect("pcsx2 mtvu default")
                .to_str()
                .expect("utf8"),
            "enabled"
        );
        assert_eq!(
            variables
                .get("pcsx2_ee_cycle_rate")
                .expect("pcsx2 ee cycle rate default")
                .to_str()
                .expect("utf8"),
            "0"
        );
    }

    #[test]
    fn default_core_variables_honor_pcsx2_renderer_env_override() {
        let _guard = pcsx2_renderer_env_lock();
        let key = "ARCADE_PCSX2_RENDERER";
        let previous = std::env::var_os(key);
        std::env::set_var(key, "paraLLEl-GS");

        let variables = default_core_variables_for(
            "pcsx2",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );
        let renderer = variables
            .get("pcsx2_renderer")
            .expect("pcsx2 renderer override");

        assert_eq!(renderer.to_str().expect("utf8"), "paraLLEl-GS");

        if let Some(previous) = previous {
            std::env::set_var(key, previous);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn pcsx2_runtime_env_preserves_existing_vulkan_loader_override() {
        let key = "GRANITE_VULKAN_LIBRARY";
        let previous = std::env::var_os(key);
        std::env::set_var(key, "/tmp/custom-libvulkan.dylib");

        apply_core_runtime_env_defaults("pcsx2", VideoBackendKind::Vulkan);

        assert_eq!(
            std::env::var(key).expect("env value"),
            "/tmp/custom-libvulkan.dylib"
        );

        if let Some(previous) = previous {
            std::env::set_var(key, previous);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn default_core_variables_honor_dolphin_stability_overrides() {
        let mut emulation = EmulationConfig::default();
        emulation
            .core_settings
            .entry("dolphin".into())
            .or_default()
            .extend([
                ("dolphin_main_cpu_thread".into(), "disabled".into()),
                ("dolphin_cpu_core".into(), "5".into()),
                ("dolphin_fastmem".into(), "disabled".into()),
                ("dolphin_fastmem_arena".into(), "disabled".into()),
                ("dolphin_main_mmu".into(), "enabled".into()),
                ("dolphin_skip_gc_bios".into(), "disabled".into()),
            ]);
        let variables = default_core_variables_for("dolphin", VideoBackendKind::OpenGl, &emulation);

        assert_eq!(
            variables
                .get("dolphin_main_cpu_thread")
                .expect("dolphin main cpu thread override")
                .to_str()
                .expect("utf8"),
            "disabled"
        );
        assert_eq!(
            variables
                .get("dolphin_cpu_core")
                .expect("dolphin cpu core override")
                .to_str()
                .expect("utf8"),
            "5"
        );
        assert_eq!(
            variables
                .get("dolphin_fastmem")
                .expect("dolphin fastmem override")
                .to_str()
                .expect("utf8"),
            "disabled"
        );
        assert_eq!(
            variables
                .get("dolphin_fastmem_arena")
                .expect("dolphin fastmem arena override")
                .to_str()
                .expect("utf8"),
            "disabled"
        );
        assert_eq!(
            variables
                .get("dolphin_main_mmu")
                .expect("dolphin main MMU override")
                .to_str()
                .expect("utf8"),
            "enabled"
        );
        assert_eq!(
            variables
                .get("dolphin_skip_gc_bios")
                .expect("dolphin skip GameCube BIOS override")
                .to_str()
                .expect("utf8"),
            "disabled"
        );
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
        let emulation = emulation_with(
            "mupen64plus_next",
            "mupen64plus-parallel-rdp-upscaling",
            "2x",
        );
        let variables =
            default_core_variables_for("parallel_n64", VideoBackendKind::Vulkan, &emulation);
        let upscale = variables
            .get("parallel-n64-parallel-rdp-upscaling")
            .expect("parallel rdp upscaling override");

        assert_eq!(upscale.to_str().expect("utf8"), "2x");
    }

    #[test]
    fn mupen64plus_next_cpucore_env_override_updates_both_variable_names() {
        let env_key = "ARCADE_MUPEN64PLUS_NEXT_CPUCORE";
        let previous = std::env::var(env_key).ok();
        std::env::set_var(env_key, "cached_interpreter");

        let variables = default_core_variables_for(
            "mupen64plus_next",
            VideoBackendKind::Vulkan,
            &EmulationConfig::default(),
        );

        if let Some(previous) = previous {
            std::env::set_var(env_key, previous);
        } else {
            std::env::remove_var(env_key);
        }

        let cpucore = variables
            .get("mupen64plus-cpucore")
            .expect("mupen64plus cpucore override");
        let cpu_core = variables
            .get("mupen64plus-cpu-core")
            .expect("mupen64plus cpu-core override");

        assert_eq!(cpucore.to_str().expect("utf8"), "cached_interpreter");
        assert_eq!(cpu_core.to_str().expect("utf8"), "cached_interpreter");
    }

    #[test]
    fn mupen64plus_next_uses_configured_n64_cpu_core_mode() {
        let env_key = "ARCADE_MUPEN64PLUS_NEXT_CPUCORE";
        let previous_override = std::env::var(env_key).ok();
        std::env::remove_var(env_key);

        let mut emulation = EmulationConfig::default();
        emulation.n64.cpu_core_mode = arcade_domain::N64CpuCoreMode::DynamicRecompiler;

        let variables =
            default_core_variables_for("mupen64plus_next", VideoBackendKind::Vulkan, &emulation);
        let cpucore = variables
            .get("mupen64plus-cpucore")
            .expect("mupen64plus cpucore");
        let cpu_core = variables
            .get("mupen64plus-cpu-core")
            .expect("mupen64plus cpu-core");

        if let Some(previous) = previous_override {
            std::env::set_var(env_key, previous);
        }

        assert_eq!(cpucore.to_str().expect("utf8"), "dynamic_recompiler");
        assert_eq!(cpu_core.to_str().expect("utf8"), "dynamic_recompiler");
    }

    #[test]
    fn registry_profiles_produce_variables_for_all_cores() {
        for profile in arcade_domain::core_profiles() {
            let variables = default_core_variables_for(
                profile.core_name,
                VideoBackendKind::Software,
                &EmulationConfig::default(),
            );
            // Every registry variable should be present in the output
            for var_def in &profile.variables {
                assert!(
                    variables.contains_key(var_def.key),
                    "missing variable {} for core {}",
                    var_def.key,
                    profile.core_name,
                );
            }
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
