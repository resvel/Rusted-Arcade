use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const PROJECT_DIR_QUALIFIER: &str = "com";
const PROJECT_DIR_ORGANIZATION: &str = "jules";
const PROJECT_NAME_NATIVE: &str = "personal-arcade-native";
const PROJECT_NAME_LEGACY: &str = "personal-web-arcade";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmulationConfig {
    /// Legacy N64 settings kept for backward-compatible deserialization.
    /// Still serialized so `preferred_core` persists; variable fields are
    /// superseded by `core_settings` once migration has run.
    #[serde(default)]
    pub n64: N64EmulationConfig,

    /// Per-core settings: keys are core names (e.g. `"mupen64plus_next"`),
    /// values are maps of variable_key → chosen value.
    #[serde(default)]
    pub core_settings: HashMap<String, HashMap<String, String>>,
}

impl EmulationConfig {
    /// Look up a stored core variable value.
    pub fn get_core_variable(&self, core_name: &str, key: &str) -> Option<&str> {
        self.core_settings
            .get(core_name)
            .and_then(|vars| vars.get(key))
            .map(|s| s.as_str())
    }

    /// Ensure platform-specific default values are present. Uses `or_insert`
    /// so explicit user choices are never overwritten.
    pub fn apply_platform_defaults(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let vars = self
                .core_settings
                .entry("mupen64plus_next".into())
                .or_default();
            vars.entry("mupen64plus-Framerate".into())
                .or_insert_with(|| "Fullspeed".into());
            vars.entry("mupen64plus-FrameDuping".into())
                .or_insert_with(|| "True".into());
            vars.entry("mupen64plus-virefresh".into())
                .or_insert_with(|| "1500".into());
        }
    }

    /// One-time migration: copy legacy `N64EmulationConfig` fields into
    /// `core_settings["mupen64plus_next"]`. Only runs if the key is absent.
    pub fn migrate_legacy_n64(&mut self) {
        if self.core_settings.contains_key("mupen64plus_next") {
            return;
        }
        let n = &self.n64;
        let mut vars = HashMap::new();
        vars.insert("mupen64plus-aspect".into(), n.aspect_ratio.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-upscaling".into(), n.parallel_rdp_upscaling.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-synchronous".into(), n.parallel_rdp_synchronous.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-super-sampled-read-back".into(), n.parallel_rdp_super_sampled_read_back.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-vi-aa".into(), n.parallel_rdp_vi_aa.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-vi-bilinear".into(), n.parallel_rdp_vi_bilinear.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-dither-filter".into(), n.parallel_rdp_dither_filter.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-divot-filter".into(), n.parallel_rdp_divot_filter.as_core_value().into());
        vars.insert("mupen64plus-parallel-rdp-gamma-dither".into(), n.parallel_rdp_gamma_dither.as_core_value().into());
        vars.insert("mupen64plus-EnableFBEmulation".into(), n.fb_emulation.as_core_value().into());
        vars.insert("mupen64plus-EnableCopyColorToRDRAM".into(), n.copy_color_to_rdram.as_core_value().into());
        vars.insert("mupen64plus-FrameDuping".into(), n.frame_duplication.as_core_value().into());
        vars.insert("mupen64plus-Framerate".into(), n.framerate.as_core_value().into());
        vars.insert("mupen64plus-virefresh".into(), n.vi_refresh.as_core_value().into());
        vars.insert("mupen64plus-CountPerOp".into(), n.count_per_op.as_core_value().into());
        vars.insert("mupen64plus-CountPerOpDenomPot".into(), n.count_per_op_denom_pot.as_core_value().into());
        self.core_settings.insert("mupen64plus_next".into(), vars);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct N64EmulationConfig {
    #[serde(default)]
    pub preferred_core: N64PreferredCore,
    #[serde(default)]
    pub parallel_rdp_upscaling: N64ParallelRdpUpscaling,
    #[serde(default)]
    pub parallel_profile: N64ParallelProfile,
    #[serde(default)]
    pub parallel_rdp_synchronous: N64ParallelRdpToggle,
    #[serde(default)]
    pub parallel_rdp_super_sampled_read_back: N64ParallelRdpToggle,
    #[serde(default)]
    pub parallel_rdp_vi_aa: N64ParallelRdpFilter,
    #[serde(default)]
    pub parallel_rdp_vi_bilinear: N64ParallelRdpFilter,
    #[serde(default)]
    pub parallel_rdp_dither_filter: N64ParallelRdpFilter,
    #[serde(default)]
    pub parallel_rdp_divot_filter: N64ParallelRdpFilter,
    #[serde(default)]
    pub parallel_rdp_gamma_dither: N64ParallelRdpFilter,
    #[serde(default)]
    pub count_per_op: N64CountPerOp,
    #[serde(default)]
    pub fb_emulation: N64FbEmulation,
    #[serde(default)]
    pub copy_color_to_rdram: N64CopyColorToRdram,
    #[serde(default)]
    pub frame_duplication: N64FrameDuplication,
    #[serde(default)]
    pub framerate: N64Framerate,
    #[serde(default)]
    pub vi_refresh: N64ViRefresh,
    #[serde(default)]
    pub count_per_op_denom_pot: N64CountPerOpDenomPot,
    #[serde(default)]
    pub aspect_ratio: N64AspectRatio,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64PreferredCore {
    #[default]
    Mupen64plusNext,
    // Keep the legacy value deserializable so old configs continue to load, but
    // route it to the single supported N64 core at runtime.
    ParallelN64,
}

impl N64PreferredCore {
    pub fn as_core_name(self) -> &'static str {
        match self {
            Self::Mupen64plusNext => "mupen64plus_next",
            Self::ParallelN64 => {
                #[cfg(target_os = "macos")]
                {
                    // Under Rosetta 2 the x86_64 parallel_n64 core can use
                    // dynarec. On native arm64 we fall back to mupen64plus_next
                    // since parallel_n64 cannot JIT without Rosetta translation.
                    if crate::platform::is_running_under_rosetta() {
                        "parallel_n64"
                    } else {
                        "mupen64plus_next"
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    "parallel_n64"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64ParallelRdpUpscaling {
    #[default]
    #[serde(rename = "1x")]
    X1,
    #[serde(rename = "2x")]
    X2,
    #[serde(rename = "4x")]
    X4,
    #[serde(rename = "8x")]
    X8,
}

impl N64ParallelRdpUpscaling {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64ParallelProfile {
    #[default]
    Balanced,
    Performance,
}

impl N64ParallelProfile {
    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64ParallelRdpToggle {
    #[default]
    #[serde(alias = "false")]
    False,
    #[serde(alias = "true")]
    True,
}

impl N64ParallelRdpToggle {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::False => "false",
            Self::True => "true",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64ParallelRdpFilter {
    #[default]
    Disabled,
    Enabled,
}

impl N64ParallelRdpFilter {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Enabled => "enabled",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64CountPerOp {
    #[default]
    #[serde(alias = "0")]
    Auto,
    #[serde(alias = "1")]
    One,
    #[serde(alias = "2")]
    Two,
    #[serde(alias = "3")]
    Three,
}

impl N64CountPerOp {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Auto => "0",
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64FbEmulation {
    #[default]
    #[serde(alias = "True")]
    True,
    #[serde(alias = "False")]
    False,
}

impl N64FbEmulation {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::True => "True",
            Self::False => "False",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum N64CopyColorToRdram {
    #[default]
    Async,
    Off,
    Sync,
}

impl N64CopyColorToRdram {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Async => "Async",
            Self::Sync => "Software",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64FrameDuplication {
    #[default]
    #[serde(rename = "False", alias = "false")]
    False,
    #[serde(rename = "True", alias = "true")]
    True,
}

impl N64FrameDuplication {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::False => "False",
            Self::True => "True",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64Framerate {
    #[default]
    #[serde(rename = "Original")]
    Original,
    #[serde(rename = "Fullspeed")]
    Fullspeed,
}

impl N64Framerate {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Fullspeed => "Fullspeed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64ViRefresh {
    #[default]
    #[serde(rename = "Auto")]
    Auto,
    #[serde(rename = "1500")]
    V1500,
    #[serde(rename = "2200")]
    V2200,
}

impl N64ViRefresh {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::V1500 => "1500",
            Self::V2200 => "2200",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64CountPerOpDenomPot {
    #[default]
    #[serde(alias = "0")]
    Zero,
    #[serde(alias = "1")]
    One,
    #[serde(alias = "2")]
    Two,
    #[serde(alias = "3")]
    Three,
    #[serde(alias = "4")]
    Four,
    #[serde(alias = "5")]
    Five,
    #[serde(alias = "6")]
    Six,
    #[serde(alias = "7")]
    Seven,
    #[serde(alias = "8")]
    Eight,
    #[serde(alias = "9")]
    Nine,
    #[serde(alias = "10")]
    Ten,
    #[serde(alias = "11")]
    Eleven,
}

impl N64CountPerOpDenomPot {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Eleven => "11",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum N64AspectRatio {
    #[default]
    #[serde(rename = "4:3")]
    Ratio43,
    #[serde(rename = "16:9")]
    Ratio169,
    #[serde(rename = "16:9 adjusted")]
    Ratio169Adjusted,
}

impl N64AspectRatio {
    pub fn as_core_value(self) -> &'static str {
        match self {
            Self::Ratio43 => "4:3",
            Self::Ratio169 => "16:9",
            Self::Ratio169Adjusted => "16:9 adjusted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathsConfig {
    pub rom_root: PathBuf,
    pub db_path: PathBuf,
    pub save_state_root: PathBuf,
    pub core_root: PathBuf,
    pub bios_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub paths: PathsConfig,
    #[serde(default)]
    pub emulation: EmulationConfig,
    #[serde(default)]
    pub management: ManagementConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManagementConfig {
    #[serde(default)]
    pub cover_scraping: CoverScrapingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverScrapingConfig {
    #[serde(default)]
    pub tgdb_api_key: Option<String>,
    #[serde(default)]
    pub platform_ids: CoverScrapePlatformIds,
    #[serde(default = "default_cover_scrape_limit")]
    pub default_limit: u32,
    #[serde(default = "default_cover_scrape_delay_ms")]
    pub default_delay_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverScrapePlatformIds {
    #[serde(default)]
    pub nes: Vec<u32>,
    #[serde(default)]
    pub snes: Vec<u32>,
    #[serde(default)]
    pub genesis: Vec<u32>,
    #[serde(default)]
    pub gb: Vec<u32>,
    #[serde(default = "default_cover_scrape_platform_gba")]
    pub gba: Vec<u32>,
    #[serde(default = "default_cover_scrape_platform_n64")]
    pub n64: Vec<u32>,
    #[serde(default = "default_cover_scrape_platform_arcade")]
    pub arcade: Vec<u32>,
}

impl Default for CoverScrapePlatformIds {
    fn default() -> Self {
        Self {
            nes: Vec::new(),
            snes: Vec::new(),
            genesis: Vec::new(),
            gb: Vec::new(),
            gba: default_cover_scrape_platform_gba(),
            n64: default_cover_scrape_platform_n64(),
            arcade: default_cover_scrape_platform_arcade(),
        }
    }
}

impl Default for CoverScrapingConfig {
    fn default() -> Self {
        Self {
            tgdb_api_key: None,
            platform_ids: CoverScrapePlatformIds::default(),
            default_limit: default_cover_scrape_limit(),
            default_delay_ms: default_cover_scrape_delay_ms(),
        }
    }
}

fn default_cover_scrape_limit() -> u32 {
    200
}

fn default_cover_scrape_delay_ms() -> u32 {
    150
}

fn default_cover_scrape_platform_gba() -> Vec<u32> {
    vec![5]
}

fn default_cover_scrape_platform_n64() -> Vec<u32> {
    vec![3]
}

fn default_cover_scrape_platform_arcade() -> Vec<u32> {
    vec![23]
}

impl Default for AppConfig {
    fn default() -> Self {
        let app_root = default_app_root();

        Self {
            paths: PathsConfig {
                rom_root: app_root.join("roms"),
                db_path: app_root.join("data").join("arcade.db"),
                save_state_root: app_root.join("data").join("save-states"),
                core_root: app_root.join("cores"),
                bios_root: app_root.join("bios"),
            },
            emulation: EmulationConfig::default(),
            management: ManagementConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn default_config_path() -> PathBuf {
        default_config_path()
    }

    pub fn load_or_create(config_path: Option<&Path>) -> Result<(Self, PathBuf)> {
        let config_path = config_path
            .map(ToOwned::to_owned)
            .unwrap_or_else(Self::default_config_path);

        if config_path.exists() {
            let raw = fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read config at {}", config_path.display()))?;
            let mut config: AppConfig = toml::from_str(&raw)
                .with_context(|| format!("invalid config TOML at {}", config_path.display()))?;
            config.emulation.migrate_legacy_n64();
            config.emulation.apply_platform_defaults();
            config.ensure_dirs()?;
            return Ok((config, config_path));
        }

        let mut config = AppConfig::default();
        config.emulation.apply_platform_defaults();
        config.ensure_dirs()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let toml = toml::to_string_pretty(&config).context("failed to serialize config")?;
        fs::write(&config_path, toml)
            .with_context(|| format!("failed to write config at {}", config_path.display()))?;

        Ok((config, config_path))
    }

    pub fn save_to_path(&self, config_path: &Path) -> Result<()> {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let toml = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(config_path, toml)
            .with_context(|| format!("failed to write config at {}", config_path.display()))?;
        Ok(())
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        let paths = [
            self.paths.db_path.parent(),
            Some(self.paths.save_state_root.as_path()),
            Some(self.paths.core_root.as_path()),
            Some(self.paths.bios_root.as_path()),
        ];

        for maybe_path in paths.into_iter().flatten() {
            fs::create_dir_all(maybe_path)
                .with_context(|| format!("failed to create {}", maybe_path.display()))?;
        }

        Ok(())
    }

    pub fn preferred_core_for_system(&self, system: &str) -> Option<&'static str> {
        if system.eq_ignore_ascii_case("N64") {
            Some(self.emulation.n64.preferred_core.as_core_name())
        } else {
            None
        }
    }
}

fn prefer_native_unless_only_legacy_exists(native: PathBuf, legacy: PathBuf) -> PathBuf {
    if native.exists() || !legacy.exists() {
        native
    } else {
        legacy
    }
}

fn project_dirs_for(project_name: &str) -> Option<ProjectDirs> {
    ProjectDirs::from(
        PROJECT_DIR_QUALIFIER,
        PROJECT_DIR_ORGANIZATION,
        project_name,
    )
}

fn fallback_data_dir(project_name: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("."));
    PathBuf::from(format!("{home}/.local/share/{project_name}"))
}

fn fallback_config_path(project_name: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("."));
    PathBuf::from(format!("{home}/.config/{project_name}/config.toml"))
}

fn project_data_dir(project_name: &str) -> PathBuf {
    project_dirs_for(project_name)
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(|| fallback_data_dir(project_name))
}

fn project_config_path(project_name: &str) -> PathBuf {
    project_dirs_for(project_name)
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| fallback_config_path(project_name))
}

fn executable_root_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn default_app_root() -> PathBuf {
    default_app_root_with(
        executable_root_dir(),
        project_data_dir(PROJECT_NAME_NATIVE),
        project_data_dir(PROJECT_NAME_LEGACY),
    )
}

fn default_app_root_with(
    executable_root: Option<PathBuf>,
    native_data_dir: PathBuf,
    legacy_data_dir: PathBuf,
) -> PathBuf {
    executable_root
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or_else(|| {
            prefer_native_unless_only_legacy_exists(native_data_dir, legacy_data_dir)
        })
}

fn default_config_path() -> PathBuf {
    default_config_path_with(
        executable_root_dir(),
        project_config_path(PROJECT_NAME_NATIVE),
        project_config_path(PROJECT_NAME_LEGACY),
    )
}

fn default_config_path_with(
    executable_root: Option<PathBuf>,
    native_config_path: PathBuf,
    legacy_config_path: PathBuf,
) -> PathBuf {
    executable_root
        .filter(|root| !root.as_os_str().is_empty())
        .map(|root| root.join("config.toml"))
        .unwrap_or_else(|| {
            prefer_native_unless_only_legacy_exists(native_config_path, legacy_config_path)
        })
}

pub fn resolve_path_from_root(path: impl AsRef<Path>, root: &Path) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        return path.to_path_buf();
    }

    let direct = root.join(path);
    if direct.exists() {
        return direct;
    }

    if let Some(stripped) = strip_known_root_prefix(path, root) {
        return root.join(stripped);
    }

    direct
}

fn strip_known_root_prefix<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    if let Ok(stripped) = path.strip_prefix(Path::new("roms")) {
        if !stripped.as_os_str().is_empty() {
            return Some(stripped);
        }
    }

    let root_name = root.file_name()?;
    let stripped = path.strip_prefix(Path::new(root_name)).ok()?;
    if stripped.as_os_str().is_empty() {
        None
    } else {
        Some(stripped)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_app_root_with, default_config_path_with, resolve_path_from_root, AppConfig,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn resolves_legacy_workspace_relative_rom_paths_against_rom_root() {
        let dir = tempdir().expect("tempdir");
        let rom_root = dir.path().join("roms");
        let rom_path = rom_root.join("nes").join("test.nes");
        fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        fs::write(&rom_path, b"rom").expect("write rom");

        let resolved = resolve_path_from_root("roms/nes/test.nes", &rom_root);
        assert_eq!(resolved, rom_path);
    }

    #[test]
    fn preserves_nested_roms_directory_when_it_exists_under_root() {
        let dir = tempdir().expect("tempdir");
        let rom_root = dir.path().join("library");
        let rom_path = rom_root.join("roms").join("nes").join("test.nes");
        fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        fs::write(&rom_path, b"rom").expect("write rom");

        let resolved = resolve_path_from_root("roms/nes/test.nes", &rom_root);
        assert_eq!(resolved, rom_path);
    }

    #[test]
    fn app_config_defaults_n64_core_preference_to_mupen64plus_next() {
        let config = AppConfig::default();
        assert_eq!(
            config.preferred_core_for_system("N64"),
            Some("mupen64plus_next")
        );
        assert_eq!(
            config.emulation.n64.parallel_rdp_upscaling.as_core_value(),
            "1x"
        );
        assert_eq!(
            config.emulation.n64.parallel_profile.as_config_value(),
            "balanced"
        );
        assert_eq!(config.management.cover_scraping.default_limit, 200);
        assert_eq!(config.management.cover_scraping.default_delay_ms, 150);
        assert_eq!(config.management.cover_scraping.platform_ids.gba, vec![5]);
        assert_eq!(config.management.cover_scraping.platform_ids.n64, vec![3]);
        assert_eq!(
            config.management.cover_scraping.platform_ids.arcade,
            vec![23]
        );
    }

    #[test]
    fn legacy_parallel_n64_preference_maps_to_mupen64plus_next() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"

[emulation.n64]
preferred_core = "parallel_n64"
"#,
        )
        .expect("config");

        assert_eq!(
            config.preferred_core_for_system("N64"),
            Some("mupen64plus_next")
        );
    }

    #[test]
    fn app_config_deserializes_without_emulation_block() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"
"#,
        )
        .expect("config");

        assert_eq!(
            config.preferred_core_for_system("N64"),
            Some("mupen64plus_next")
        );
        assert_eq!(
            config.emulation.n64.parallel_rdp_upscaling.as_core_value(),
            "1x"
        );
        assert_eq!(
            config.emulation.n64.parallel_profile.as_config_value(),
            "balanced"
        );
        assert_eq!(config.management.cover_scraping.default_limit, 200);
    }

    #[test]
    fn app_config_deserializes_parallel_rdp_upscaling_override() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"

[emulation.n64]
parallel_rdp_upscaling = "2x"
"#,
        )
        .expect("config");

        assert_eq!(
            config.emulation.n64.parallel_rdp_upscaling.as_core_value(),
            "2x"
        );
        assert_eq!(
            config.emulation.n64.parallel_profile.as_config_value(),
            "balanced"
        );
    }

    #[test]
    fn app_config_deserializes_parallel_rdp_upscaling_4x_override() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"

[emulation.n64]
parallel_rdp_upscaling = "4x"
"#,
        )
        .expect("config");

        assert_eq!(
            config.emulation.n64.parallel_rdp_upscaling.as_core_value(),
            "4x"
        );
        assert_eq!(
            config.emulation.n64.parallel_profile.as_config_value(),
            "balanced"
        );
    }

    #[test]
    fn app_config_deserializes_parallel_profile_override() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"

[emulation.n64]
parallel_profile = "performance"
"#,
        )
        .expect("config");

        assert_eq!(
            config.emulation.n64.parallel_profile.as_config_value(),
            "performance"
        );
    }

    #[test]
    fn app_config_deserializes_without_management_block() {
        let config: AppConfig = toml::from_str(
            r#"
[paths]
rom_root = "/tmp/roms"
db_path = "/tmp/arcade.db"
save_state_root = "/tmp/save-states"
core_root = "/tmp/cores"
bios_root = "/tmp/bios"
"#,
        )
        .expect("config");

        assert_eq!(config.management.cover_scraping.default_limit, 200);
        assert_eq!(config.management.cover_scraping.default_delay_ms, 150);
    }

    #[test]
    fn default_config_path_prefers_executable_directory_when_available() {
        let portable_root = PathBuf::from("/portable-app");
        let resolved = default_config_path_with(
            Some(portable_root.clone()),
            PathBuf::from("/native/config.toml"),
            PathBuf::from("/legacy/config.toml"),
        );
        assert_eq!(resolved, portable_root.join("config.toml"));
    }

    #[test]
    fn default_app_root_prefers_executable_directory_when_available() {
        let portable_root = PathBuf::from("/portable-app");
        let resolved = default_app_root_with(
            Some(portable_root.clone()),
            PathBuf::from("/native-data"),
            PathBuf::from("/legacy-data"),
        );
        assert_eq!(resolved, portable_root);
    }
}
