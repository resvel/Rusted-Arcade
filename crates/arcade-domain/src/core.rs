pub const SUPPORTED_SYSTEMS: &[&str] = &[
    "NES",
    "SNES",
    "GENESIS",
    "GB",
    "GBA",
    "N64",
    "ARCADE",
    "PSX",
    "PS2",
    "DREAMCAST",
    "GAMECUBE",
    "SATURN",
    "PCECD",
    "DOS",
];

pub const SUPPORTED_CORES: &[&str] = &[
    "fceumm",
    "snes9x",
    "genesis_plus_gx",
    "gambatte",
    "mgba",
    "mupen64plus_next",
    "fbneo",
    "mame2003",
    "mame2003_plus",
    "mednafen_psx_hw",
    "mednafen_saturn",
    "mednafen_pce_fast",
    "pcsx2",
    "pcarmsx2",
    "play",
    "flycast",
    "dolphin",
    "dosbox_pure",
];

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn default_ps2_core() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        if env_flag_enabled("ARCADE_PCSX2_METAL_POC") {
            return "pcarmsx2";
        }
        "play"
    } else {
        "pcsx2"
    }
}

fn default_core(system: &str) -> &'static str {
    match system {
        "NES" => "fceumm",
        "SNES" => "snes9x",
        "GENESIS" => "genesis_plus_gx",
        "GB" => "gambatte",
        "GBA" => "mgba",
        "N64" => "mupen64plus_next",
        "ARCADE" => "fbneo",
        "PSX" => "mednafen_psx_hw",
        "PS2" => default_ps2_core(),
        "DREAMCAST" => "flycast",
        "GAMECUBE" => "dolphin",
        "SATURN" => "mednafen_saturn",
        "PCECD" => "mednafen_pce_fast",
        "DOS" => "dosbox_pure",
        _ => "fceumm",
    }
}

fn allowlist(system: &str) -> &'static [&'static str] {
    match system {
        "NES" => &["fceumm"],
        "SNES" => &["snes9x"],
        "GENESIS" => &["genesis_plus_gx"],
        "GB" => &["gambatte"],
        "GBA" => &["mgba"],
        "N64" => &["mupen64plus_next"],
        "ARCADE" => &["fbneo", "mame2003", "mame2003_plus"],
        "PSX" => &["mednafen_psx_hw"],
        "PS2" => ps2_allowlist(),
        "DREAMCAST" => &["flycast"],
        "GAMECUBE" => &["dolphin"],
        "SATURN" => &["mednafen_saturn"],
        "PCECD" => &["mednafen_pce_fast"],
        "DOS" => &["dosbox_pure"],
        _ => &["fceumm"],
    }
}

fn ps2_allowlist() -> &'static [&'static str] {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        &["play", "pcarmsx2"]
    } else {
        &["pcsx2", "play"]
    }
}

pub fn supported_cores_for_system(system: &str) -> &'static [&'static str] {
    let normalized_system = normalize_system(system).unwrap_or_else(|| String::from("NES"));
    allowlist(&normalized_system)
}

pub fn normalize_system(value: &str) -> Option<String> {
    let system = value.trim().to_uppercase();
    if SUPPORTED_SYSTEMS.contains(&system.as_str()) {
        Some(system)
    } else {
        None
    }
}

pub fn normalize_core(value: &str) -> Option<String> {
    let core = value.trim().to_lowercase();
    if SUPPORTED_CORES.contains(&core.as_str()) {
        Some(core)
    } else {
        None
    }
}

pub fn resolve_core(system: &str, core_override: Option<&str>) -> String {
    let normalized_system = normalize_system(system).unwrap_or_else(|| String::from("NES"));

    if let Some(raw_override) = core_override {
        if let Some(core) = normalize_core(raw_override) {
            if allowlist(&normalized_system).contains(&core.as_str()) {
                return core;
            }
        }
    }

    String::from(default_core(&normalized_system))
}

const FBNEO_PREFERRED_TITLES: &[&str] = &[
    "garou", "galaxyfg", "galaga", "toutrun", "fatfury1", "fatfury2", "fatfury3", "fatfursp",
    "sf2ce", "sf2hf", "kof94", "kof95", "kof96", "kof98", "kof99", "kof2000", "kof2001", "kof2003",
    "lastblad", "lastbld2", "samsho", "samsho2", "samsho3", "samsho4", "samsho5", "mslug",
    "mslug2", "mslug3", "mslug4", "mslug5", "mslugx",
];

const MAME2003_REQUIRED_TITLES: &[&str] = &["galaga88"];

pub fn resolve_effective_core_override(
    system: &str,
    configured_override: Option<&str>,
    title: Option<&str>,
) -> Option<String> {
    let configured = configured_override.and_then(normalize_core);

    if !system.eq_ignore_ascii_case("ARCADE") {
        return configured;
    }

    resolve_arcade_hard_core_override(title)
        .or_else(|| resolve_arcade_core_override(title))
        .or(configured)
}

pub fn resolve_arcade_core_override(title: Option<&str>) -> Option<String> {
    let title = title.unwrap_or("").trim().to_lowercase();
    if title.is_empty() {
        return None;
    }

    if FBNEO_PREFERRED_TITLES.contains(&title.as_str()) {
        return Some(String::from("fbneo"));
    }

    None
}

pub fn resolve_arcade_hard_core_override(title: Option<&str>) -> Option<String> {
    let title = title.unwrap_or("").trim().to_lowercase();
    if title.is_empty() {
        return None;
    }

    if MAME2003_REQUIRED_TITLES.contains(&title.as_str()) {
        return Some(String::from("mame2003"));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_flag_removed<T>(key: &str, f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = std::env::var_os(key);
        std::env::remove_var(key);
        let result = f();
        if let Some(value) = previous {
            std::env::set_var(key, value);
        }
        result
    }

    fn with_env_flag<T>(key: &str, value: &str, f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        let result = f();
        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
        result
    }

    #[test]
    fn core_mapping_defaults_work() {
        assert_eq!(resolve_core("NES", None), "fceumm");
        assert_eq!(resolve_core("SNES", None), "snes9x");
        assert_eq!(resolve_core("ARCADE", None), "fbneo");
        assert_eq!(resolve_core("SATURN", None), "mednafen_saturn");
        assert_eq!(resolve_core("PCECD", None), "mednafen_pce_fast");
        assert_eq!(resolve_core("GAMECUBE", None), "dolphin");
        assert_eq!(supported_cores_for_system("SATURN"), &["mednafen_saturn"]);
        assert_eq!(supported_cores_for_system("GAMECUBE"), &["dolphin"]);
    }

    #[test]
    fn ps2_default_core_respects_platform() {
        with_env_flag_removed("ARCADE_PCSX2_METAL_POC", || {
            let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                "play"
            } else {
                "pcsx2"
            };
            assert_eq!(resolve_core("PS2", None), expected);
        });
    }

    #[test]
    fn ps2_metal_poc_env_selects_pcarmsx2_on_native_macos_arm64() {
        with_env_flag("ARCADE_PCSX2_METAL_POC", "1", || {
            let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                "pcarmsx2"
            } else {
                "pcsx2"
            };
            assert_eq!(resolve_core("PS2", None), expected);
        });
    }

    #[test]
    fn ps2_allowlist_respects_native_macos_arm64_play_path() {
        let expected: &[&str] = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            &["play", "pcarmsx2"]
        } else {
            &["pcsx2", "play"]
        };
        assert_eq!(supported_cores_for_system("PS2"), expected);
    }

    #[test]
    fn ps2_pcsx2_override_is_ignored_on_native_macos_arm64() {
        let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "play"
        } else {
            "pcsx2"
        };
        assert_eq!(resolve_core("PS2", Some("pcsx2")), expected);
    }

    #[test]
    fn ps2_pcarmsx2_override_is_available_only_on_native_macos_arm64() {
        let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "pcarmsx2"
        } else {
            "pcsx2"
        };
        assert_eq!(resolve_core("PS2", Some("pcarmsx2")), expected);
    }

    #[test]
    fn invalid_core_override_falls_back() {
        assert_eq!(resolve_core("NES", Some("mupen64plus_next")), "fceumm");
    }

    #[test]
    fn arcade_overrides_apply() {
        assert_eq!(
            resolve_arcade_core_override(Some("mslug")),
            Some(String::from("fbneo"))
        );
        assert_eq!(
            resolve_arcade_hard_core_override(Some("galaga88")),
            Some(String::from("mame2003"))
        );
    }

    #[test]
    fn effective_arcade_override_prefers_fbneo_soft_title_preferences() {
        assert_eq!(
            resolve_effective_core_override("ARCADE", Some("mame2003"), Some("galaga")),
            Some(String::from("fbneo"))
        );
        assert_eq!(
            resolve_effective_core_override("ARCADE", Some("fbneo"), Some("galaga88")),
            Some(String::from("mame2003"))
        );
        assert_eq!(
            resolve_effective_core_override("ARCADE", Some("mame2003"), Some("unknown")),
            Some(String::from("mame2003"))
        );
    }
}
