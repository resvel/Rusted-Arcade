use std::collections::{BTreeMap, BTreeSet};

use crate::CoreCatalogCompatibility;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibretroInfoIndex {
    pub cores: BTreeMap<String, LibretroCoreInfo>,
}

impl LibretroInfoIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, info: LibretroCoreInfo) {
        self.cores.insert(info.core_name.clone(), info);
    }

    pub fn parse_core(core_name: impl AsRef<str>, contents: &str) -> LibretroCoreInfo {
        parse_libretro_info(core_name, contents)
    }

    pub fn from_infos<'a>(infos: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut index = Self::new();
        for (core_name, contents) in infos {
            index.insert(parse_libretro_info(core_name, contents));
        }
        index
    }

    pub fn get(&self, core_name: &str) -> Option<&LibretroCoreInfo> {
        self.cores.get(&normalize_core_name(core_name))
    }

    pub fn is_empty(&self) -> bool {
        self.cores.is_empty()
    }

    pub fn classify(&self, core_name: &str) -> InferredCoreClassification {
        self.get(core_name)
            .map(classify_libretro_core_info)
            .unwrap_or_else(|| InferredCoreClassification::unclassified(core_name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibretroCoreInfo {
    pub core_name: String,
    pub display_name: Option<String>,
    pub system_name: Option<String>,
    pub system_id: Vec<String>,
    pub supported_extensions: Vec<String>,
    pub database: Vec<String>,
    pub required_hw_api: Vec<String>,
    pub permissions: Vec<String>,
    pub supports_no_game: Option<bool>,
    pub experimental: Option<bool>,
    pub firmware: Vec<LibretroFirmwareInfo>,
    pub raw: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibretroFirmwareInfo {
    pub index: usize,
    pub description: Option<String>,
    pub path: Option<String>,
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataClassificationConfidence {
    Strong,
    Conservative,
    Ambiguous,
    Guarded,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredCoreClassification {
    pub core_name: String,
    pub systems: Vec<String>,
    pub compatibility: CoreCatalogCompatibility,
    pub confidence: MetadataClassificationConfidence,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

impl InferredCoreClassification {
    pub fn unclassified(core_name: &str) -> Self {
        Self {
            core_name: normalize_core_name(core_name),
            systems: vec![String::from("ALL")],
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Unknown,
            notes: Vec::new(),
            warnings: vec![String::from(
                "No libretro .info metadata was available; keep this core in the Advanced browser.",
            )],
        }
    }

    pub fn is_reliably_system_scoped(&self) -> bool {
        self.confidence == MetadataClassificationConfidence::Strong
            && self.compatibility != CoreCatalogCompatibility::Advanced
            && self.systems.len() == 1
            && self.systems[0] != "ALL"
    }
}

pub fn parse_libretro_info(core_name: impl AsRef<str>, contents: &str) -> LibretroCoreInfo {
    let mut raw = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        if key.is_empty() {
            continue;
        }
        raw.insert(key, parse_value(value));
    }

    LibretroCoreInfo {
        core_name: normalize_core_name(core_name.as_ref()),
        display_name: raw
            .get("display_name")
            .or_else(|| raw.get("displayname"))
            .or_else(|| raw.get("corename"))
            .cloned(),
        system_name: raw.get("systemname").cloned(),
        system_id: split_pipe_field(raw.get("systemid")),
        supported_extensions: split_pipe_field(raw.get("supported_extensions")),
        database: split_pipe_field(raw.get("database")),
        required_hw_api: split_pipe_field(raw.get("required_hw_api")),
        permissions: split_pipe_field(raw.get("permissions")),
        supports_no_game: raw
            .get("supports_no_game")
            .and_then(|value| parse_bool(value)),
        experimental: raw.get("experimental").and_then(|value| parse_bool(value)),
        firmware: parse_firmware(&raw),
        raw,
    }
}

pub fn classify_libretro_core_info(info: &LibretroCoreInfo) -> InferredCoreClassification {
    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut systems = BTreeSet::new();
    let mut saw_strong_metadata = false;

    for value in info.system_id.iter().chain(info.database.iter()) {
        let mapped = systems_from_metadata_token(value);
        if !mapped.is_empty() {
            saw_strong_metadata = true;
            for system in mapped {
                systems.insert(system.to_string());
            }
        }
    }

    if systems.is_empty() {
        notes.push(String::from(
            "Metadata did not contain a recognized supported system id or database tag.",
        ));
        return InferredCoreClassification {
            core_name: info.core_name.clone(),
            systems: vec![String::from("ALL")],
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Unknown,
            notes,
            warnings: vec![String::from(
                "Unclassified metadata; keep this core in the Advanced browser.",
            )],
        };
    }

    let mut systems = systems.into_iter().collect::<Vec<_>>();
    let is_ps2 =
        systems.iter().any(|system| system == "PS2") || looks_like_ps2_core(&info.core_name);
    let is_experimental =
        info.experimental == Some(true) || looks_experimental(&info.core_name, &info.raw);
    let requires_hw_render = info.required_hw_api.iter().any(|api| {
        !matches!(
            api.trim().to_ascii_lowercase().as_str(),
            "" | "none" | "false"
        )
    });

    if has_known_multisystem_ambiguity(info) {
        warnings.push(String::from(
            "Known multi-system libretro core; keep it global until a curated Rusted Arcade entry scopes it safely.",
        ));
        return InferredCoreClassification {
            core_name: info.core_name.clone(),
            systems: vec![String::from("ALL")],
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Ambiguous,
            notes,
            warnings,
        };
    }

    if systems.len() > 1 {
        warnings.push(format!(
            "Metadata maps this core to multiple systems ({}); keep it global until curated.",
            systems.join(", ")
        ));
        systems = vec![String::from("ALL")];
        return InferredCoreClassification {
            core_name: info.core_name.clone(),
            systems,
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Ambiguous,
            notes,
            warnings,
        };
    }

    if is_ps2 {
        warnings.push(String::from(
            "PS2 buildbot cores are protected in Rusted Arcade and must not replace platform-specific PS2 lanes automatically.",
        ));
        return InferredCoreClassification {
            core_name: info.core_name.clone(),
            systems,
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Guarded,
            notes,
            warnings,
        };
    }

    if is_experimental || requires_hw_render {
        if is_experimental {
            warnings.push(String::from(
                "Metadata or core identity marks this core as experimental.",
            ));
        }
        if requires_hw_render {
            warnings.push(format!(
                "Requires libretro hardware rendering API(s): {}.",
                info.required_hw_api.join(", ")
            ));
        }
        return InferredCoreClassification {
            core_name: info.core_name.clone(),
            systems,
            compatibility: CoreCatalogCompatibility::Advanced,
            confidence: MetadataClassificationConfidence::Conservative,
            notes,
            warnings,
        };
    }

    notes.push(String::from(
        "Classified from libretro .info systemid/database metadata; curated catalog entries remain authoritative.",
    ));
    InferredCoreClassification {
        core_name: info.core_name.clone(),
        systems,
        compatibility: CoreCatalogCompatibility::Compatible,
        confidence: if saw_strong_metadata {
            MetadataClassificationConfidence::Strong
        } else {
            MetadataClassificationConfidence::Conservative
        },
        notes,
        warnings,
    }
}

fn parse_value(raw: &str) -> String {
    let mut value = raw.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value = &value[1..value.len() - 1];
        return value.trim().to_string();
    }

    let comment_start = value
        .char_indices()
        .find(|(_, ch)| *ch == '#' || *ch == ';')
        .map(|(idx, _)| idx);
    if let Some(idx) = comment_start {
        value = &value[..idx];
    }
    value.trim().trim_matches('"').trim().to_string()
}

fn split_pipe_field(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split('|')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn parse_firmware(raw: &BTreeMap<String, String>) -> Vec<LibretroFirmwareInfo> {
    let mut indexes = BTreeSet::new();
    for key in raw.keys() {
        if let Some(rest) = key.strip_prefix("firmware") {
            if let Some((index, field)) = rest.split_once('_') {
                if matches!(field, "desc" | "path" | "opt") {
                    if let Ok(index) = index.parse::<usize>() {
                        indexes.insert(index);
                    }
                }
            }
        }
    }

    indexes
        .into_iter()
        .map(|index| LibretroFirmwareInfo {
            index,
            description: raw.get(&format!("firmware{index}_desc")).cloned(),
            path: raw.get(&format!("firmware{index}_path")).cloned(),
            optional: raw
                .get(&format!("firmware{index}_opt"))
                .and_then(|value| parse_bool(value)),
        })
        .collect()
}

fn normalize_core_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches("_libretro.dylib.zip")
        .trim_end_matches("_libretro.dylib")
        .trim_end_matches("_libretro.info")
        .trim_end_matches(".info")
        .to_ascii_lowercase()
        .replace(' ', "_")
}

fn systems_from_metadata_token(token: &str) -> Vec<&'static str> {
    let value = token.trim().to_ascii_lowercase();
    let compact = value.replace(['_', '/', ':'], " ");
    let mut systems = BTreeSet::new();

    if compact == "nes" || compact == "famicom" {
        systems.insert("NES");
    }
    if compact == "super nes" || compact == "super famicom" || compact == "super nintendo" {
        systems.insert("SNES");
    }
    if compact == "game boy advance" || compact == "gba" {
        systems.insert("GBA");
    }
    if compact == "game boy" || compact == "gameboy" || compact == "gb" || compact == "gbc" {
        systems.insert("GB");
    }
    if compact == "nintendo 64" || compact == "n64" {
        systems.insert("N64");
    }
    if compact == "playstation" || compact == "psx" {
        systems.insert("PSX");
    }

    if compact.contains("playstation 2") || compact == "sony - ps2" || compact == "ps2" {
        systems.insert("PS2");
    } else if compact.contains("playstation") && !compact.contains("portable") {
        systems.insert("PSX");
    }

    if compact.contains("super nintendo") || compact.contains("snes") {
        systems.insert("SNES");
    }
    if compact.contains("nintendo entertainment system") && !compact.contains("super") {
        systems.insert("NES");
    }
    if compact.contains("game boy advance") || compact.contains("gba") {
        systems.insert("GBA");
    }
    if (compact.contains("game boy") || compact.contains("gameboy") || compact.contains("gbc"))
        && !compact.contains("advance")
    {
        systems.insert("GB");
    }
    if compact.contains("mega drive") || compact.contains("genesis") {
        systems.insert("GENESIS");
    }
    if compact.contains("gamecube") {
        systems.insert("GAMECUBE");
    }
    if compact.contains("dreamcast") {
        systems.insert("DREAMCAST");
    }
    if compact.contains("saturn") {
        systems.insert("SATURN");
    }
    if compact.contains("pc engine") || compact.contains("turbografx") {
        systems.insert("PCECD");
    }
    if compact == "dos" || compact.contains("dosbox") {
        systems.insert("DOS");
    }

    systems.into_iter().collect()
}

fn has_known_multisystem_ambiguity(info: &LibretroCoreInfo) -> bool {
    matches!(info.core_name.as_str(), "mgba" | "genesis_plus_gx")
        && info.system_id.len() + info.database.len() > 1
}

fn looks_like_ps2_core(core_name: &str) -> bool {
    matches!(core_name, "pcsx2" | "lrps2" | "play" | "play!")
}

fn looks_experimental(core_name: &str, raw: &BTreeMap<String, String>) -> bool {
    core_name.eq_ignore_ascii_case("play")
        || core_name.eq_ignore_ascii_case("play!")
        || raw
            .values()
            .any(|value| value.to_ascii_lowercase().contains("experimental"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(core_name: &str, body: &str) -> LibretroCoreInfo {
        parse_libretro_info(core_name, body)
    }

    #[test]
    fn parser_handles_quotes_comments_pipe_fields_bools_and_raw_keys() {
        let parsed = info(
            "snes9x_libretro.info",
            r#"
                # comment
                display_name = "Snes9x"
                systemid = "Nintendo - Super Nintendo Entertainment System"
                supported_extensions = "smc|sfc|fig"
                database = "Nintendo - Super Nintendo Entertainment System"
                required_hw_api = ""
                permissions = "filesystem|network"
                supports_no_game = "false"
                experimental = "no"
                custom_key = "custom value"
            "#,
        );

        assert_eq!(parsed.core_name, "snes9x");
        assert_eq!(parsed.display_name.as_deref(), Some("Snes9x"));
        assert_eq!(parsed.supported_extensions, vec!["smc", "sfc", "fig"]);
        assert_eq!(parsed.permissions, vec!["filesystem", "network"]);
        assert_eq!(parsed.supports_no_game, Some(false));
        assert_eq!(parsed.experimental, Some(false));
        assert_eq!(
            parsed.raw.get("custom_key").map(String::as_str),
            Some("custom value")
        );
    }

    #[test]
    fn parser_groups_required_and_optional_firmware() {
        let parsed = info(
            "mednafen_psx",
            r#"
                firmware0_desc = "PlayStation BIOS (Japan)"
                firmware0_path = "scph5500.bin"
                firmware0_opt = "false"
                firmware1_desc = "PlayStation BIOS (USA)"
                firmware1_path = "scph5501.bin"
                firmware1_opt = "true"
            "#,
        );

        assert_eq!(parsed.firmware.len(), 2);
        assert_eq!(parsed.firmware[0].index, 0);
        assert_eq!(parsed.firmware[0].path.as_deref(), Some("scph5500.bin"));
        assert_eq!(parsed.firmware[0].optional, Some(false));
        assert_eq!(parsed.firmware[1].optional, Some(true));
    }

    #[test]
    fn classifier_maps_simple_supported_systems_from_strong_metadata() {
        for (core, systemid, expected) in [
            (
                "snes9x",
                "Nintendo - Super Nintendo Entertainment System",
                "SNES",
            ),
            ("fceumm", "Nintendo - Nintendo Entertainment System", "NES"),
            ("gambatte", "Nintendo - Game Boy", "GB"),
            ("vba_next", "Nintendo - Game Boy Advance", "GBA"),
            ("mednafen_psx", "Sony - PlayStation", "PSX"),
        ] {
            let parsed = info(core, &format!("systemid = \"{systemid}\"\n"));
            let classified = classify_libretro_core_info(&parsed);
            assert_eq!(classified.systems, vec![expected.to_string()], "{core}");
            assert_eq!(
                classified.compatibility,
                CoreCatalogCompatibility::Compatible
            );
            assert_eq!(
                classified.confidence,
                MetadataClassificationConfidence::Strong
            );
        }
    }

    #[test]
    fn ambiguous_multisystem_cores_stay_global_advanced() {
        for (core, database) in [
            (
                "mgba",
                "Nintendo - Game Boy|Nintendo - Game Boy Color|Nintendo - Game Boy Advance",
            ),
            (
                "genesis_plus_gx",
                "Sega - Mega Drive - Genesis|Sega - Master System - Mark III|Sega - Game Gear",
            ),
        ] {
            let parsed = info(core, &format!("database = \"{database}\"\n"));
            let classified = classify_libretro_core_info(&parsed);
            assert_eq!(classified.systems, vec!["ALL".to_string()], "{core}");
            assert_eq!(classified.compatibility, CoreCatalogCompatibility::Advanced);
            assert_eq!(
                classified.confidence,
                MetadataClassificationConfidence::Ambiguous
            );
        }
    }

    #[test]
    fn experimental_and_hardware_rendering_are_advanced_with_warnings() {
        let experimental = classify_libretro_core_info(&info(
            "play",
            "systemid = \"Sony - PlayStation 2\"\nexperimental = \"true\"\n",
        ));
        assert_eq!(experimental.systems, vec!["PS2".to_string()]);
        assert_eq!(
            experimental.compatibility,
            CoreCatalogCompatibility::Advanced
        );
        assert_eq!(
            experimental.confidence,
            MetadataClassificationConfidence::Guarded
        );
        assert!(experimental
            .warnings
            .iter()
            .any(|warning| warning.contains("PS2")));

        let hw = classify_libretro_core_info(&info(
            "dolphin",
            "systemid = \"Nintendo - GameCube\"\nrequired_hw_api = \"vulkan|opengl\"\n",
        ));
        assert_eq!(hw.systems, vec!["GAMECUBE".to_string()]);
        assert_eq!(hw.compatibility, CoreCatalogCompatibility::Advanced);
        assert!(hw
            .warnings
            .iter()
            .any(|warning| warning.contains("hardware rendering")));
    }

    #[test]
    fn ps2_metadata_is_guarded_and_not_promoted() {
        let classified =
            classify_libretro_core_info(&info("pcsx2", "systemid = \"Sony - PlayStation 2\"\n"));
        assert_eq!(classified.systems, vec!["PS2".to_string()]);
        assert_eq!(classified.compatibility, CoreCatalogCompatibility::Advanced);
        assert_eq!(
            classified.confidence,
            MetadataClassificationConfidence::Guarded
        );
    }
}
