pub const SUPPORTED_SYSTEMS: &[&str] = &["NES", "SNES", "GENESIS", "GB", "GBA", "N64", "ARCADE"];

pub const SUPPORTED_CORES: &[&str] = &[
    "fceumm",
    "snes9x",
    "genesis_plus_gx",
    "gambatte",
    "mgba",
    "parallel_n64",
    "mupen64plus_next",
    "fbneo",
    "mame2003",
    "mame2003_plus",
];

#[cfg(target_os = "macos")]
const DEFAULT_N64_CORE: &str = "mupen64plus_next";
#[cfg(not(target_os = "macos"))]
const DEFAULT_N64_CORE: &str = "parallel_n64";

#[cfg(target_os = "macos")]
const ALLOWLIST_N64: &[&str] = &["mupen64plus_next"];
#[cfg(not(target_os = "macos"))]
const ALLOWLIST_N64: &[&str] = &["parallel_n64", "mupen64plus_next"];

fn default_core(system: &str) -> &'static str {
    match system {
        "NES" => "fceumm",
        "SNES" => "snes9x",
        "GENESIS" => "genesis_plus_gx",
        "GB" => "gambatte",
        "GBA" => "mgba",
        "N64" => DEFAULT_N64_CORE,
        "ARCADE" => "fbneo",
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
        "N64" => ALLOWLIST_N64,
        "ARCADE" => &["fbneo", "mame2003", "mame2003_plus"],
        _ => &["fceumm"],
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

    #[test]
    fn core_mapping_defaults_work() {
        assert_eq!(resolve_core("NES", None), "fceumm");
        assert_eq!(resolve_core("SNES", None), "snes9x");
        assert_eq!(resolve_core("ARCADE", None), "fbneo");
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
