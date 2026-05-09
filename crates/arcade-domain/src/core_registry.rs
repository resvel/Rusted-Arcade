use std::collections::HashMap;

/// Metadata for a single allowed value of a core variable.
#[derive(Debug, Clone)]
pub struct CoreVariableOption {
    /// The value sent to the libretro core.
    pub value: &'static str,
    /// Optional display label for the UI. Falls back to `value` if `None`.
    pub display: Option<&'static str>,
}

/// Metadata describing a single configurable core variable.
#[derive(Debug, Clone)]
pub struct CoreVariableDefinition {
    /// The libretro variable key (e.g., `"mupen64plus-aspect"`).
    pub key: &'static str,
    /// Human-readable label shown in the UI.
    pub label: &'static str,
    /// Grouping header for the UI. Variables with the same group are rendered
    /// under the same heading.
    pub group: &'static str,
    /// Ordered list of allowed values. The first value is the default.
    pub options: Vec<CoreVariableOption>,
}

/// A full set of configurable variables for one libretro core.
#[derive(Debug, Clone)]
pub struct CoreProfile {
    /// Internal core identifier (e.g., `"mupen64plus_next"`).
    pub core_name: &'static str,
    /// Display name shown in the UI tab.
    pub display_name: &'static str,
    /// The system this core belongs to (e.g., `"N64"`, `"ARCADE"`).
    pub system: &'static str,
    /// Ordered list of user-configurable variables.
    pub variables: Vec<CoreVariableDefinition>,
}

fn opt(value: &'static str) -> CoreVariableOption {
    CoreVariableOption {
        value,
        display: None,
    }
}

fn opt_d(value: &'static str, display: &'static str) -> CoreVariableOption {
    CoreVariableOption {
        value,
        display: Some(display),
    }
}

fn var(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    options: Vec<CoreVariableOption>,
) -> CoreVariableDefinition {
    CoreVariableDefinition {
        key,
        label,
        group,
        options,
    }
}

fn enabled_options(default_enabled: bool) -> Vec<CoreVariableOption> {
    if default_enabled {
        vec![opt_d("enabled", "On"), opt_d("disabled", "Off")]
    } else {
        vec![opt_d("disabled", "Off"), opt_d("enabled", "On")]
    }
}

fn opt_values(values: &[&'static str]) -> Vec<CoreVariableOption> {
    values.iter().copied().map(opt).collect()
}

fn dolphin_cpu_core_options() -> Vec<CoreVariableOption> {
    #[cfg(target_arch = "aarch64")]
    {
        vec![
            opt_d("4", "JITARM64"),
            opt_d("0", "Interpreter"),
            opt_d("5", "Cached Interpreter"),
        ]
    }
    #[cfg(target_arch = "x86_64")]
    {
        vec![
            opt_d("1", "JIT64"),
            opt_d("0", "Interpreter"),
            opt_d("5", "Cached Interpreter"),
        ]
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        vec![opt_d("5", "Cached Interpreter"), opt_d("0", "Interpreter")]
    }
}

/// Returns all registered core profiles.
pub fn core_profiles() -> Vec<CoreProfile> {
    vec![
        mupen64plus_next_profile(),
        fceumm_profile(),
        snes9x_profile(),
        genesis_plus_gx_profile(),
        gambatte_profile(),
        mgba_profile(),
        fbneo_profile(),
        mame2003_plus_profile(),
        mednafen_psx_hw_profile(),
        pcsx2_profile(),
        pcarmsx2_profile(),
        play_profile(),
        flycast_profile(),
        dolphin_profile(),
        mednafen_saturn_profile(),
        mednafen_pce_fast_profile(),
        dosbox_pure_profile(),
    ]
}

// ---------------------------------------------------------------------------
// GameCube: dolphin
// ---------------------------------------------------------------------------

fn dolphin_profile() -> CoreProfile {
    CoreProfile {
        core_name: "dolphin",
        display_name: "GameCube",
        system: "GAMECUBE",
        variables: vec![
            var(
                "dolphin_renderer",
                "Renderer",
                "Display",
                vec![opt("Hardware"), opt("Software")],
            ),
            var(
                "dolphin_aspect_ratio",
                "Aspect Ratio",
                "Display",
                vec![
                    opt_d("3", "Stretch"),
                    opt_d("0", "Auto"),
                    opt_d("1", "Force Wide"),
                    opt_d("2", "Force Standard"),
                    opt_d("4", "Custom"),
                    opt_d("5", "Custom Stretch"),
                    opt_d("6", "Raw"),
                ],
            ),
            var(
                "dolphin_crop_overscan",
                "Crop Overscan",
                "Display",
                enabled_options(false),
            ),
            var(
                "dolphin_efb_scale",
                "Internal Resolution",
                "Display",
                vec![
                    opt_d("1", "1x Native"),
                    opt_d("2", "2x"),
                    opt_d("3", "3x"),
                    opt_d("4", "4x"),
                    opt_d("5", "5x"),
                    opt_d("6", "6x"),
                ],
            ),
            var(
                "dolphin_widescreen_hack",
                "Widescreen Hack",
                "Display",
                enabled_options(false),
            ),
            var(
                "dolphin_shader_compilation_mode",
                "Shader Compilation",
                "Display",
                vec![
                    opt_d("0", "Synchronous"),
                    opt_d("3", "Async Skip Rendering"),
                    opt_d("1", "Sync UberShaders"),
                    opt_d("2", "Async UberShaders"),
                ],
            ),
            var(
                "dolphin_wait_for_shaders",
                "Wait for Shaders",
                "Display",
                enabled_options(false),
            ),
            var(
                "dolphin_anti_aliasing",
                "Anti-Aliasing",
                "Display",
                vec![
                    opt_d("0", "None"),
                    opt_d("1", "2x MSAA"),
                    opt_d("2", "4x MSAA"),
                    opt_d("3", "8x MSAA"),
                    opt_d("4", "2x SSAA"),
                    opt_d("5", "4x SSAA"),
                    opt_d("6", "8x SSAA"),
                ],
            ),
            var(
                "dolphin_progressive_scan",
                "Progressive Scan",
                "Video",
                enabled_options(true),
            ),
            var("dolphin_pal60", "PAL60", "Video", enabled_options(true)),
            var(
                "dolphin_max_anisotropy",
                "Max Anisotropy",
                "Video",
                vec![
                    opt_d("0", "1x"),
                    opt_d("1", "2x"),
                    opt_d("2", "4x"),
                    opt_d("3", "8x"),
                    opt_d("4", "16x"),
                ],
            ),
            var(
                "dolphin_efb_scaled_copy",
                "Scaled EFB Copy",
                "Video",
                enabled_options(true),
            ),
            var(
                "dolphin_texture_cache_accuracy",
                "Texture Cache Accuracy",
                "Video",
                vec![
                    opt_d("128", "Fast"),
                    opt_d("512", "Middle"),
                    opt_d("0", "Safe"),
                ],
            ),
            var(
                "dolphin_gpu_texture_decoding",
                "GPU Texture Decoding",
                "Video",
                enabled_options(false),
            ),
            var(
                "dolphin_pixel_lighting",
                "Pixel Lighting",
                "Video",
                enabled_options(false),
            ),
            var(
                "dolphin_fast_depth_calculation",
                "Fast Depth Calculation",
                "Video",
                enabled_options(true),
            ),
            var(
                "dolphin_disable_fog",
                "Disable Fog",
                "Video",
                enabled_options(false),
            ),
            var(
                "dolphin_force_texture_filtering_mode",
                "Texture Filtering",
                "Enhancements",
                vec![
                    opt_d("0", "Default"),
                    opt_d("1", "Nearest"),
                    opt_d("2", "Linear"),
                ],
            ),
            var(
                "dolphin_load_custom_textures",
                "Load Custom Textures",
                "Enhancements",
                enabled_options(false),
            ),
            var(
                "dolphin_cache_custom_textures",
                "Prefetch Custom Textures",
                "Enhancements",
                enabled_options(false),
            ),
            var(
                "dolphin_enhance_output_resampling",
                "Output Resampling",
                "Enhancements",
                vec![
                    opt_d("0", "Default"),
                    opt_d("1", "Bilinear"),
                    opt_d("2", "B-Spline"),
                    opt_d("3", "Mitchell-Netravali"),
                    opt_d("4", "Catmull-Rom"),
                    opt_d("5", "Sharp Bilinear"),
                    opt_d("6", "Area Sampling"),
                ],
            ),
            var(
                "dolphin_force_true_color",
                "Force True Color",
                "Enhancements",
                enabled_options(true),
            ),
            var(
                "dolphin_disable_copy_filter",
                "Disable Copy Filter",
                "Enhancements",
                enabled_options(true),
            ),
            var(
                "dolphin_enhance_hdr_output",
                "HDR Output",
                "Enhancements",
                enabled_options(false),
            ),
            var(
                "dolphin_mipmap_detection",
                "Arbitrary Mipmap Detection",
                "Enhancements",
                enabled_options(false),
            ),
            var(
                "dolphin_efb_access_enable",
                "EFB Access from CPU",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_efb_access_defer_invalidation",
                "EFB Access Defer Invalidation",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_efb_access_tile_size",
                "EFB Access Tile Size",
                "Graphics Hacks",
                vec![
                    opt_d("64", "64"),
                    opt_d("32", "32"),
                    opt_d("16", "16"),
                    opt_d("8", "8"),
                    opt_d("4", "4"),
                    opt_d("1", "1"),
                ],
            ),
            var(
                "dolphin_bbox_enabled",
                "Bounding Box",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_force_progressive",
                "Force Progressive",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_efb_to_texture",
                "Skip EFB Copy to RAM",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_xfb_to_texture_enable",
                "Skip XFB Copy to RAM",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_efb_to_vram",
                "Disable EFB to VRAM",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_defer_efb_copies",
                "Defer EFB Copies",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_immediate_xfb",
                "Immediate XFB",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_skip_dupe_frames",
                "Skip Duplicate Frames",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_early_xfb_output",
                "Early XFB Output",
                "Graphics Hacks",
                enabled_options(true),
            ),
            var(
                "dolphin_efb_emulate_format_changes",
                "EFB Format Changes",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_vertex_rounding",
                "Vertex Rounding",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_vi_skip",
                "VI Skip",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_fast_texture_sampling",
                "Fast Texture Sampling",
                "Graphics Hacks",
                enabled_options(true),
            ),
            #[cfg(target_os = "macos")]
            var(
                "dolphin_no_mipmapping",
                "Disable Mipmapping",
                "Graphics Hacks",
                enabled_options(false),
            ),
            var(
                "dolphin_main_cpu_thread",
                "Dual Core Mode",
                "Performance",
                enabled_options(true),
            ),
            var(
                "dolphin_cpu_core",
                "CPU Core",
                "Performance",
                dolphin_cpu_core_options(),
            ),
            var(
                "dolphin_cpu_clock_rate",
                "CPU Clock Rate",
                "Performance",
                vec![
                    opt_d("1.00", "100%"),
                    opt_d("0.05", "5%"),
                    opt_d("0.10", "10%"),
                    opt_d("0.20", "20%"),
                    opt_d("0.30", "30%"),
                    opt_d("0.40", "40%"),
                    opt_d("0.50", "50%"),
                    opt_d("0.60", "60%"),
                    opt_d("0.70", "70%"),
                    opt_d("0.80", "80%"),
                    opt_d("0.90", "90%"),
                    opt_d("1.50", "150%"),
                    opt_d("2.00", "200%"),
                    opt_d("2.50", "250%"),
                    opt_d("3.00", "300%"),
                ],
            ),
            var(
                "dolphin_emulation_speed",
                "Emulation Speed",
                "Performance",
                vec![opt_d("0.0", "Unlimited"), opt_d("1.0", "100%")],
            ),
            var(
                "dolphin_precision_frame_timing",
                "Precision Frame Timing",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_fastmem",
                "Fastmem",
                "Performance",
                enabled_options(true),
            ),
            var(
                "dolphin_fastmem_arena",
                "Fastmem Arena",
                "Performance",
                enabled_options(true),
            ),
            var(
                "dolphin_main_accurate_cpu_cache",
                "Accurate CPU Cache",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_main_mmu",
                "MMU",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_fast_disc_speed",
                "Fast Disc Speed",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_rush_presentation",
                "Rush Presentation",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_early_presentation",
                "Smooth Early Presentation",
                "Performance",
                enabled_options(false),
            ),
            var(
                "dolphin_call_back_audio_method",
                "Audio Callback",
                "Audio",
                vec![
                    opt_d("0", "Sync Samples"),
                    opt_d("1", "Sync Per Frame"),
                    opt_d("2", "Async Callback"),
                ],
            ),
            var(
                "dolphin_dsp_hle",
                "DSP HLE",
                "Audio",
                vec![opt_d("enabled", "HLE"), opt_d("disabled", "LLE")],
            ),
            var("dolphin_dsp_jit", "DSP JIT", "Audio", enabled_options(true)),
            var(
                "dolphin_osd_enabled",
                "On-Screen Display",
                "Interface",
                enabled_options(true),
            ),
            var(
                "dolphin_log_level",
                "Log Level",
                "Interface",
                vec![
                    opt_d("4", "Info"),
                    opt_d("1", "Notice"),
                    opt_d("2", "Error"),
                    opt_d("3", "Warning"),
                    opt_d("5", "Debug"),
                ],
            ),
            var(
                "dolphin_debug_mode_enabled",
                "Debug Mode",
                "Interface",
                enabled_options(false),
            ),
            var(
                "dolphin_cheats_enabled",
                "Internal Cheats",
                "System",
                enabled_options(false),
            ),
            var(
                "dolphin_cheats_import",
                "Import Cheats",
                "System",
                enabled_options(true),
            ),
            var(
                "dolphin_skip_gc_bios",
                "Skip GameCube BIOS",
                "System",
                enabled_options(true),
            ),
            var(
                "dolphin_language",
                "Language",
                "System",
                vec![
                    opt_d("1", "English"),
                    opt_d("0", "Japanese"),
                    opt_d("2", "German"),
                    opt_d("3", "French"),
                    opt_d("4", "Spanish"),
                    opt_d("5", "Italian"),
                    opt_d("6", "Dutch"),
                    opt_d("7", "Simplified Chinese"),
                    opt_d("8", "Traditional Chinese"),
                    opt_d("9", "Korean"),
                ],
            ),
            var(
                "dolphin_gc_sp1",
                "SP1 Device",
                "System",
                vec![
                    opt_d("255", "None"),
                    opt_d("0", "Dummy"),
                    opt_d("5", "Ethernet"),
                    opt_d("10", "Ethernet XLink"),
                    opt_d("11", "Ethernet TapServer"),
                    opt_d("12", "Ethernet Built-In"),
                    opt_d("13", "Modem TapServer"),
                    opt_d("6", "Baseboard"),
                ],
            ),
            var(
                "dolphin_enable_gamecube_mic",
                "GameCube Microphone",
                "System",
                enabled_options(false),
            ),
            var(
                "dolphin_hotkey_activate_microphone",
                "Microphone Button",
                "System",
                vec![
                    opt_d("Disabled", "Disabled"),
                    opt("L3"),
                    opt("R3"),
                    opt("L1"),
                    opt("R1"),
                    opt("L2"),
                    opt("R2"),
                    opt("A"),
                    opt("B"),
                    opt("X"),
                    opt("Y"),
                    opt("Start"),
                    opt("Select"),
                ],
            ),
            var(
                "dolphin_widescreen",
                "Widescreen (Wii)",
                "Wii",
                enabled_options(true),
            ),
            var(
                "dolphin_sensor_bar_position",
                "Sensor Bar Position",
                "Wii",
                vec![opt_d("0", "Bottom"), opt_d("1", "Top")],
            ),
            var(
                "dolphin_enable_rumble",
                "Controller Rumble",
                "Wii",
                enabled_options(true),
            ),
            var(
                "dolphin_wiimote_continuous_scanning",
                "Wiimote Continuous Scanning",
                "Wii",
                enabled_options(false),
            ),
            var(
                "dolphin_alt_gc_ports_on_wii",
                "Alt GC Ports (Wii)",
                "Wii",
                enabled_options(false),
            ),
            var(
                "dolphin_bluetooth_passthrough",
                "Bluetooth Passthrough",
                "Wii",
                enabled_options(false),
            ),
            var(
                "dolphin_wiispeak_enable",
                "Wii Speak",
                "Wii",
                enabled_options(false),
            ),
            var(
                "dolphin_wiispeak_muted",
                "Wii Speak Muted",
                "Wii",
                enabled_options(true),
            ),
            var(
                "dolphin_wii_logi_microphone_enable",
                "Logitech USB Microphone",
                "Wii",
                enabled_options(false),
            ),
            var(
                "dolphin_hotkey_sideways_toggle",
                "Wiimote Sideways Toggle",
                "Wiimote",
                vec![
                    opt("L3"),
                    opt_d("Disabled", "Disabled"),
                    opt("R3"),
                    opt("L1"),
                    opt("R1"),
                    opt("L2"),
                    opt("R2"),
                    opt("A"),
                    opt("B"),
                    opt("X"),
                    opt("Y"),
                    opt("Start"),
                    opt("Select"),
                ],
            ),
            var(
                "dolphin_ir_mode",
                "Wiimote IR Mode",
                "Wiimote",
                vec![
                    opt_d("1", "Right Stick Absolute"),
                    opt_d("0", "Right Stick Relative"),
                    opt_d("2", "Mouse"),
                ],
            ),
            var(
                "dolphin_ir_offset",
                "IR Vertical Offset",
                "Wiimote",
                opt_values(&[
                    "0", "-50", "-49", "-48", "-47", "-46", "-45", "-44", "-43", "-42", "-41",
                    "-40", "-39", "-38", "-37", "-36", "-35", "-34", "-33", "-32", "-31", "-30",
                    "-29", "-28", "-27", "-26", "-25", "-24", "-23", "-22", "-21", "-20", "-19",
                    "-18", "-17", "-16", "-15", "-14", "-13", "-12", "-11", "-10", "-9", "-8",
                    "-7", "-6", "-5", "-4", "-3", "-2", "-1", "1", "2", "3", "4", "5", "6", "7",
                    "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20",
                    "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31", "32", "33",
                    "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
                    "47", "48", "49", "50",
                ]),
            ),
            var(
                "dolphin_ir_yaw",
                "IR Total Yaw",
                "Wiimote",
                opt_values(&[
                    "25", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13",
                    "14", "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "26", "27",
                    "28", "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "40",
                    "41", "42", "43", "44", "45", "46", "47", "48", "49", "50", "51", "52", "53",
                    "54", "55", "56", "57", "58", "59", "60", "61", "62", "63", "64", "65", "66",
                    "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77", "78", "79",
                    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "90", "91", "92",
                    "93", "94", "95", "96", "97", "98", "99", "100",
                ]),
            ),
            var(
                "dolphin_ir_pitch",
                "IR Total Pitch",
                "Wiimote",
                opt_values(&[
                    "25", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13",
                    "14", "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "26", "27",
                    "28", "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "40",
                    "41", "42", "43", "44", "45", "46", "47", "48", "49", "50", "51", "52", "53",
                    "54", "55", "56", "57", "58", "59", "60", "61", "62", "63", "64", "65", "66",
                    "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77", "78", "79",
                    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "90", "91", "92",
                    "93", "94", "95", "96", "97", "98", "99", "100",
                ]),
            ),
            var(
                "dolphin_ir_deadzone",
                "IR Deadzone",
                "Wiimote",
                opt_values(&[
                    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14",
                    "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27",
                    "28", "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "40",
                    "41", "42", "43", "44", "45", "46", "47", "48", "49", "50",
                ]),
            ),
            var(
                "dolphin_ir_modifier",
                "IR Modifier Button",
                "Wiimote",
                vec![
                    opt_d("None", "None"),
                    opt("L3"),
                    opt("R3"),
                    opt("L1"),
                    opt("R1"),
                    opt("L2"),
                    opt("R2"),
                    opt("A"),
                    opt("B"),
                    opt("X"),
                    opt("Y"),
                    opt("Start"),
                    opt("Select"),
                ],
            ),
            var(
                "dolphin_swing_modifier",
                "Swing Modifier Button",
                "Wiimote",
                vec![
                    opt_d("Disabled", "Disabled"),
                    opt_d("None", "None"),
                    opt("L3"),
                    opt("R3"),
                    opt("L1"),
                    opt("R1"),
                    opt("L2"),
                    opt("R2"),
                    opt("A"),
                    opt("B"),
                    opt("X"),
                    opt("Y"),
                    opt("Start"),
                    opt("Select"),
                ],
            ),
            var(
                "dolphin_swing_angle",
                "Swing Angle",
                "Wiimote",
                vec![
                    opt("90"),
                    opt("1"),
                    opt("5"),
                    opt("10"),
                    opt("15"),
                    opt("20"),
                    opt("25"),
                    opt("30"),
                    opt("35"),
                    opt("40"),
                    opt("45"),
                    opt("50"),
                    opt("55"),
                    opt("60"),
                    opt("65"),
                    opt("70"),
                    opt("75"),
                    opt("80"),
                    opt("85"),
                    opt("100"),
                    opt("110"),
                    opt("120"),
                    opt("130"),
                    opt("140"),
                    opt("150"),
                    opt("160"),
                    opt("170"),
                    opt("180"),
                ],
            ),
            var(
                "dolphin_save_load_settings",
                "Load and Prevent Save Settings",
                "Wiimote",
                enabled_options(false),
            ),
        ],
    }
}

/// Look up the profile for a specific core.
pub fn core_profile_for(core_name: &str) -> Option<CoreProfile> {
    core_profiles()
        .into_iter()
        .find(|p| p.core_name == core_name)
}

/// Resolve the current value for a core variable from the settings map,
/// falling back to the registry default.
pub fn resolve_core_variable(
    core_settings: &HashMap<String, HashMap<String, String>>,
    core_name: &str,
    def: &CoreVariableDefinition,
) -> String {
    core_settings
        .get(core_name)
        .and_then(|vars| vars.get(def.key))
        .cloned()
        .unwrap_or_else(|| {
            def.options
                .first()
                .map(|o| o.value.to_string())
                .unwrap_or_default()
        })
}

// ---------------------------------------------------------------------------
// N64: mupen64plus_next
// ---------------------------------------------------------------------------

fn mupen64plus_next_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mupen64plus_next",
        display_name: "N64",
        system: "N64",
        variables: vec![
            var(
                "mupen64plus-aspect",
                "Aspect Ratio",
                "Display",
                vec![
                    opt("4:3"),
                    opt("16:9"),
                    opt_d("16:9 adjusted", "16:9 Adjusted"),
                ],
            ),
            var(
                "mupen64plus-parallel-rdp-upscaling",
                "Internal Resolution",
                "Display",
                vec![opt("1x"), opt("2x"), opt("4x"), opt("8x")],
            ),
            var(
                "mupen64plus-parallel-rdp-synchronous",
                "Synchronous Rendering",
                "ParaLLEl RDP",
                vec![opt("false"), opt("true")],
            ),
            var(
                "mupen64plus-parallel-rdp-super-sampled-read-back",
                "Super-Sampled Read-Back",
                "ParaLLEl RDP",
                vec![opt("false"), opt("true")],
            ),
            var(
                "mupen64plus-parallel-rdp-vi-aa",
                "VI Anti-Aliasing",
                "ParaLLEl RDP",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mupen64plus-parallel-rdp-vi-bilinear",
                "VI Bilinear Filtering",
                "ParaLLEl RDP",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mupen64plus-parallel-rdp-dither-filter",
                "Dither Filter",
                "ParaLLEl RDP",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mupen64plus-parallel-rdp-divot-filter",
                "Divot Filter",
                "ParaLLEl RDP",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mupen64plus-parallel-rdp-gamma-dither",
                "Gamma Dither",
                "ParaLLEl RDP",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mupen64plus-EnableFBEmulation",
                "Framebuffer Emulation",
                "Emulation",
                vec![opt_d("True", "On"), opt_d("False", "Off")],
            ),
            var(
                "mupen64plus-EnableCopyColorToRDRAM",
                "Copy Color to RDRAM",
                "Emulation",
                vec![opt("Async"), opt("Off"), opt_d("Software", "Sync")],
            ),
            var(
                "mupen64plus-FrameDuping",
                "Frame Duplication",
                "Performance",
                vec![opt_d("False", "Off"), opt_d("True", "On")],
            ),
            var(
                "mupen64plus-Framerate",
                "Framerate",
                "Performance",
                vec![opt("Original"), opt("Fullspeed")],
            ),
            var(
                "mupen64plus-virefresh",
                "VI Refresh Rate",
                "Performance",
                vec![opt("Auto"), opt("1500"), opt("2200")],
            ),
            var(
                "mupen64plus-CountPerOp",
                "Count Per Op",
                "Performance",
                vec![opt_d("0", "Auto"), opt("1"), opt("2"), opt("3")],
            ),
            var(
                "mupen64plus-CountPerOpDenomPot",
                "Count Per Op Denom Pot",
                "Performance",
                vec![
                    opt("0"),
                    opt("1"),
                    opt("2"),
                    opt("3"),
                    opt("4"),
                    opt("5"),
                    opt("6"),
                    opt("7"),
                    opt("8"),
                    opt("9"),
                    opt("10"),
                    opt("11"),
                ],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// NES: fceumm
// ---------------------------------------------------------------------------

fn fceumm_profile() -> CoreProfile {
    CoreProfile {
        core_name: "fceumm",
        display_name: "NES",
        system: "NES",
        variables: vec![
            var(
                "fceumm_palette",
                "Color Palette",
                "Display",
                vec![
                    opt("default"),
                    opt_d("asqrealc", "ASQ Real"),
                    opt_d("nintendo-vc", "Nintendo VC"),
                    opt("rgb"),
                    opt_d("yuv-v3", "YUV v3"),
                    opt_d("unsaturated-final", "Unsaturated Final"),
                    opt_d("smooth-fbx", "Smooth FBX"),
                    opt_d("composite-direct-fbx", "Composite Direct FBX"),
                    opt_d("nes-classic-fbx-fs", "NES Classic FBX"),
                    opt("wavebeam"),
                    opt("raw"),
                ],
            ),
            var(
                "fceumm_ntsc_filter",
                "NTSC Filter",
                "Display",
                vec![
                    opt("disabled"),
                    opt("composite"),
                    opt("svideo"),
                    opt("rgb"),
                    opt("monochrome"),
                ],
            ),
            var(
                "fceumm_aspect",
                "Aspect Ratio",
                "Display",
                vec![opt("8:7 PAR"), opt("4:3")],
            ),
            var(
                "fceumm_overscan_h",
                "Crop Horizontal Overscan",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fceumm_overscan_v",
                "Crop Vertical Overscan",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fceumm_region",
                "Region",
                "System",
                vec![opt("Auto"), opt("NTSC"), opt("PAL"), opt("Dendy")],
            ),
            var(
                "fceumm_overclocking",
                "Overclocking",
                "Performance",
                vec![
                    opt("disabled"),
                    opt_d("2x-Postrender", "2x Post-render"),
                    opt_d("2x-VBlank", "2x VBlank"),
                ],
            ),
            var(
                "fceumm_nospritelimit",
                "Remove Sprite Limit",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fceumm_sndquality",
                "Sound Quality",
                "Audio",
                vec![opt("Low"), opt("High"), opt_d("Very High", "Very High")],
            ),
            var(
                "fceumm_sndlowpass",
                "Low-Pass Audio Filter",
                "Audio",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fceumm_ramstate",
                "RAM Power-On State",
                "System",
                vec![opt("Fill $00"), opt("Fill $FF"), opt("Random")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// SNES: snes9x
// ---------------------------------------------------------------------------

fn snes9x_profile() -> CoreProfile {
    CoreProfile {
        core_name: "snes9x",
        display_name: "SNES",
        system: "SNES",
        variables: vec![
            var(
                "snes9x_aspect",
                "Aspect Ratio",
                "Display",
                vec![
                    opt("auto"),
                    opt("ntsc"),
                    opt("pal"),
                    opt("4:3"),
                    opt("uncorrected"),
                ],
            ),
            var(
                "snes9x_overscan",
                "Crop Overscan",
                "Display",
                vec![opt("enabled"), opt("disabled"), opt("auto")],
            ),
            var(
                "snes9x_gfx_hires",
                "Hi-Res Blending",
                "Display",
                vec![opt("enabled"), opt("disabled"), opt("merge")],
            ),
            var(
                "snes9x_region",
                "Console Region",
                "System",
                vec![opt("auto"), opt("ntsc"), opt("pal")],
            ),
            var(
                "snes9x_overclock_superfx",
                "SuperFX Overclock",
                "Performance",
                vec![
                    opt("100%"),
                    opt("150%"),
                    opt("200%"),
                    opt("250%"),
                    opt("300%"),
                    opt("50%"),
                    opt("60%"),
                    opt("70%"),
                    opt("80%"),
                    opt("90%"),
                ],
            ),
            var(
                "snes9x_overclock_cycles",
                "Reduce Slowdown",
                "Performance",
                vec![opt("disabled"), opt("light"), opt("compatible"), opt("max")],
            ),
            var(
                "snes9x_reduce_sprite_flicker",
                "Reduce Sprite Flicker",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "snes9x_audio_interpolation",
                "Audio Interpolation",
                "Audio",
                vec![
                    opt("gaussian"),
                    opt("cubic"),
                    opt("sinc"),
                    opt("none"),
                    opt("linear"),
                ],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Genesis: genesis_plus_gx
// ---------------------------------------------------------------------------

fn genesis_plus_gx_profile() -> CoreProfile {
    CoreProfile {
        core_name: "genesis_plus_gx",
        display_name: "Genesis",
        system: "GENESIS",
        variables: vec![
            var(
                "genesis_plus_gx_system_hw",
                "System Hardware",
                "System",
                vec![
                    opt("auto"),
                    opt_d("sg-1000", "SG-1000"),
                    opt_d("sg-1000 II", "SG-1000 II"),
                    opt_d("mark-III", "Mark III"),
                    opt_d("master system", "Master System"),
                    opt_d("master system II", "Master System II"),
                    opt_d("game gear", "Game Gear"),
                    opt_d("mega drive / genesis", "Mega Drive / Genesis"),
                ],
            ),
            var(
                "genesis_plus_gx_region_detect",
                "System Region",
                "System",
                vec![
                    opt("auto"),
                    opt_d("ntsc-u", "NTSC-U"),
                    opt_d("pal", "PAL"),
                    opt_d("ntsc-j", "NTSC-J"),
                ],
            ),
            var(
                "genesis_plus_gx_aspect_ratio",
                "Aspect Ratio",
                "Display",
                vec![opt("auto"), opt("NTSC PAR"), opt("PAL PAR")],
            ),
            var(
                "genesis_plus_gx_overscan",
                "Borders",
                "Display",
                vec![
                    opt("disabled"),
                    opt("top/bottom"),
                    opt("left/right"),
                    opt("full"),
                ],
            ),
            var(
                "genesis_plus_gx_blargg_ntsc_filter",
                "NTSC Filter",
                "Display",
                vec![
                    opt("disabled"),
                    opt("monochrome"),
                    opt("composite"),
                    opt("svideo"),
                    opt("rgb"),
                ],
            ),
            var(
                "genesis_plus_gx_render",
                "Interlaced Mode 2 Output",
                "Display",
                vec![opt("single field"), opt("double field")],
            ),
            var(
                "genesis_plus_gx_overclock",
                "CPU Speed",
                "Performance",
                vec![
                    opt("100%"),
                    opt("125%"),
                    opt("150%"),
                    opt("175%"),
                    opt("200%"),
                ],
            ),
            var(
                "genesis_plus_gx_no_sprite_limit",
                "Remove Sprite Limit",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "genesis_plus_gx_ym2612",
                "FM Synthesis Model",
                "Audio",
                vec![
                    opt_d("mame (ym2612)", "MAME YM2612"),
                    opt_d("mame (asic ym3438)", "MAME ASIC YM3438"),
                    opt_d("mame (enhanced ym3438)", "MAME Enhanced YM3438"),
                    opt_d("nuked (ym2612)", "Nuked YM2612"),
                    opt_d("nuked (asic ym3438)", "Nuked ASIC YM3438"),
                    opt_d("nuked (discrete ym3438)", "Nuked Discrete YM3438"),
                ],
            ),
            var(
                "genesis_plus_gx_sound_output",
                "Sound Output",
                "Audio",
                vec![opt("stereo"), opt("mono")],
            ),
            var(
                "genesis_plus_gx_audio_filter",
                "Audio Filter",
                "Audio",
                vec![opt("disabled"), opt("low-pass")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Game Boy / GBC: gambatte
// ---------------------------------------------------------------------------

fn gambatte_profile() -> CoreProfile {
    CoreProfile {
        core_name: "gambatte",
        display_name: "Game Boy",
        system: "GB",
        variables: vec![
            var(
                "gambatte_gb_colorization",
                "GB Colorization",
                "Display",
                vec![
                    opt("disabled"),
                    opt("auto"),
                    opt("GBC"),
                    opt("SGB"),
                    opt("internal"),
                    opt("custom"),
                ],
            ),
            var(
                "gambatte_gbc_color_correction",
                "Color Correction",
                "Display",
                vec![opt("GBC only"), opt("always"), opt("disabled")],
            ),
            var(
                "gambatte_gbc_color_correction_mode",
                "Color Correction Mode",
                "Display",
                vec![opt("accurate"), opt("fast")],
            ),
            var(
                "gambatte_mix_frames",
                "Interframe Blending",
                "Display",
                vec![
                    opt("disabled"),
                    opt("mix"),
                    opt_d("lcd_ghosting", "LCD Ghosting"),
                    opt_d("lcd_ghosting_fast", "LCD Ghosting (Fast)"),
                ],
            ),
            var(
                "gambatte_gb_hwmode",
                "Emulated Hardware",
                "System",
                vec![opt("Auto"), opt("GB"), opt("GBC"), opt("GBA")],
            ),
            var(
                "gambatte_gb_bootloader",
                "Use Official Bootloader",
                "System",
                vec![opt("enabled"), opt("disabled")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// GBA: mgba
// ---------------------------------------------------------------------------

fn mgba_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mgba",
        display_name: "GBA",
        system: "GBA",
        variables: vec![
            var(
                "mgba_gb_model",
                "Game Boy Model",
                "System",
                vec![
                    opt("Autodetect"),
                    opt("Game Boy"),
                    opt("Super Game Boy"),
                    opt("Game Boy Color"),
                    opt("Game Boy Advance"),
                ],
            ),
            var(
                "mgba_use_bios",
                "Use BIOS File If Found",
                "System",
                vec![opt("ON"), opt("OFF")],
            ),
            var(
                "mgba_skip_bios",
                "Skip BIOS Intro",
                "System",
                vec![opt("OFF"), opt("ON")],
            ),
            var(
                "mgba_sgb_borders",
                "Super Game Boy Borders",
                "Display",
                vec![opt("ON"), opt("OFF")],
            ),
            var(
                "mgba_color_correction",
                "Color Correction",
                "Display",
                vec![opt("OFF"), opt("GBA"), opt("GBC"), opt("Auto")],
            ),
            var(
                "mgba_interframe_blending",
                "Interframe Blending",
                "Display",
                vec![
                    opt("OFF"),
                    opt("mix"),
                    opt_d("lcd_ghosting", "LCD Ghosting"),
                ],
            ),
            var(
                "mgba_frameskip",
                "Frameskip",
                "Performance",
                vec![
                    opt("disabled"),
                    opt("auto"),
                    opt_d("auto_threshold", "Auto (Threshold)"),
                    opt_d("fixed_interval", "Fixed Interval"),
                ],
            ),
            var(
                "mgba_idle_optimization",
                "Idle Loop Removal",
                "Performance",
                vec![
                    opt("Remove Known"),
                    opt("Detect and Remove"),
                    opt("Don't Remove"),
                ],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Arcade: fbneo
// ---------------------------------------------------------------------------

fn fbneo_profile() -> CoreProfile {
    CoreProfile {
        core_name: "fbneo",
        display_name: "Arcade (FBNeo)",
        system: "ARCADE",
        variables: vec![
            var(
                "fbneo-cpu-speed-adjust",
                "CPU Speed (%)",
                "Performance",
                vec![
                    opt("100"),
                    opt("110"),
                    opt("120"),
                    opt("130"),
                    opt("140"),
                    opt("150"),
                    opt("160"),
                    opt("170"),
                    opt("180"),
                    opt("190"),
                    opt("200"),
                    opt("50"),
                    opt("60"),
                    opt("70"),
                    opt("80"),
                    opt("90"),
                ],
            ),
            var(
                "fbneo-frameskip",
                "Frameskip",
                "Performance",
                vec![opt("0"), opt("1"), opt("2"), opt("3"), opt("4"), opt("5")],
            ),
            var(
                "fbneo-frameskip-type",
                "Frameskip Type",
                "Performance",
                vec![opt("disabled"), opt("Fixed"), opt("Auto")],
            ),
            var(
                "fbneo-neogeo-mode",
                "Neo Geo Mode",
                "System",
                vec![opt("MVS"), opt("AES"), opt("UNIBIOS")],
            ),
            var(
                "fbneo-samplerate",
                "Sample Rate",
                "Audio",
                vec![opt("48000"), opt("44100"), opt("22050"), opt("11025")],
            ),
            var(
                "fbneo-sample-interpolation",
                "Sample Interpolation",
                "Audio",
                vec![
                    opt_d("4-pointed 3rd order", "4-pointed 3rd Order"),
                    opt_d("2-pointed 1st order", "2-pointed 1st Order"),
                    opt("disabled"),
                ],
            ),
            var(
                "fbneo-lowpass-filter",
                "Low-Pass Audio Filter",
                "Audio",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fbneo-vertical-mode",
                "Vertical Mode",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fbneo-force-60hz",
                "Force 60Hz",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "fbneo-hiscores",
                "Hiscores",
                "System",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "fbneo-memcard-mode",
                "Memory Card Mode",
                "System",
                vec![opt("disabled"), opt("shared"), opt("per-game")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Arcade: mame2003_plus
// ---------------------------------------------------------------------------

fn mame2003_plus_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mame2003_plus",
        display_name: "Arcade (MAME 2003+)",
        system: "ARCADE",
        variables: vec![
            var(
                "mame2003-plus_skip_disclaimer",
                "Skip Disclaimer",
                "System",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "mame2003-plus_skip_warnings",
                "Skip Warnings",
                "System",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "mame2003-plus_display_setup",
                "Display MAME Menu",
                "System",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mame2003-plus_frameskip",
                "Frameskip",
                "Performance",
                vec![opt("0"), opt("1"), opt("2"), opt("3"), opt("4"), opt("5")],
            ),
            var(
                "mame2003-plus_sample_rate",
                "Sample Rate (KHz)",
                "Audio",
                vec![opt("48000"), opt("44100"), opt("22050"), opt("11025")],
            ),
            var(
                "mame2003-plus_input_interface",
                "Input Interface",
                "Input",
                vec![opt("simultaneous"), opt("keyboard"), opt("gamepad")],
            ),
            var(
                "mame2003-plus_brightness",
                "Brightness",
                "Display",
                vec![
                    opt("1.0"),
                    opt("0.2"),
                    opt("0.3"),
                    opt("0.4"),
                    opt("0.5"),
                    opt("0.6"),
                    opt("0.7"),
                    opt("0.8"),
                    opt("0.9"),
                    opt("1.1"),
                    opt("1.2"),
                    opt("1.3"),
                    opt("1.4"),
                    opt("1.5"),
                    opt("1.6"),
                    opt("1.7"),
                    opt("1.8"),
                    opt("1.9"),
                    opt("2.0"),
                ],
            ),
            var(
                "mame2003-plus_gamma",
                "Gamma Correction",
                "Display",
                vec![
                    opt("1.0"),
                    opt("0.2"),
                    opt("0.3"),
                    opt("0.4"),
                    opt("0.5"),
                    opt("0.6"),
                    opt("0.7"),
                    opt("0.8"),
                    opt("0.9"),
                    opt("1.1"),
                    opt("1.2"),
                    opt("1.3"),
                    opt("1.4"),
                    opt("1.5"),
                    opt("1.6"),
                    opt("1.7"),
                    opt("1.8"),
                    opt("1.9"),
                    opt("2.0"),
                ],
            ),
            var(
                "mame2003-plus_cpu_clock_scale",
                "CPU Clock Scale (%)",
                "Performance",
                vec![
                    opt("100"),
                    opt("50"),
                    opt("60"),
                    opt("70"),
                    opt("80"),
                    opt("90"),
                    opt("110"),
                    opt("120"),
                    opt("130"),
                    opt("140"),
                    opt("150"),
                ],
            ),
            var(
                "mame2003-plus_tate_mode",
                "TATE Mode",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "mame2003-plus_samples",
                "Samples",
                "Audio",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "mame2003-plus_dcs_speedhack",
                "DCS Speedhack",
                "Performance",
                vec![opt("enabled"), opt("disabled")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// PlayStation: mednafen_psx_hw
// ---------------------------------------------------------------------------

fn mednafen_psx_hw_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mednafen_psx_hw",
        display_name: "PlayStation",
        system: "PSX",
        variables: vec![
            var(
                "beetle_psx_hw_internal_resolution",
                "Internal Resolution",
                "Display",
                vec![
                    opt_d("1x(native)", "1x Native"),
                    opt_d("2x", "2x"),
                    opt_d("4x", "4x"),
                    opt_d("8x", "8x"),
                    opt_d("16x", "16x"),
                ],
            ),
            var(
                "beetle_psx_hw_widescreen_hack",
                "Widescreen Hack",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_psx_hw_dithering_pattern",
                "Dithering Pattern",
                "Display",
                vec![
                    opt_d("1x(native)", "1x Native"),
                    opt_d("2x resolution", "2x"),
                    opt("disabled"),
                ],
            ),
            var(
                "beetle_psx_hw_texture_filtering",
                "Texture Filtering",
                "Display",
                vec![
                    opt("nearest"),
                    opt_d("SABR", "SABR"),
                    opt_d("bilinear", "Bilinear"),
                    opt_d("3-point", "3-Point"),
                    opt_d("JINC2", "JINC2"),
                    opt_d("xBR", "xBR"),
                ],
            ),
            var(
                "beetle_psx_hw_skip_bios",
                "Skip BIOS Intro",
                "System",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_psx_hw_analog_self_calibration",
                "Analog Self-Calibration",
                "Input",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "beetle_psx_hw_cpu_freq_scale",
                "CPU Frequency Scaling",
                "Performance",
                vec![
                    opt_d("100%(native)", "100% (Native)"),
                    opt("125%"),
                    opt("150%"),
                    opt("175%"),
                    opt("200%"),
                    opt("300%"),
                    opt("400%"),
                ],
            ),
            var(
                "beetle_psx_hw_frame_duplication_hack",
                "Frame Duplication Hack",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// PlayStation 2: pcsx2
// ---------------------------------------------------------------------------

fn pcsx2_profile() -> CoreProfile {
    CoreProfile {
        core_name: "pcsx2",
        display_name: "PlayStation 2",
        system: "PS2",
        variables: vec![
            var(
                "pcsx2_upscale_multiplier",
                "Internal Resolution",
                "Display",
                vec![
                    opt_d("1", "1x Native"),
                    opt_d("2", "2x"),
                    opt_d("3", "3x"),
                    opt_d("4", "4x"),
                    opt_d("6", "6x"),
                    opt_d("8", "8x"),
                ],
            ),
            var(
                "pcsx2_renderer",
                "Renderer",
                "Display",
                vec![opt("Auto"), opt("Vulkan"), opt("OpenGL"), opt("Software")],
            ),
            var(
                "pcsx2_bilinear_filtering",
                "Bilinear Filtering",
                "Display",
                vec![opt("disabled"), opt("basic"), opt("forced")],
            ),
            var(
                "pcsx2_widescreen_patch",
                "Widescreen Patch",
                "Display",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "pcsx2_speedhacks_toggles",
                "Speedhacks",
                "Performance",
                enabled_options(true),
            ),
            var("pcsx2_mtvu", "MTVU", "Performance", enabled_options(true)),
            var(
                "pcsx2_instant_vu1",
                "Instant VU1",
                "Performance",
                enabled_options(true),
            ),
            var(
                "pcsx2_ee_cycle_rate",
                "EE Cycle Rate",
                "Performance",
                vec![
                    opt_d("0", "100%"),
                    opt_d("1", "130%"),
                    opt_d("2", "180%"),
                    opt_d("3", "300%"),
                    opt_d("-1", "75%"),
                    opt_d("-2", "60%"),
                    opt_d("-3", "50%"),
                ],
            ),
            var(
                "pcsx2_ee_cycle_skip",
                "EE Cycle Skipping",
                "Performance",
                vec![
                    opt_d("0", "Off"),
                    opt_d("1", "Mild"),
                    opt_d("2", "Moderate"),
                    opt_d("3", "Maximum"),
                ],
            ),
            var(
                "pcsx2_audio_sync",
                "Audio Sync",
                "Audio",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "pcsx2_turbo_limiter",
                "Turbo Limiter",
                "Performance",
                vec![opt("disabled"), opt("enabled")],
            ),
        ],
    }
}

fn pcarmsx2_profile() -> CoreProfile {
    let mut profile = pcsx2_profile();
    profile.core_name = "pcarmsx2";
    profile.display_name = "pcarmsx2 ARM64 PoC";
    let mut variables = vec![
        var(
            "pcarmsx2_metal_host",
            "Metal Host View",
            "ARM64 PoC",
            enabled_options(true),
        ),
        var(
            "pcarmsx2_audio_backend",
            "Audio Backend",
            "ARM64 PoC",
            vec![opt("Cubeb"), opt("Null"), opt("SDL")],
        ),
        var(
            "pcarmsx2_enable_ee_rec",
            "EE Recompiler",
            "Recompilers",
            enabled_options(true),
        ),
        var(
            "pcarmsx2_enable_vu0_rec",
            "VU0 Recompiler",
            "Recompilers",
            enabled_options(true),
        ),
        var(
            "pcarmsx2_enable_vu1_rec",
            "VU1 Recompiler",
            "Recompilers",
            enabled_options(true),
        ),
        var(
            "pcarmsx2_enable_iop_rec",
            "IOP Recompiler",
            "Recompilers",
            enabled_options(true),
        ),
        var(
            "pcarmsx2_use_jita64",
            "jitA64 Stub Path",
            "Recompilers",
            enabled_options(false),
        ),
        var(
            "pcarmsx2_enable_xgkick_hack",
            "XGKICK Gamefix",
            "Gamefixes",
            enabled_options(false),
        ),
    ];
    variables.extend(profile.variables);
    profile.variables = variables;
    profile
}

// ---------------------------------------------------------------------------
// PlayStation 2: play
// ---------------------------------------------------------------------------

fn play_profile() -> CoreProfile {
    CoreProfile {
        core_name: "play",
        display_name: if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "Play! Native PS2"
        } else {
            "Play!"
        },
        system: "PS2",
        variables: vec![
            var(
                "play_res_multi",
                "Resolution Multiplier",
                "Display",
                vec![opt_d("1x", "1x Native"), opt("2x"), opt("4x"), opt("8x")],
            ),
            var(
                "play_presentation_mode",
                "Presentation Mode",
                "Display",
                vec![opt("Fit Screen"), opt("Fill Screen"), opt("Original Size")],
            ),
            var(
                "play_bilinear_filtering",
                "Force Bilinear Filtering",
                "Display",
                vec![opt("false"), opt("true")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Dreamcast: flycast
// ---------------------------------------------------------------------------

fn flycast_profile() -> CoreProfile {
    CoreProfile {
        core_name: "flycast",
        display_name: "Dreamcast",
        system: "DREAMCAST",
        variables: vec![
            var(
                "flycast_region",
                "Region",
                "System",
                vec![opt("Default"), opt("Japan"), opt("USA"), opt("Europe")],
            ),
            var(
                "flycast_language",
                "Language",
                "System",
                vec![
                    opt("Default"),
                    opt("Japanese"),
                    opt("English"),
                    opt("German"),
                    opt("French"),
                    opt("Spanish"),
                    opt("Italian"),
                ],
            ),
            var(
                "flycast_enable_dsp",
                "Enable DSP",
                "Audio",
                vec![opt_d("enabled", "Enabled (Recommended)"), opt("disabled")],
            ),
            var(
                "flycast_threaded_rendering",
                "Threaded Rendering",
                "Performance",
                vec![opt_d("enabled", "Enabled (Recommended)"), opt("disabled")],
            ),
            var(
                "flycast_boot_to_bios",
                "Boot to BIOS",
                "System",
                vec![opt_d("disabled", "Disabled (Recommended)"), opt("enabled")],
            ),
            var(
                "flycast_internal_resolution",
                "Internal Resolution",
                "Display",
                vec![
                    opt_d("640x480", "640x480 (Native)"),
                    opt("1280x960"),
                    opt("1920x1440"),
                    opt("2560x1920"),
                    opt("3840x2880"),
                ],
            ),
            var(
                "flycast_anisotropic_filtering",
                "Anisotropic Filtering",
                "Display",
                vec![opt("off"), opt("2"), opt("4"), opt("8"), opt("16")],
            ),
            var(
                "flycast_cable_type",
                "Cable Type",
                "Display",
                vec![
                    opt_d("VGA(RGB)", "VGA"),
                    opt_d("TV (Composite)", "Composite/AV"),
                    opt_d("TV (RGB)", "TV RGB (SCART)"),
                ],
            ),
            var(
                "flycast_broadcast",
                "Broadcast Region",
                "System",
                vec![opt("NTSC"), opt("PAL"), opt("PAL-M"), opt("PAL-N")],
            ),
            var(
                "flycast_force_wince",
                "Force Windows CE Mode",
                "System",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "flycast_alpha_sorting",
                "Alpha Sorting",
                "Display",
                vec![
                    opt_d("Per-Strip (fast, least accurate)", "Per-Strip (Fast)"),
                    opt_d("Per-Triangle (normal)", "Per-Triangle (Normal)"),
                    opt_d(
                        "Per-Pixel (accurate, but slowest)",
                        "Per-Pixel (Most Accurate)",
                    ),
                ],
            ),
            var(
                "flycast_delay_frame_swapping",
                "Delay Frame Swapping",
                "Display",
                vec![opt_d("disabled", "Disabled (Recommended)"), opt("enabled")],
            ),
            var(
                "flycast_pvr2_filtering",
                "PowerVR2 Post-Processing Filter",
                "Display",
                vec![opt_d("disabled", "Disabled"), opt("enabled")],
            ),
            var(
                "flycast_synchronous_rendering",
                "Synchronous Rendering",
                "Performance",
                vec![opt("enabled"), opt("disabled")],
            ),
            var(
                "flycast_skip_frame",
                "Auto Skip Frame",
                "Performance",
                vec![opt_d("disabled", "Disabled"), opt("enabled")],
            ),
            var(
                "flycast_frame_skipping",
                "Frame Skipping",
                "Performance",
                vec![
                    opt_d("disabled", "Disabled"),
                    opt("1"),
                    opt("2"),
                    opt("3"),
                    opt("4"),
                    opt("5"),
                    opt("6"),
                ],
            ),
            var(
                "flycast_gdrom_fast_loading",
                "GD-ROM Fast Loading",
                "Performance",
                vec![opt_d("On", "On (Faster, Less Accurate)"), opt("Off")],
            ),
            var(
                "flycast_audio_buffer_size",
                "Audio Buffer Size",
                "Audio",
                vec![opt("1024"), opt("2048"), opt("512")],
            ),
            var(
                "flycast_per_content_vmus",
                "Per-Game VMUs",
                "Memory",
                vec![opt("disabled"), opt("VMU A1"), opt("All VMUs")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Sega Saturn: mednafen_saturn
// ---------------------------------------------------------------------------

fn mednafen_saturn_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mednafen_saturn",
        display_name: "Saturn",
        system: "SATURN",
        variables: vec![
            var(
                "beetle_saturn_region",
                "System Region",
                "System",
                vec![
                    opt("Auto Detect"),
                    opt("Asia (NTSC)"),
                    opt("Asia (PAL)"),
                    opt("Latin America"),
                ],
            ),
            var(
                "beetle_saturn_autortc_lang",
                "BIOS Language",
                "System",
                vec![
                    opt("english"),
                    opt("japanese"),
                    opt("german"),
                    opt("french"),
                    opt("spanish"),
                    opt("italian"),
                ],
            ),
            var(
                "beetle_saturn_autortc",
                "RTC Automatic Set",
                "System",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_saturn_cdimagecache",
                "CD Image Cache",
                "System",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_saturn_cart",
                "Cartridge",
                "Cartridge / Memory Card",
                vec![
                    opt("auto"),
                    opt("backup"),
                    opt("extram1"),
                    opt("extram4"),
                    opt("kof95"),
                    opt("ultraman"),
                ],
            ),
            var(
                "beetle_saturn_horizontal_overscan",
                "Horizontal Overscan Mask",
                "Video",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_saturn_horizontal_blend",
                "Horizontal Blend",
                "Video",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_saturn_deinterlacer",
                "Deinterlace Method",
                "Video",
                vec![opt("disabled"), opt("weave")],
            ),
            var(
                "beetle_saturn_multitap_port1",
                "6Player Adaptor on Port 1",
                "Input",
                vec![opt("disabled"), opt("enabled")],
            ),
            var(
                "beetle_saturn_multitap_port2",
                "6Player Adaptor on Port 2",
                "Input",
                vec![opt("disabled"), opt("enabled")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// PCE-CD: mednafen_pce_fast
// ---------------------------------------------------------------------------

fn mednafen_pce_fast_profile() -> CoreProfile {
    CoreProfile {
        core_name: "mednafen_pce_fast",
        display_name: "PCE-CD",
        system: "PCECD",
        variables: vec![
            var(
                "pce_fast_cdimagecache",
                "CD Image Cache",
                "PC Engine CD",
                vec![
                    opt_d("disabled", "Disabled (Lower RAM Use)"),
                    opt_d("enabled", "Enabled (Faster Seeks)"),
                ],
            ),
            var(
                "pce_fast_cdbios",
                "CD BIOS",
                "PC Engine CD",
                vec![
                    opt_d("syscard3.pce", "System Card 3"),
                    opt_d("syscard2.pce", "System Card 2"),
                    opt_d("syscard1.pce", "System Card 1"),
                    opt_d("gexpress.pce", "Games Express"),
                ],
            ),
            var(
                "pce_fast_nospritelimit",
                "No Sprite Limit",
                "Emulation Hacks",
                vec![opt_d("disabled", "Disabled (Accurate)"), opt("enabled")],
            ),
            var(
                "pce_fast_ocmultiplier",
                "CPU Overclock Multiplier",
                "Emulation Hacks",
                vec![opt("1"), opt("2"), opt("3"), opt("4"), opt("6"), opt("8")],
            ),
            var(
                "pce_fast_palette",
                "Color Palette",
                "Video",
                vec![opt("RGB"), opt("Composite"), opt("S-Video")],
            ),
            var(
                "pce_fast_hoverscan",
                "Horizontal Overscan",
                "Video",
                vec![opt("352"), opt("340"), opt("336"), opt("320")],
            ),
            var(
                "pce_fast_initial_scanline",
                "Initial Scanline",
                "Video",
                vec![opt("3"), opt("0"), opt("8"), opt("16")],
            ),
            var(
                "pce_fast_last_scanline",
                "Last Scanline",
                "Video",
                vec![opt("242"), opt("239"), opt("232"), opt("224")],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// DOS: dosbox_pure
// ---------------------------------------------------------------------------

fn dosbox_pure_profile() -> CoreProfile {
    CoreProfile {
        core_name: "dosbox_pure",
        display_name: "DOSBox",
        system: "DOS",
        variables: vec![
            var(
                "dosbox_pure_machine",
                "Emulated Machine",
                "System",
                vec![
                    opt_d("svga", "SVGA (Default)"),
                    opt_d("svga_s3", "SVGA (S3 Trio)"),
                    opt_d("svga_et3000", "SVGA (ET3000)"),
                    opt_d("svga_et4000", "SVGA (ET4000)"),
                    opt_d("svga_paradise", "SVGA (Paradise)"),
                    opt_d("vgaonly", "VGA Only"),
                    opt_d("ega", "EGA"),
                    opt_d("cga", "CGA"),
                    opt_d("tandy", "Tandy"),
                    opt_d("hercules", "Hercules"),
                    opt_d("pcjr", "PCjr"),
                ],
            ),
            var(
                "dosbox_pure_memory_size",
                "Memory Size (MB)",
                "System",
                vec![
                    opt("16"),
                    opt("4"),
                    opt("8"),
                    opt("24"),
                    opt("32"),
                    opt("48"),
                    opt("64"),
                ],
            ),
            var(
                "dosbox_pure_cpu_type",
                "CPU Type",
                "Performance",
                vec![
                    opt_d("auto", "Auto (Recommended)"),
                    opt_d("386", "386"),
                    opt_d("386_slow", "386 (Slow)"),
                    opt_d("486_slow", "486 (Slow)"),
                    opt_d("pentium_slow", "Pentium (Slow)"),
                    opt_d("386_prefetch", "386 Prefetch"),
                ],
            ),
            var(
                "dosbox_pure_cpu_core",
                "CPU Core",
                "Performance",
                vec![
                    opt_d("auto", "Auto"),
                    opt_d("dynamic", "Dynamic (Fast)"),
                    opt_d("simple", "Simple"),
                    opt_d("normal", "Normal (Accurate)"),
                ],
            ),
            var(
                "dosbox_pure_cycles",
                "Emulated CPU Speed",
                "Performance",
                vec![
                    opt_d("auto", "Auto (Game Default)"),
                    opt_d("max", "Max (Uncapped)"),
                    opt("3000"),
                    opt("5000"),
                    opt("10000"),
                    opt("15000"),
                    opt("20000"),
                    opt("30000"),
                    opt("50000"),
                ],
            ),
            var(
                "dosbox_pure_sblaster_type",
                "Sound Blaster Type",
                "Audio",
                vec![
                    opt_d("sb16", "Sound Blaster 16"),
                    opt_d("sbpro2", "Sound Blaster Pro 2"),
                    opt_d("sbpro1", "Sound Blaster Pro 1"),
                    opt_d("sb2", "Sound Blaster 2"),
                    opt_d("sb1", "Sound Blaster 1"),
                    opt_d("gb", "GameBlaster"),
                    opt("none"),
                ],
            ),
            var(
                "dosbox_pure_aspect_correction",
                "Aspect Ratio Correction",
                "Display",
                vec![opt("false"), opt("true")],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_profiles_have_unique_core_names() {
        let profiles = core_profiles();
        let mut names: Vec<&str> = profiles.iter().map(|p| p.core_name).collect();
        let total = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate core_name in profiles");
    }

    #[test]
    fn all_profiles_have_at_least_one_variable() {
        for p in core_profiles() {
            assert!(
                !p.variables.is_empty(),
                "profile {} has no variables",
                p.core_name
            );
        }
    }

    #[test]
    fn all_variables_have_at_least_one_option() {
        for p in core_profiles() {
            for v in &p.variables {
                assert!(
                    !v.options.is_empty(),
                    "variable {} in profile {} has no options",
                    v.key,
                    p.core_name
                );
            }
        }
    }

    #[test]
    fn lookup_by_core_name_works() {
        assert!(core_profile_for("mupen64plus_next").is_some());
        assert!(core_profile_for("fceumm").is_some());
        assert!(core_profile_for("play").is_some());
        assert!(core_profile_for("pcarmsx2").is_some());
        assert!(core_profile_for("mednafen_saturn").is_some());
        assert!(core_profile_for("mednafen_pce_fast").is_some());
        assert!(core_profile_for("dolphin").is_some());
        assert!(core_profile_for("nonexistent").is_none());
    }

    #[test]
    fn pcarmsx2_profile_exposes_runtime_toggles() {
        let profile = core_profile_for("pcarmsx2").unwrap();
        for key in [
            "pcarmsx2_metal_host",
            "pcarmsx2_audio_backend",
            "pcarmsx2_enable_ee_rec",
            "pcarmsx2_enable_vu0_rec",
            "pcarmsx2_enable_vu1_rec",
            "pcarmsx2_enable_iop_rec",
            "pcarmsx2_use_jita64",
            "pcarmsx2_enable_xgkick_hack",
        ] {
            assert!(
                profile.variables.iter().any(|variable| variable.key == key),
                "missing pcarmsx2 runtime toggle {key}"
            );
        }
    }

    #[test]
    fn resolve_falls_back_to_default() {
        let settings = HashMap::new();
        let profile = core_profile_for("mupen64plus_next").unwrap();
        let aspect = profile
            .variables
            .iter()
            .find(|v| v.key == "mupen64plus-aspect")
            .unwrap();
        assert_eq!(
            resolve_core_variable(&settings, "mupen64plus_next", aspect),
            "4:3"
        );
    }

    #[test]
    fn resolve_uses_stored_value() {
        let mut inner = HashMap::new();
        inner.insert("mupen64plus-aspect".to_string(), "16:9".to_string());
        let mut settings = HashMap::new();
        settings.insert("mupen64plus_next".to_string(), inner);
        let profile = core_profile_for("mupen64plus_next").unwrap();
        let aspect = profile
            .variables
            .iter()
            .find(|v| v.key == "mupen64plus-aspect")
            .unwrap();
        assert_eq!(
            resolve_core_variable(&settings, "mupen64plus_next", aspect),
            "16:9"
        );
    }
}
