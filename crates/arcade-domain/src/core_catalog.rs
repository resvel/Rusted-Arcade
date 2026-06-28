use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    classify_libretro_core_info, dependency_buildbot_base_url, dependency_core_root,
    normalize_system, resolve_core, runtime_arch, LibretroInfoIndex, PathsConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCatalog {
    pub entries: Vec<CoreCatalogEntry>,
}

impl CoreCatalog {
    pub fn for_paths(paths: &PathsConfig) -> Self {
        let core_root = dependency_core_root(paths);
        let entries = curated_catalog_entries(&core_root);
        Self { entries }
    }

    pub fn for_paths_with_buildbot_listing(
        paths: &PathsConfig,
        listing: Option<&BuildbotCoreListing>,
    ) -> Self {
        Self::for_paths_with_buildbot_listing_and_info(paths, listing, None)
    }

    pub fn for_paths_with_buildbot_listing_and_info(
        paths: &PathsConfig,
        listing: Option<&BuildbotCoreListing>,
        info_index: Option<&LibretroInfoIndex>,
    ) -> Self {
        let core_root = dependency_core_root(paths);
        let mut entries = curated_catalog_entries(&core_root);
        if let Some(listing) = listing {
            apply_buildbot_listing(&mut entries, listing, info_index, &core_root);
        }
        Self { entries }
    }

    pub fn entry(&self, id: &str) -> Option<&CoreCatalogEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn system_group(&self, system: &str) -> Option<CoreCatalogSystemGroup> {
        let normalized = normalize_system(system)?;
        let entries = self
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .systems
                    .iter()
                    .any(|candidate| candidate == &normalized)
            })
            .cloned()
            .collect::<Vec<_>>();

        if entries.is_empty() {
            None
        } else {
            Some(CoreCatalogSystemGroup::from_entries(normalized, entries))
        }
    }

    pub fn system_groups(&self) -> Vec<CoreCatalogSystemGroup> {
        let mut by_system: BTreeMap<String, Vec<CoreCatalogEntry>> = BTreeMap::new();
        for entry in &self.entries {
            for system in &entry.systems {
                by_system
                    .entry(system.clone())
                    .or_default()
                    .push(entry.clone());
            }
        }

        by_system
            .into_iter()
            .map(|(system, entries)| CoreCatalogSystemGroup::from_entries(system, entries))
            .collect()
    }

    pub fn remote_buildbot_entries(&self) -> Vec<CoreCatalogEntry> {
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| {
                entry.systems.iter().any(|system| system == "ALL")
                    && entry.compatibility == CoreCatalogCompatibility::Advanced
            })
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by(entry_sort);
        entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCatalogEntry {
    pub id: String,
    pub core_name: String,
    pub display_name: String,
    pub systems: Vec<String>,
    /// The downloadable buildbot zip file name when this is a buildbot entry.
    pub buildbot_file_name: Option<String>,
    /// The dylib member expected inside the buildbot archive, or the local
    /// compatibility dylib expected on disk for protected/local lanes.
    pub expected_archive_member: String,
    pub source: CoreCatalogSource,
    pub target_path: PathBuf,
    pub install_state: CoreCatalogInstallState,
    /// Whether this core is the resolver default for one of its systems.
    /// This is intentionally separate from `compatibility`: protected lanes can
    /// be defaults without being normal downloadable recommended catalog rows.
    pub is_default_core_for_system: bool,
    pub compatibility: CoreCatalogCompatibility,
    pub install_policy: CoreCatalogInstallPolicy,
    pub label: String,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

impl CoreCatalogEntry {
    pub fn installed(&self) -> bool {
        self.install_state.installed()
    }

    pub fn is_recommended_catalog_entry(&self) -> bool {
        self.compatibility == CoreCatalogCompatibility::Recommended
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildbotCoreListing {
    pub source_url: String,
    pub archive_file_names: Vec<String>,
}

impl BuildbotCoreListing {
    pub fn new(source_url: impl Into<String>, mut archive_file_names: Vec<String>) -> Self {
        archive_file_names.sort();
        archive_file_names.dedup();
        Self {
            source_url: source_url.into(),
            archive_file_names,
        }
    }

    pub fn contains_archive(&self, archive_file_name: &str) -> bool {
        self.archive_file_names
            .binary_search_by(|candidate| candidate.as_str().cmp(archive_file_name))
            .is_ok()
    }
}

pub fn parse_buildbot_core_listing(source_url: &str, html: &str) -> BuildbotCoreListing {
    let mut archive_file_names = Vec::new();
    for token in html
        .split(|ch: char| ch == '<' || ch == '>' || ch == '"' || ch == '\'' || ch.is_whitespace())
    {
        let file_name = token
            .rsplit('/')
            .next()
            .unwrap_or(token)
            .split('?')
            .next()
            .unwrap_or(token)
            .split('#')
            .next()
            .unwrap_or(token);
        if file_name.ends_with("_libretro.dylib.zip") {
            archive_file_names.push(file_name.to_string());
        }
    }
    BuildbotCoreListing::new(source_url, archive_file_names)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCatalogSource {
    LibretroBuildbot {
        base_url: String,
        archive_file_name: String,
    },
    ProtectedCompatibility {
        identity: String,
    },
    Unavailable {
        reason: String,
    },
}

impl CoreCatalogSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LibretroBuildbot { .. } => "libretro buildbot",
            Self::ProtectedCompatibility { .. } => "protected compatibility lane",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    pub fn can_install_in_app(&self) -> bool {
        matches!(self, Self::LibretroBuildbot { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreCatalogInstallPolicy {
    Downloadable,
    AdvancedDownloadable,
    ManualProtected,
    Unavailable,
}

impl CoreCatalogInstallPolicy {
    pub fn can_install_in_app(self) -> bool {
        matches!(self, Self::Downloadable | Self::AdvancedDownloadable)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Downloadable => "Downloadable",
            Self::AdvancedDownloadable => "Advanced download",
            Self::ManualProtected => "Manual protected import",
            Self::Unavailable => "Unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoreCatalogCompatibility {
    Recommended,
    Compatible,
    Optional,
    Advanced,
    Protected,
}

impl CoreCatalogCompatibility {
    pub fn label(self) -> &'static str {
        match self {
            Self::Recommended => "Recommended core",
            Self::Compatible => "Other compatible core",
            Self::Optional => "Optional compatible core",
            Self::Advanced => "Advanced core",
            Self::Protected => "Protected platform lane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCatalogInstallState {
    Installed { path: PathBuf, size_bytes: u64 },
    Missing { path: PathBuf },
    Empty { path: PathBuf },
}

impl CoreCatalogInstallState {
    pub fn from_target_path(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Self::Installed {
                path: path.to_path_buf(),
                size_bytes: metadata.len(),
            },
            Ok(metadata) if metadata.is_file() => Self::Empty {
                path: path.to_path_buf(),
            },
            _ => Self::Missing {
                path: path.to_path_buf(),
            },
        }
    }

    pub fn installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Installed { path, .. } | Self::Missing { path } | Self::Empty { path } => path,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreCatalogSystemGroup {
    pub system: String,
    pub recommended: Vec<CoreCatalogEntry>,
    pub compatible: Vec<CoreCatalogEntry>,
    pub advanced: Vec<CoreCatalogEntry>,
    pub protected: Vec<CoreCatalogEntry>,
}

impl CoreCatalogSystemGroup {
    fn from_entries(system: String, entries: Vec<CoreCatalogEntry>) -> Self {
        let mut group = Self {
            system,
            ..Self::default()
        };

        for entry in entries {
            match entry.compatibility {
                CoreCatalogCompatibility::Recommended => group.recommended.push(entry),
                CoreCatalogCompatibility::Compatible | CoreCatalogCompatibility::Optional => {
                    group.compatible.push(entry)
                }
                CoreCatalogCompatibility::Advanced => group.advanced.push(entry),
                CoreCatalogCompatibility::Protected => group.protected.push(entry),
            }
        }

        group.recommended.sort_by(entry_sort);
        group.compatible.sort_by(entry_sort);
        group.advanced.sort_by(entry_sort);
        group.protected.sort_by(entry_sort);
        group
    }

    pub fn entries(&self) -> impl Iterator<Item = &CoreCatalogEntry> {
        self.recommended
            .iter()
            .chain(self.compatible.iter())
            .chain(self.protected.iter())
            .chain(self.advanced.iter())
    }
}

fn entry_sort(left: &CoreCatalogEntry, right: &CoreCatalogEntry) -> std::cmp::Ordering {
    left.display_name
        .cmp(&right.display_name)
        .then(left.core_name.cmp(&right.core_name))
        .then(left.id.cmp(&right.id))
}

fn curated_catalog_entries(core_root: &Path) -> Vec<CoreCatalogEntry> {
    let mut entries = Vec::new();

    for (system, core_name, display_name, compatibility, notes, warnings) in [
        (
            "NES",
            "fceumm",
            "FCEUmm",
            CoreCatalogCompatibility::Recommended,
            vec!["Default NES core."],
            vec![],
        ),
        (
            "SNES",
            "snes9x",
            "Snes9x",
            CoreCatalogCompatibility::Recommended,
            vec!["Default SNES core."],
            vec![],
        ),
        (
            "GENESIS",
            "genesis_plus_gx",
            "Genesis Plus GX",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Genesis / Mega Drive core."],
            vec![],
        ),
        (
            "GB",
            "gambatte",
            "Gambatte",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Game Boy and Game Boy Color core."],
            vec![],
        ),
        (
            "GBA",
            "mgba",
            "mGBA",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Game Boy Advance core."],
            vec![],
        ),
        (
            "N64",
            "mupen64plus_next",
            "Mupen64Plus-Next",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Nintendo 64 core."],
            vec![],
        ),
        (
            "ARCADE",
            "fbneo",
            "FinalBurn Neo",
            CoreCatalogCompatibility::Recommended,
            vec!["Default arcade core for current Rusted Arcade arcade ROM-set guidance."],
            vec!["Arcade cores require ROM sets that match the selected core family."],
        ),
        (
            "ARCADE",
            "mame2003",
            "MAME 2003",
            CoreCatalogCompatibility::Optional,
            vec!["Optional arcade core for titles/sets that require the MAME 2003 family."],
            vec!["Use only with compatible MAME 2003 ROM sets."],
        ),
        (
            "ARCADE",
            "mame2003_plus",
            "MAME 2003-Plus",
            CoreCatalogCompatibility::Optional,
            vec!["Optional arcade core for MAME 2003-Plus compatible sets."],
            vec!["Use only with compatible MAME 2003-Plus ROM sets."],
        ),
        (
            "PSX",
            "mednafen_psx_hw",
            "Beetle PSX HW",
            CoreCatalogCompatibility::Recommended,
            vec!["Default PlayStation core."],
            vec![],
        ),
        (
            "DREAMCAST",
            "flycast",
            "Flycast",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Dreamcast core."],
            vec![],
        ),
        (
            "GAMECUBE",
            "dolphin",
            "Dolphin",
            CoreCatalogCompatibility::Recommended,
            vec!["Default GameCube core."],
            vec!["Requires Dolphin Sys resources and compatible runtime settings."],
        ),
        (
            "SATURN",
            "mednafen_saturn",
            "Beetle Saturn",
            CoreCatalogCompatibility::Recommended,
            vec!["Default Saturn core."],
            vec![],
        ),
        (
            "PCECD",
            "mednafen_pce_fast",
            "Beetle PCE Fast",
            CoreCatalogCompatibility::Recommended,
            vec!["Default PC Engine / PCE-CD core."],
            vec![],
        ),
        (
            "DOS",
            "dosbox_pure",
            "DOSBox Pure",
            CoreCatalogCompatibility::Recommended,
            vec!["Default DOS core."],
            vec![],
        ),
    ] {
        entries.push(buildbot_entry(
            system,
            core_name,
            display_name,
            compatibility,
            core_root,
            notes,
            warnings,
        ));
    }

    add_platform_compatibility_entries(&mut entries, core_root);
    entries.sort_by(|left, right| {
        left.systems
            .first()
            .cmp(&right.systems.first())
            .then(entry_sort(left, right))
    });
    entries
}

fn buildbot_entry(
    system: &str,
    core_name: &str,
    display_name: &str,
    compatibility: CoreCatalogCompatibility,
    core_root: &Path,
    notes: Vec<&str>,
    warnings: Vec<&str>,
) -> CoreCatalogEntry {
    let dylib_name = format!("{core_name}_libretro.dylib");
    let archive_file_name = format!("{dylib_name}.zip");
    let target_path = core_root.join(&dylib_name);
    let source = dependency_buildbot_base_url()
        .map(|base_url| CoreCatalogSource::LibretroBuildbot {
            base_url: base_url.to_string(),
            archive_file_name: archive_file_name.clone(),
        })
        .unwrap_or_else(|| CoreCatalogSource::Unavailable {
            reason: format!(
                "No libretro buildbot source is configured for runtime architecture {}",
                runtime_arch()
            ),
        });

    let install_policy = if matches!(source, CoreCatalogSource::LibretroBuildbot { .. }) {
        CoreCatalogInstallPolicy::Downloadable
    } else {
        CoreCatalogInstallPolicy::Unavailable
    };

    CoreCatalogEntry {
        id: format!("core-{core_name}"),
        core_name: core_name.to_string(),
        display_name: display_name.to_string(),
        systems: vec![system.to_string()],
        buildbot_file_name: Some(archive_file_name),
        expected_archive_member: dylib_name,
        source,
        target_path: target_path.clone(),
        install_state: CoreCatalogInstallState::from_target_path(&target_path),
        is_default_core_for_system: resolve_core(system, None) == core_name,
        compatibility,
        install_policy,
        label: compatibility.label().to_string(),
        notes: notes.into_iter().map(str::to_string).collect(),
        warnings: warnings.into_iter().map(str::to_string).collect(),
    }
}

fn add_platform_compatibility_entries(entries: &mut Vec<CoreCatalogEntry>, core_root: &Path) {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        entries.push(protected_entry(
            CompatibilityEntrySpec {
                id: "compat-pcarmsx2-arm64",
                system: "PS2",
                core_name: "pcarmsx2",
                display_name: "pcarmsx2 Apple Silicon",
                dylib_name: "pcarmsx2_libretro.dylib",
                notes: vec![
                    "Protected native ARM64 PS2 compatibility lane; not a stock buildbot replacement.",
                ],
                warnings: vec!["Requires user-provided PS2 BIOS and local compatibility resources."],
            },
            core_root,
        ));
        entries.push(protected_entry(
            CompatibilityEntrySpec {
                id: "compat-mupen64plus-next-arm64-dynarec",
                system: "N64",
                core_name: "mupen64plus_next",
                display_name: "Mupen64Plus-Next ARM64 dynarec",
                dylib_name: "mupen64plus_next_dynarec_arm64_libretro.dylib",
                notes: vec!["Protected ARM64 dynarec compatibility build for N64."],
                warnings: vec![
                    "Separate from the standard upstream buildbot Mupen64Plus-Next core.",
                ],
            },
            core_root,
        ));
    } else if cfg!(target_os = "macos") {
        entries.push(protected_entry(
            CompatibilityEntrySpec {
                id: "compat-pcsx2-metal-poc-x86_64",
                system: "PS2",
                core_name: "pcsx2",
                display_name: "PCSX2 Metal PoC Rosetta",
                dylib_name: "pcsx2_metal_poc_libretro.dylib",
                notes: vec!["Protected x86_64/Rosetta PS2 Metal compatibility lane."],
                warnings: vec![
                    "Do not replace this lane with generic buildbot PS2 cores automatically.",
                ],
            },
            core_root,
        ));
    }

    entries.push(advanced_entry(
        CompatibilityEntrySpec {
            id: "advanced-ps2-buildbot-pcsx2",
            system: "PS2",
            core_name: "pcsx2",
            display_name: "PCSX2 / LRPS2 buildbot core",
            dylib_name: "pcsx2_libretro.dylib",
            notes: vec![
                "Generic buildbot PS2 core exposed only as an advanced/experimental catalog entry.",
            ],
            warnings: vec![
                "PS2 platform-specific Rusted Arcade lanes are protected; this is not a recommended replacement.",
            ],
        },
        core_root,
    ));
}

struct CompatibilityEntrySpec {
    id: &'static str,
    system: &'static str,
    core_name: &'static str,
    display_name: &'static str,
    dylib_name: &'static str,
    notes: Vec<&'static str>,
    warnings: Vec<&'static str>,
}

fn protected_entry(spec: CompatibilityEntrySpec, core_root: &Path) -> CoreCatalogEntry {
    let target_path = core_root.join(spec.dylib_name);
    CoreCatalogEntry {
        id: spec.id.to_string(),
        core_name: spec.core_name.to_string(),
        display_name: spec.display_name.to_string(),
        systems: vec![spec.system.to_string()],
        buildbot_file_name: None,
        expected_archive_member: spec.dylib_name.to_string(),
        source: CoreCatalogSource::ProtectedCompatibility {
            identity: spec.id.to_string(),
        },
        target_path: target_path.clone(),
        install_state: CoreCatalogInstallState::from_target_path(&target_path),
        is_default_core_for_system: resolve_core(spec.system, None) == spec.core_name,
        compatibility: CoreCatalogCompatibility::Protected,
        install_policy: CoreCatalogInstallPolicy::ManualProtected,
        label: CoreCatalogCompatibility::Protected.label().to_string(),
        notes: spec.notes.into_iter().map(str::to_string).collect(),
        warnings: spec.warnings.into_iter().map(str::to_string).collect(),
    }
}

fn advanced_entry(spec: CompatibilityEntrySpec, core_root: &Path) -> CoreCatalogEntry {
    advanced_buildbot_entry(
        spec.id,
        spec.system,
        spec.core_name,
        spec.display_name,
        spec.dylib_name,
        core_root,
        spec.notes.into_iter().map(str::to_string).collect(),
        spec.warnings.into_iter().map(str::to_string).collect(),
    )
}

fn advanced_buildbot_entry(
    id: impl Into<String>,
    system: impl Into<String>,
    core_name: impl Into<String>,
    display_name: impl Into<String>,
    dylib_name: impl Into<String>,
    core_root: &Path,
    notes: Vec<String>,
    warnings: Vec<String>,
) -> CoreCatalogEntry {
    let dylib_name = dylib_name.into();
    let archive_file_name = format!("{dylib_name}.zip");
    let target_path = core_root.join(&dylib_name);
    let source = dependency_buildbot_base_url()
        .map(|base_url| CoreCatalogSource::LibretroBuildbot {
            base_url: base_url.to_string(),
            archive_file_name: archive_file_name.clone(),
        })
        .unwrap_or_else(|| CoreCatalogSource::Unavailable {
            reason: format!(
                "No libretro buildbot source is configured for runtime architecture {}",
                runtime_arch()
            ),
        });

    let install_policy = if matches!(source, CoreCatalogSource::LibretroBuildbot { .. }) {
        CoreCatalogInstallPolicy::AdvancedDownloadable
    } else {
        CoreCatalogInstallPolicy::Unavailable
    };

    CoreCatalogEntry {
        id: id.into(),
        core_name: core_name.into(),
        display_name: display_name.into(),
        systems: vec![system.into()],
        buildbot_file_name: Some(archive_file_name),
        expected_archive_member: dylib_name,
        source,
        target_path: target_path.clone(),
        install_state: CoreCatalogInstallState::from_target_path(&target_path),
        is_default_core_for_system: false,
        compatibility: CoreCatalogCompatibility::Advanced,
        install_policy,
        label: CoreCatalogCompatibility::Advanced.label().to_string(),
        notes,
        warnings,
    }
}

fn apply_buildbot_listing(
    entries: &mut Vec<CoreCatalogEntry>,
    listing: &BuildbotCoreListing,
    info_index: Option<&LibretroInfoIndex>,
    core_root: &Path,
) {
    for entry in entries.iter_mut() {
        let Some(archive_file_name) = entry.buildbot_file_name.as_deref() else {
            continue;
        };
        if !listing.contains_archive(archive_file_name) {
            entry.source = CoreCatalogSource::Unavailable {
                reason: format!(
                    "{} is not present in the live buildbot listing for {}",
                    archive_file_name,
                    runtime_arch()
                ),
            };
            entry.install_policy = CoreCatalogInstallPolicy::Unavailable;
            entry.warnings.push(String::from(
                "This catalog entry was not present in the latest live buildbot listing.",
            ));
        }
    }

    for archive_file_name in &listing.archive_file_names {
        if entries
            .iter()
            .any(|entry| entry.buildbot_file_name.as_deref() == Some(archive_file_name.as_str()))
        {
            continue;
        }
        let Some(core_name) = archive_file_name.strip_suffix("_libretro.dylib.zip") else {
            continue;
        };
        let dylib_name = format!("{core_name}_libretro.dylib");
        let core_info = info_index.and_then(|index| index.get(core_name));
        let classification = core_info.map(classify_libretro_core_info);
        let systems = classification
            .as_ref()
            .map(|classification| classification.systems.clone())
            .unwrap_or_else(|| vec![String::from("ALL")]);
        let display_name = core_info
            .and_then(|info| info.display_name.as_deref())
            .filter(|display_name| !display_name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| remote_core_display_name(core_name));
        entries.push(advanced_buildbot_entry(
            format!("remote-buildbot-{core_name}"),
            systems
                .first()
                .cloned()
                .unwrap_or_else(|| String::from("ALL")),
            core_name.to_string(),
            display_name,
            dylib_name,
            core_root,
            vec![format!(
                "Discovered from live buildbot listing: {}",
                listing.source_url
            )],
            vec![String::from(
                "Unclassified live buildbot core. It may be untested or incompatible; use only from Advanced Core Browser.",
            )],
        ));
        if let Some(entry) = entries.last_mut() {
            if let Some(classification) = classification {
                entry.systems = classification.systems;
                entry.compatibility = classification.compatibility;
                entry.label = classification.compatibility.label().to_string();
                entry.notes.extend(classification.notes);
                entry.warnings = classification.warnings;
                if classification.compatibility == CoreCatalogCompatibility::Compatible {
                    entry.install_policy = CoreCatalogInstallPolicy::Downloadable;
                }
            } else if systems.len() > 1 {
                entry.systems = systems;
            }
        }
    }
    entries.sort_by(|left, right| {
        left.systems
            .first()
            .cmp(&right.systems.first())
            .then(entry_sort(left, right))
    });
}

fn remote_core_display_name(core_name: &str) -> String {
    core_name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scan_dependency_report, DependencyState};
    use tempfile::tempdir;

    fn paths(root: &Path) -> PathsConfig {
        PathsConfig {
            rom_root: root.join("roms"),
            db_path: root.join("db.sqlite"),
            save_state_root: root.join("saves"),
            core_root: root.join("cores"),
            bios_root: root.join("bios"),
        }
    }

    #[test]
    fn recommended_cores_are_visible_for_their_systems() {
        let temp = tempdir().expect("tempdir");
        let catalog = CoreCatalog::for_paths(&paths(temp.path()));

        for (system, core_name) in [
            ("NES", "fceumm"),
            ("SNES", "snes9x"),
            ("GENESIS", "genesis_plus_gx"),
            ("GB", "gambatte"),
            ("GBA", "mgba"),
            ("N64", "mupen64plus_next"),
            ("ARCADE", "fbneo"),
            ("PSX", "mednafen_psx_hw"),
            ("DREAMCAST", "flycast"),
            ("GAMECUBE", "dolphin"),
            ("SATURN", "mednafen_saturn"),
            ("PCECD", "mednafen_pce_fast"),
            ("DOS", "dosbox_pure"),
        ] {
            let group = catalog.system_group(system).expect("system group");
            assert!(
                group
                    .recommended
                    .iter()
                    .any(|entry| entry.core_name == core_name
                        && entry.is_recommended_catalog_entry()),
                "missing recommended {core_name} for {system}"
            );
        }
    }

    #[test]
    fn installed_and_missing_state_reflects_non_empty_files() {
        let temp = tempdir().expect("tempdir");
        let paths = paths(temp.path());
        fs::create_dir_all(&paths.core_root).expect("core root");
        fs::write(paths.core_root.join("fceumm_libretro.dylib"), b"core").expect("write core");
        fs::write(paths.core_root.join("snes9x_libretro.dylib"), b"").expect("write empty core");

        let catalog = CoreCatalog::for_paths(&paths);

        let nes = catalog.entry("core-fceumm").expect("nes entry");
        assert!(matches!(
            nes.install_state,
            CoreCatalogInstallState::Installed { size_bytes: 4, .. }
        ));

        let snes = catalog.entry("core-snes9x").expect("snes entry");
        assert!(matches!(
            snes.install_state,
            CoreCatalogInstallState::Empty { .. }
        ));
        assert!(!snes.installed());

        let gba = catalog.entry("core-mgba").expect("gba entry");
        assert!(matches!(
            gba.install_state,
            CoreCatalogInstallState::Missing { .. }
        ));
    }

    #[test]
    fn active_target_core_root_is_respected() {
        let temp = tempdir().expect("tempdir");
        let paths = paths(temp.path());
        let expected_root = dependency_core_root(&paths);
        let catalog = CoreCatalog::for_paths(&paths);

        let nes = catalog.entry("core-fceumm").expect("nes entry");
        assert_eq!(nes.target_path, expected_root.join("fceumm_libretro.dylib"));
    }

    #[test]
    fn advanced_and_optional_entries_do_not_become_recommended() {
        let temp = tempdir().expect("tempdir");
        let catalog = CoreCatalog::for_paths(&paths(temp.path()));
        let arcade = catalog.system_group("ARCADE").expect("arcade group");

        assert!(arcade
            .compatible
            .iter()
            .any(|entry| entry.core_name == "mame2003" && !entry.is_recommended_catalog_entry()));
        assert!(arcade.compatible.iter().any(
            |entry| entry.core_name == "mame2003_plus" && !entry.is_recommended_catalog_entry()
        ));

        let ps2 = catalog.system_group("PS2").expect("ps2 group");
        assert!(ps2
            .advanced
            .iter()
            .any(|entry| entry.id == "advanced-ps2-buildbot-pcsx2"
                && !entry.is_recommended_catalog_entry()));
        assert!(ps2.recommended.is_empty());
    }

    #[test]
    fn ps2_platform_lane_remains_protected() {
        let temp = tempdir().expect("tempdir");
        let catalog = CoreCatalog::for_paths(&paths(temp.path()));
        let ps2 = catalog.system_group("PS2").expect("ps2 group");

        if cfg!(target_os = "macos") {
            assert_eq!(ps2.protected.len(), 1);
            assert!(ps2.protected[0].id.starts_with("compat-"));
            assert!(matches!(
                ps2.protected[0].source,
                CoreCatalogSource::ProtectedCompatibility { .. }
            ));
        }
        assert!(ps2.protected.iter().all(|entry| {
            entry.compatibility == CoreCatalogCompatibility::Protected
                && !entry.is_recommended_catalog_entry()
                && entry.install_policy == CoreCatalogInstallPolicy::ManualProtected
        }));
        assert!(ps2.advanced.iter().any(|entry| {
            entry.id == "advanced-ps2-buildbot-pcsx2"
                && entry.install_policy == CoreCatalogInstallPolicy::AdvancedDownloadable
        }));
    }

    #[test]
    fn catalog_does_not_change_dependency_report_missing_required_counts() {
        let temp = tempdir().expect("tempdir");
        let paths = paths(temp.path());
        let before = scan_dependency_report(&paths).missing_required_count();
        let catalog = CoreCatalog::for_paths(&paths);
        let after = scan_dependency_report(&paths).missing_required_count();

        assert!(catalog.entry("advanced-ps2-buildbot-pcsx2").is_some());
        assert_eq!(before, after);
        assert!(!scan_dependency_report(&paths)
            .components
            .iter()
            .any(|status| {
                status.component.id == "advanced-ps2-buildbot-pcsx2"
                    && status.state == DependencyState::Missing
            }));
    }

    #[test]
    fn grouping_sort_is_stable() {
        let temp = tempdir().expect("tempdir");
        let catalog = CoreCatalog::for_paths(&paths(temp.path()));
        let groups = catalog.system_groups();
        let systems = groups
            .iter()
            .map(|group| group.system.as_str())
            .collect::<Vec<_>>();
        let mut sorted = systems.clone();
        sorted.sort_unstable();
        assert_eq!(systems, sorted);

        let arcade = catalog.system_group("ARCADE").expect("arcade group");
        let compatible_names = arcade
            .compatible
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(compatible_names, vec!["MAME 2003", "MAME 2003-Plus"]);
    }

    #[test]
    fn buildbot_listing_parser_extracts_core_archives_from_h5ai_html() {
        let html = r#"
            <a href="/nightly/apple/osx/arm64/latest/fceumm_libretro.dylib.zip">fceumm_libretro.dylib.zip</a>
            <a href='/nightly/apple/osx/arm64/latest/snes9x_libretro.dylib.zip?download=1'>Snes9x</a>
            <a href="/nightly/apple/osx/arm64/latest/not-a-core.txt">ignore</a>
            <a href="../parent">Parent Directory</a>
            <a href="/nightly/apple/osx/arm64/latest/fceumm_libretro.dylib.zip">duplicate</a>
        "#;

        let listing = parse_buildbot_core_listing("https://buildbot.example/latest", html);

        assert_eq!(
            listing.archive_file_names,
            vec![
                "fceumm_libretro.dylib.zip".to_string(),
                "snes9x_libretro.dylib.zip".to_string()
            ]
        );
    }

    #[test]
    fn live_buildbot_listing_marks_missing_curated_entries_unavailable() {
        let temp = tempdir().expect("tempdir");
        let listing = BuildbotCoreListing::new(
            "https://buildbot.example/latest",
            vec!["fceumm_libretro.dylib.zip".to_string()],
        );

        let catalog =
            CoreCatalog::for_paths_with_buildbot_listing(&paths(temp.path()), Some(&listing));

        let nes = catalog.entry("core-fceumm").expect("fceumm entry");
        assert_eq!(nes.install_policy, CoreCatalogInstallPolicy::Downloadable);
        let snes = catalog.entry("core-snes9x").expect("snes9x entry");
        assert_eq!(snes.install_policy, CoreCatalogInstallPolicy::Unavailable);
        assert!(matches!(snes.source, CoreCatalogSource::Unavailable { .. }));
    }

    #[test]
    fn unknown_live_buildbot_entries_become_advanced_global_entries() {
        let temp = tempdir().expect("tempdir");
        let listing = BuildbotCoreListing::new(
            "https://buildbot.example/latest",
            vec![
                "fceumm_libretro.dylib.zip".to_string(),
                "vecx_libretro.dylib.zip".to_string(),
            ],
        );

        let catalog =
            CoreCatalog::for_paths_with_buildbot_listing(&paths(temp.path()), Some(&listing));
        let nes = catalog.system_group("NES").expect("nes group");
        assert!(nes
            .advanced
            .iter()
            .all(|entry| entry.id != "remote-buildbot-vecx"));
        let remote_entries = catalog.remote_buildbot_entries();
        let remote = remote_entries
            .iter()
            .find(|entry| entry.id == "remote-buildbot-vecx")
            .expect("remote vecx entry");

        assert_eq!(remote.core_name, "vecx");
        assert_eq!(remote.systems, vec!["ALL".to_string()]);
        assert_eq!(
            remote.install_policy,
            CoreCatalogInstallPolicy::AdvancedDownloadable
        );
    }

    #[test]
    fn libretro_info_can_classify_reliable_remote_only_entries() {
        let temp = tempdir().expect("tempdir");
        let listing = BuildbotCoreListing::new(
            "https://buildbot.example/latest",
            vec![
                "quicknes_libretro.dylib.zip".to_string(),
                "vecx_libretro.dylib.zip".to_string(),
            ],
        );
        let info_index = LibretroInfoIndex::from_infos([(
            "quicknes",
            r#"
                display_name = "QuickNES"
                systemid = "Nintendo - Nintendo Entertainment System"
                database = "Nintendo - Nintendo Entertainment System"
            "#,
        )]);

        let catalog = CoreCatalog::for_paths_with_buildbot_listing_and_info(
            &paths(temp.path()),
            Some(&listing),
            Some(&info_index),
        );
        let nes = catalog.system_group("NES").expect("nes group");
        let quicknes = nes
            .compatible
            .iter()
            .find(|entry| entry.id == "remote-buildbot-quicknes")
            .expect("classified quicknes");

        assert_eq!(quicknes.display_name, "QuickNES");
        assert_eq!(quicknes.systems, vec!["NES".to_string()]);
        assert_eq!(quicknes.compatibility, CoreCatalogCompatibility::Compatible);
        assert_eq!(
            quicknes.install_policy,
            CoreCatalogInstallPolicy::Downloadable
        );
        assert!(catalog
            .remote_buildbot_entries()
            .iter()
            .any(|entry| entry.id == "remote-buildbot-vecx"));
    }

    #[test]
    fn libretro_info_does_not_override_curated_catalog_entries() {
        let temp = tempdir().expect("tempdir");
        let listing = BuildbotCoreListing::new(
            "https://buildbot.example/latest",
            vec!["fceumm_libretro.dylib.zip".to_string()],
        );
        let info_index = LibretroInfoIndex::from_infos([(
            "fceumm",
            r#"
                display_name = "Wrong Name"
                systemid = "Sony - PlayStation 2"
            "#,
        )]);

        let catalog = CoreCatalog::for_paths_with_buildbot_listing_and_info(
            &paths(temp.path()),
            Some(&listing),
            Some(&info_index),
        );
        let fceumm = catalog.entry("core-fceumm").expect("curated fceumm");

        assert_eq!(fceumm.display_name, "FCEUmm");
        assert_eq!(fceumm.systems, vec!["NES".to_string()]);
        assert_eq!(fceumm.compatibility, CoreCatalogCompatibility::Recommended);
    }
}
