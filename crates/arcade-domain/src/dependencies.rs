use std::path::{Path, PathBuf};

use crate::{
    find_dolphin_sys_directory, find_pcecd_bios_file, find_saturn_bios_file,
    get_arcade_bios_directory, get_dolphin_sys_directory, get_missing_arcade_shared_bios_files,
    get_pcecd_bios_directory, get_saturn_bios_directory, runtime_arch, PathsConfig,
    ARCADE_SHARED_BIOS_FILES, PCECD_ACCEPTED_BIOS_FILES, SATURN_ACCEPTED_BIOS_FILES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyComponentKind {
    Core,
    CompatCore,
    Resource,
    Bios,
    Romset,
}

impl DependencyComponentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::CompatCore => "Compatibility Core",
            Self::Resource => "Resource",
            Self::Bios => "BIOS",
            Self::Romset => "ROM Set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    LibretroBuildbot { file_name: String },
    Homebrew { package_name: String },
    UpstreamDownload { url: String },
    ExternalGuided { url: String },
    LocalImport,
    UserProvided,
}

impl DependencySource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LibretroBuildbot { .. } => "libretro buildbot",
            Self::Homebrew { .. } => "Homebrew",
            Self::UpstreamDownload { .. } => "upstream download",
            Self::ExternalGuided { .. } => "guided upstream download",
            Self::LocalImport => "local import",
            Self::UserProvided => "user provided",
        }
    }

    pub fn can_install_in_app(&self) -> bool {
        matches!(
            self,
            Self::LibretroBuildbot { .. }
                | Self::Homebrew { .. }
                | Self::UpstreamDownload { .. }
                | Self::LocalImport
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyCheck {
    PathExists(PathBuf),
    AnyNamedFile {
        directory: PathBuf,
        names: Vec<String>,
    },
    AllNamedFiles {
        directory: PathBuf,
        names: Vec<String>,
    },
    DirectoryLooksLikeDolphinSys,
    AnyFileInDirectory(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyComponent {
    pub id: String,
    pub system: String,
    pub title: String,
    pub description: String,
    pub kind: DependencyComponentKind,
    pub required: bool,
    pub target_path: PathBuf,
    pub check: DependencyCheck,
    pub source: DependencySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyState {
    Ready,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyComponentStatus {
    pub component: DependencyComponent,
    pub state: DependencyState,
    pub detail: String,
}

impl DependencyComponentStatus {
    pub fn ready(&self) -> bool {
        self.state == DependencyState::Ready
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyReport {
    pub components: Vec<DependencyComponentStatus>,
}

impl DependencyReport {
    pub fn missing_required_count(&self) -> usize {
        self.components
            .iter()
            .filter(|status| status.component.required && !status.ready())
            .count()
    }

    pub fn missing_optional_count(&self) -> usize {
        self.components
            .iter()
            .filter(|status| !status.component.required && !status.ready())
            .count()
    }

    pub fn ready_count(&self) -> usize {
        self.components
            .iter()
            .filter(|status| status.ready())
            .count()
    }

    pub fn has_missing_required(&self) -> bool {
        self.missing_required_count() > 0
    }
}

pub fn dependency_manifest(paths: &PathsConfig) -> Vec<DependencyComponent> {
    let core_root = dependency_core_root(paths);
    let buildbot_core_names = [
        ("NES", "fceumm", true),
        ("SNES", "snes9x", true),
        ("GENESIS", "genesis_plus_gx", true),
        ("GB", "gambatte", true),
        ("GBA", "mgba", true),
        ("N64", "mupen64plus_next", true),
        ("ARCADE", "fbneo", true),
        ("ARCADE", "mame2003", false),
        ("ARCADE", "mame2003_plus", false),
        ("PSX", "mednafen_psx_hw", true),
        ("DREAMCAST", "flycast", true),
        ("GAMECUBE", "dolphin", true),
        ("SATURN", "mednafen_saturn", true),
        ("PCECD", "mednafen_pce_fast", true),
        ("DOS", "dosbox_pure", true),
    ];

    let mut components = buildbot_core_names
        .into_iter()
        .map(|(system, core_name, required)| buildbot_core(system, core_name, required, &core_root))
        .collect::<Vec<_>>();

    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        components.push(local_compat_core(
            "compat-pcarmsx2-arm64",
            "PS2",
            "pcarmsx2 Apple Silicon core",
            "Required native ARM64 PS2 compatibility build.",
            "pcarmsx2_libretro.dylib",
            true,
            &core_root,
        ));
        components.push(local_compat_core(
            "compat-mupen64plus-next-arm64-dynarec",
            "N64",
            "Mupen64Plus Next ARM64 dynarec core",
            "Optional fast ARM64 N64 compatibility build.",
            "mupen64plus_next_dynarec_arm64_libretro.dylib",
            false,
            &core_root,
        ));
        components.push(path_resource(
            "pcarmsx2-metal-source",
            "PS2",
            "pcarmsx2 Metal shader source",
            "Runtime Metal shader source used when a compatible metallib is not present.",
            paths.core_root.join("metal_source"),
            true,
            DependencySource::LocalImport,
            DependencyCheck::PathExists(paths.core_root.join("metal_source")),
        ));
        components.push(path_resource(
            "pcarmsx2-resources",
            "PS2",
            "pcarmsx2 runtime resources",
            "Resource directory used by the native ARM64 PS2 compatibility core.",
            paths.core_root.join("resources"),
            true,
            DependencySource::LocalImport,
            DependencyCheck::PathExists(paths.core_root.join("resources")),
        ));
    } else if cfg!(target_os = "macos") {
        components.push(local_compat_core(
            "compat-pcsx2-metal-poc-x86_64",
            "PS2",
            "PCSX2 Metal PoC Rosetta core",
            "Required x86_64/Rosetta PS2 compatibility build.",
            "pcsx2_metal_poc_libretro.dylib",
            true,
            &core_root,
        ));
        components.push(path_resource(
            "pcsx2-metal-source",
            "PS2",
            "PCSX2 Metal shader source",
            "Runtime Metal shader source used by the Rosetta PS2 Metal PoC.",
            paths.core_root.join("x86_64").join("metal_source"),
            true,
            DependencySource::LocalImport,
            DependencyCheck::PathExists(paths.core_root.join("x86_64").join("metal_source")),
        ));
    }

    components.push(path_resource(
        "dolphin-sys",
        "GAMECUBE",
        "Dolphin Sys folder",
        "Dolphin support data folder. Import an existing Sys folder.",
        get_dolphin_sys_directory(&paths.rom_root, Some(&paths.bios_root)),
        true,
        DependencySource::LocalImport,
        DependencyCheck::DirectoryLooksLikeDolphinSys,
    ));

    components.push(bios_component(
        "pcecd-bios",
        "PCECD",
        "PCE-CD BIOS",
        "User-provided PCE-CD system card BIOS.",
        get_pcecd_bios_directory(&paths.rom_root, Some(&paths.bios_root)),
        PCECD_ACCEPTED_BIOS_FILES,
    ));
    components.push(bios_component(
        "saturn-bios",
        "SATURN",
        "Saturn BIOS",
        "User-provided Saturn BIOS.",
        get_saturn_bios_directory(&paths.rom_root, Some(&paths.bios_root)),
        SATURN_ACCEPTED_BIOS_FILES,
    ));
    components.push(DependencyComponent {
        id: "arcade-shared-bios".to_string(),
        system: "ARCADE".to_string(),
        title: "Shared arcade BIOS files".to_string(),
        description: "User-provided shared arcade BIOS archives.".to_string(),
        kind: DependencyComponentKind::Bios,
        required: false,
        target_path: get_arcade_bios_directory(&paths.rom_root, Some(&paths.bios_root)),
        check: DependencyCheck::AllNamedFiles {
            directory: get_arcade_bios_directory(&paths.rom_root, Some(&paths.bios_root)),
            names: ARCADE_SHARED_BIOS_FILES
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        },
        source: DependencySource::UserProvided,
    });
    components.push(DependencyComponent {
        id: "ps2-bios".to_string(),
        system: "PS2".to_string(),
        title: "PS2 BIOS".to_string(),
        description:
            "User-provided PlayStation 2 BIOS files. Rusted Arcade never downloads BIOS files."
                .to_string(),
        kind: DependencyComponentKind::Bios,
        required: true,
        target_path: paths.bios_root.join("pcsx2").join("bios"),
        check: DependencyCheck::AnyFileInDirectory(paths.bios_root.join("pcsx2").join("bios")),
        source: DependencySource::UserProvided,
    });

    components
}

pub fn scan_dependency_report(paths: &PathsConfig) -> DependencyReport {
    let components = dependency_manifest(paths)
        .into_iter()
        .map(|component| {
            let (state, detail) = scan_component(&component, paths);
            DependencyComponentStatus {
                component,
                state,
                detail,
            }
        })
        .collect();
    DependencyReport { components }
}

pub fn dependency_core_root(paths: &PathsConfig) -> PathBuf {
    if cfg!(target_os = "macos") && runtime_arch() == "x86_64" {
        paths.core_root.join("x86_64")
    } else {
        paths.core_root.clone()
    }
}

pub fn dependency_buildbot_base_url() -> Option<&'static str> {
    match runtime_arch() {
        "x86_64" => Some("https://buildbot.libretro.com/nightly/apple/osx/x86_64/latest"),
        "arm64" | "aarch64" => Some("https://buildbot.libretro.com/nightly/apple/osx/arm64/latest"),
        _ => None,
    }
}

fn buildbot_core(
    system: &str,
    core_name: &str,
    required: bool,
    core_root: &Path,
) -> DependencyComponent {
    let file_name = format!("{core_name}_libretro.dylib");
    DependencyComponent {
        id: format!("core-{core_name}"),
        system: system.to_string(),
        title: format!("{core_name} core"),
        description: "Standard libretro core from the upstream buildbot.".to_string(),
        kind: DependencyComponentKind::Core,
        required,
        target_path: core_root.join(&file_name),
        check: DependencyCheck::PathExists(core_root.join(&file_name)),
        source: DependencySource::LibretroBuildbot { file_name },
    }
}

fn local_compat_core(
    id: &str,
    system: &str,
    title: &str,
    description: &str,
    file_name: &str,
    required: bool,
    core_root: &Path,
) -> DependencyComponent {
    DependencyComponent {
        id: id.to_string(),
        system: system.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        kind: DependencyComponentKind::CompatCore,
        required,
        target_path: core_root.join(file_name),
        check: DependencyCheck::PathExists(core_root.join(file_name)),
        source: DependencySource::LocalImport,
    }
}

fn path_resource(
    id: &str,
    system: &str,
    title: &str,
    description: &str,
    target_path: PathBuf,
    required: bool,
    source: DependencySource,
    check: DependencyCheck,
) -> DependencyComponent {
    DependencyComponent {
        id: id.to_string(),
        system: system.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        kind: DependencyComponentKind::Resource,
        required,
        target_path,
        check,
        source,
    }
}

fn bios_component(
    id: &str,
    system: &str,
    title: &str,
    description: &str,
    directory: PathBuf,
    accepted_names: &[&str],
) -> DependencyComponent {
    DependencyComponent {
        id: id.to_string(),
        system: system.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        kind: DependencyComponentKind::Bios,
        required: true,
        target_path: directory.clone(),
        check: DependencyCheck::AnyNamedFile {
            directory,
            names: accepted_names
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        },
        source: DependencySource::UserProvided,
    }
}

fn scan_component(
    component: &DependencyComponent,
    paths: &PathsConfig,
) -> (DependencyState, String) {
    match &component.check {
        DependencyCheck::PathExists(path) => {
            if path.exists() {
                (DependencyState::Ready, format!("Found {}", path.display()))
            } else {
                (
                    DependencyState::Missing,
                    format!("Missing {}", path.display()),
                )
            }
        }
        DependencyCheck::AnyNamedFile { directory, names } => {
            if component.id == "pcecd-bios"
                && find_pcecd_bios_file(&paths.rom_root, Some(&paths.bios_root)).is_some()
            {
                return (
                    DependencyState::Ready,
                    "Found accepted BIOS file.".to_string(),
                );
            }
            if component.id == "saturn-bios"
                && find_saturn_bios_file(&paths.rom_root, Some(&paths.bios_root)).is_some()
            {
                return (
                    DependencyState::Ready,
                    "Found accepted BIOS file.".to_string(),
                );
            }
            let found = names.iter().any(|name| directory.join(name).is_file());
            if found {
                (DependencyState::Ready, "Found accepted file.".to_string())
            } else {
                (
                    DependencyState::Missing,
                    format!("Add one of {} to {}", names.join(", "), directory.display()),
                )
            }
        }
        DependencyCheck::AllNamedFiles { directory, names } => {
            let missing = if component.id == "arcade-shared-bios" {
                get_missing_arcade_shared_bios_files(&paths.rom_root, Some(&paths.bios_root))
            } else {
                names
                    .iter()
                    .filter(|name| !directory.join(name.as_str()).is_file())
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if missing.is_empty() {
                (DependencyState::Ready, "Found required files.".to_string())
            } else {
                (
                    DependencyState::Missing,
                    format!("Missing {} in {}", missing.join(", "), directory.display()),
                )
            }
        }
        DependencyCheck::DirectoryLooksLikeDolphinSys => {
            if let Some(path) = find_dolphin_sys_directory(&paths.rom_root, Some(&paths.bios_root))
            {
                (
                    DependencyState::Ready,
                    format!("Found Dolphin Sys at {}", path.display()),
                )
            } else {
                (
                    DependencyState::Missing,
                    format!(
                        "Place Dolphin's Sys folder at {}",
                        component.target_path.display()
                    ),
                )
            }
        }
        DependencyCheck::AnyFileInDirectory(directory) => {
            let found = directory
                .read_dir()
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| entry.path().is_file());
            if found {
                (
                    DependencyState::Ready,
                    format!("Found files in {}", directory.display()),
                )
            } else {
                (
                    DependencyState::Missing,
                    format!("Add your files to {}", directory.display()),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn manifest_keeps_buildbot_cores_and_compatibility_cores_separate() {
        let temp = tempdir().expect("tempdir");
        let manifest = dependency_manifest(&paths(temp.path()));

        assert!(manifest.iter().any(|component| matches!(
            component.source,
            DependencySource::LibretroBuildbot { .. }
        )));
        assert!(manifest
            .iter()
            .any(|component| matches!(component.kind, DependencyComponentKind::CompatCore)));
    }

    #[test]
    fn scan_reports_missing_required_dependencies() {
        let temp = tempdir().expect("tempdir");
        let report = scan_dependency_report(&paths(temp.path()));

        assert!(report.has_missing_required());
        assert!(report
            .components
            .iter()
            .any(|status| status.component.id == "ps2-bios" && !status.ready()));
    }
}
