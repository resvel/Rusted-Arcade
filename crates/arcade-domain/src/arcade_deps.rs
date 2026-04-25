use std::fs;
use std::path::{Path, PathBuf};

use crate::resolve_path_from_root;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArcadeCompatibilityStatus {
    Ready,
    BlockedMissingDependency,
}

#[derive(Debug, Clone)]
pub struct ArcadeCompatibility {
    pub compatibility_status: ArcadeCompatibilityStatus,
    pub compatibility_reason: Option<String>,
    pub blocked_cps3_title: bool,
    pub missing_shared_bios_files: Vec<String>,
    pub missing_merged_entries: Vec<String>,
    pub missing_set_entries: Vec<String>,
}

pub const ARCADE_SHARED_BIOS_FILES: &[&str] = &["neogeo.zip", "qsound.zip", "pgm.zip"];
pub const ARCADE_CPS3_BLOCKED_TITLES: &[&str] = &["sfiii", "sfiii2", "sfiii3", "jojo", "redearth"];
pub const PCECD_ACCEPTED_BIOS_FILES: &[&str] = &[
    "syscard3.pce",
    "syscard2.pce",
    "syscard1.pce",
    "gexpress.pce",
];
pub const SATURN_ACCEPTED_BIOS_FILES: &[&str] = &["sega_101.bin", "mpr-17933.bin"];

const NEOGEO_BIOS_REQUIRED_ENTRIES: &[&str] = &["sp-s3.sp1", "sm1.sm1", "sfix.sfix", "000-lo.lo"];

fn merge_required_entries_by_title(title: &str) -> Option<Vec<&'static str>> {
    match title {
        "fatfury1" => Some(vec!["033-p1.p1", "033-c1.c1"]),
        "fatfury2" => Some(vec!["047-p1.p1", "047-c1.c1"]),
        "fatfury3" => Some(vec!["069-p1.p1", "069-c1.c1"]),
        "fatfursp" => Some(vec!["058-p1.p1", "058-c1.c1"]),
        "garou" => Some(vec!["253-ep1.p1", "253-c1.c1"]),
        "sf2ce" => Some(vec![
            "s92_21a.6f",
            "s92_22b.7f",
            "s92e_23b.8f",
            "sf2_26.bin",
        ]),
        "sf2hf" => Some(vec!["s2te_21.6f", "s2te_22.7f", "s2te_23.8f", "sf2_26.bin"]),
        "kof94" => Some(vec!["055-p1.p1", "055-c1.c1"]),
        "kof95" => Some(vec!["084-p1.p1", "084-c1.c1"]),
        "kof96" => Some(vec!["214-p1.p1", "214-c1.c1"]),
        "kof98" => Some(vec!["242-p1.p1", "242-c1.c1"]),
        "kof99" => Some(vec!["251-p1.p1", "251-c1.c1"]),
        "kof2000" => Some(vec!["257-p1.p1", "257-c1.c1"]),
        "kof2001" => Some(vec!["262-p1-08-e0.p1", "262-c1-08-e0.c1"]),
        "kof2003" => Some(vec!["271-p1c.p1", "271-c1c.c1"]),
        "lastblad" => Some(vec!["234-p1.p1", "234-c1.c1"]),
        "lastbld2" => Some(vec!["243-pg1.p1", "243-c1.c1"]),
        "samsho" => Some(vec!["045-p1.p1", "045-c1.c1"]),
        "samsho2" => Some(vec!["063-p1.p1", "063-c1.c1"]),
        "samsho3" => Some(vec!["087-p5.p5", "087-c1.c1"]),
        "samsho4" => Some(vec!["222-p1.p1", "222-c1.c1"]),
        "samsho5" => Some(vec!["270-p1.p1", "270-c1.c1"]),
        "mslug" => Some(vec!["201-p1.bin", "201-s1.bin"]),
        "mslug2" => Some(vec!["241-p1.p1", "241-s1.s1"]),
        "mslug3" => Some(vec!["256-pg1.p1", "256-c1.c1"]),
        "mslug4" => Some(vec!["263-p1.p1", "263-c1.c1"]),
        "mslug5" => Some(vec!["268-p1cr.p1", "268-c1c.c1"]),
        "mslugx" => Some(vec![
            "250-p1.bin",
            "250-p2.bin",
            "250-s1.bin",
            "250-m1.bin",
            "201-p1.bin",
        ]),
        _ => None,
    }
    .map(|mut entries| {
        entries.extend(NEOGEO_BIOS_REQUIRED_ENTRIES.iter().copied());
        entries
    })
}

fn set_validation_required_entries(title: &str, core: Option<&str>) -> Option<Vec<&'static str>> {
    match (
        title,
        core.unwrap_or("").trim().to_ascii_lowercase().as_str(),
    ) {
        ("galaga", "mame2003") | ("galaga", "mame2003_plus") => Some(vec![
            "04m_g01.bin",
            "04k_g02.bin",
            "04j_g03.bin",
            "04h_g04.bin",
            "04e_g05.bin",
            "04d_g06.bin",
            "07m_g08.bin",
            "07h_g09.bin",
            "07e_g10.bin",
            "5n.bin",
            "2n.bin",
            "1c.bin",
            "5c.bin",
            "1d.bin",
        ]),
        ("galaga", _) => Some(vec!["prom-4.2n", "prom-3.1c", "prom-1.1d", "prom-2.5c"]),
        _ => None,
    }
}

fn normalize_title(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}

pub fn get_arcade_bios_directory(rom_root: &Path, bios_root: Option<&Path>) -> PathBuf {
    candidate_arcade_bios_directories(rom_root, bios_root)
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| rom_root.join("arcade-mame2003"))
}

pub fn get_missing_arcade_shared_bios_files(
    rom_root: &Path,
    bios_root: Option<&Path>,
) -> Vec<String> {
    ARCADE_SHARED_BIOS_FILES
        .iter()
        .filter_map(|name| {
            if candidate_arcade_bios_directories(rom_root, bios_root)
                .iter()
                .any(|bios_dir| bios_dir.join(name).exists())
            {
                None
            } else {
                Some((*name).to_string())
            }
        })
        .collect()
}

fn candidate_arcade_bios_directories(rom_root: &Path, bios_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_unique = |path: PathBuf| {
        if !candidates.iter().any(|existing| existing == &path) {
            candidates.push(path);
        }
    };

    if let Some(bios_root) = bios_root {
        push_unique(bios_root.join("arcade-mame2003"));
        push_unique(bios_root.to_path_buf());
        push_unique(bios_root.join("roms").join("arcade-mame2003"));
    }
    push_unique(rom_root.join("arcade-mame2003"));
    push_unique(rom_root.join("roms").join("arcade-mame2003"));
    candidates
}

pub fn get_pcecd_bios_directory(rom_root: &Path, bios_root: Option<&Path>) -> PathBuf {
    candidate_pcecd_bios_directories(rom_root, bios_root)
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| rom_root.join("pcecd"))
}

pub fn get_saturn_bios_directory(rom_root: &Path, bios_root: Option<&Path>) -> PathBuf {
    candidate_saturn_bios_directories(rom_root, bios_root)
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| rom_root.join("saturn"))
}

pub fn find_pcecd_bios_file(rom_root: &Path, bios_root: Option<&Path>) -> Option<PathBuf> {
    let accepted = PCECD_ACCEPTED_BIOS_FILES
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    for dir in candidate_pcecd_bios_directories(rom_root, bios_root) {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if accepted.contains(&file_name.to_ascii_lowercase()) {
                return Some(path);
            }
        }
    }

    None
}

pub fn find_saturn_bios_file(rom_root: &Path, bios_root: Option<&Path>) -> Option<PathBuf> {
    let accepted = SATURN_ACCEPTED_BIOS_FILES
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    for dir in candidate_saturn_bios_directories(rom_root, bios_root) {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if accepted.contains(&file_name.to_ascii_lowercase()) {
                return Some(path);
            }
        }
    }

    None
}

fn candidate_pcecd_bios_directories(rom_root: &Path, bios_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_unique = |path: PathBuf| {
        if !candidates.iter().any(|existing| existing == &path) {
            candidates.push(path);
        }
    };

    if let Some(bios_root) = bios_root {
        push_unique(bios_root.join("pcecd"));
        push_unique(bios_root.to_path_buf());
        push_unique(bios_root.join("roms").join("pcecd"));
    }
    push_unique(rom_root.join("pcecd"));
    push_unique(rom_root.join("roms").join("pcecd"));
    candidates
}

fn candidate_saturn_bios_directories(rom_root: &Path, bios_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_unique = |path: PathBuf| {
        if !candidates.iter().any(|existing| existing == &path) {
            candidates.push(path);
        }
    };

    if let Some(bios_root) = bios_root {
        push_unique(bios_root.join("saturn"));
        push_unique(bios_root.to_path_buf());
        push_unique(bios_root.join("roms").join("saturn"));
    }
    push_unique(rom_root.join("saturn"));
    push_unique(rom_root.join("roms").join("saturn"));
    candidates
}

pub fn is_arcade_cps3_blocked_title(title: Option<&str>) -> bool {
    let title = normalize_title(title);
    ARCADE_CPS3_BLOCKED_TITLES.contains(&title.as_str())
}

fn resolve_archive_path(file_path: Option<&str>, rom_root: &Path) -> Option<PathBuf> {
    let value = file_path?.trim();
    if value.is_empty() {
        return None;
    }

    Some(resolve_path_from_root(value, rom_root))
}

fn read_zip_content_for_presence_check(archive_path: &Path) -> String {
    fs::read(archive_path)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .unwrap_or_default()
}

fn get_missing_entries(required_entries: &[&str], archive_path: Option<&Path>) -> Vec<String> {
    if required_entries.is_empty() {
        return Vec::new();
    }

    let Some(path) = archive_path else {
        return required_entries.iter().map(|s| (*s).to_string()).collect();
    };

    if !path.exists() {
        return required_entries.iter().map(|s| (*s).to_string()).collect();
    }

    let zip_text = read_zip_content_for_presence_check(path);
    required_entries
        .iter()
        .filter_map(|entry| {
            if zip_text.contains(entry) {
                None
            } else {
                Some((*entry).to_string())
            }
        })
        .collect()
}

pub fn get_arcade_compatibility(
    system: Option<&str>,
    title: Option<&str>,
    core: Option<&str>,
    file_path: Option<&str>,
    rom_root: &Path,
    bios_root: &Path,
) -> ArcadeCompatibility {
    let is_arcade = system.unwrap_or("").eq_ignore_ascii_case("ARCADE");
    if !is_arcade {
        return ArcadeCompatibility {
            compatibility_status: ArcadeCompatibilityStatus::Ready,
            compatibility_reason: None,
            blocked_cps3_title: false,
            missing_shared_bios_files: Vec::new(),
            missing_merged_entries: Vec::new(),
            missing_set_entries: Vec::new(),
        };
    }

    if is_arcade_cps3_blocked_title(title) {
        return ArcadeCompatibility {
            compatibility_status: ArcadeCompatibilityStatus::BlockedMissingDependency,
            compatibility_reason: Some(String::from(
                "CPS3 titles are temporarily blocked until valid CPS3 BIOS/CHD assets are installed.",
            )),
            blocked_cps3_title: true,
            missing_shared_bios_files: Vec::new(),
            missing_merged_entries: Vec::new(),
            missing_set_entries: Vec::new(),
        };
    }

    let missing_shared = get_missing_arcade_shared_bios_files(rom_root, Some(bios_root));
    if !missing_shared.is_empty() {
        let bios_dir = get_arcade_bios_directory(rom_root, Some(bios_root));
        return ArcadeCompatibility {
            compatibility_status: ArcadeCompatibilityStatus::BlockedMissingDependency,
            compatibility_reason: Some(format!(
                "Missing shared arcade BIOS files in {}: {}",
                bios_dir.display(),
                missing_shared.join(", ")
            )),
            blocked_cps3_title: false,
            missing_shared_bios_files: missing_shared,
            missing_merged_entries: Vec::new(),
            missing_set_entries: Vec::new(),
        };
    }

    let normalized_title = normalize_title(title);
    let archive_path = resolve_archive_path(file_path, rom_root);

    if let Some(required_merged) = merge_required_entries_by_title(&normalized_title) {
        let missing_merged = get_missing_entries(&required_merged, archive_path.as_deref());
        if !missing_merged.is_empty() {
            let archive_name = archive_path
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| String::from("archive"));
            return ArcadeCompatibility {
                compatibility_status: ArcadeCompatibilityStatus::BlockedMissingDependency,
                compatibility_reason: Some(format!(
                    "Arcade set {} needs merged parent/BIOS content. Missing entries in {}: {}",
                    title.unwrap_or(""),
                    archive_name,
                    missing_merged.join(", ")
                )),
                blocked_cps3_title: false,
                missing_shared_bios_files: Vec::new(),
                missing_merged_entries: missing_merged,
                missing_set_entries: Vec::new(),
            };
        }
    }

    if let Some(required_set) = set_validation_required_entries(&normalized_title, core) {
        let missing_set = get_missing_entries(&required_set, archive_path.as_deref());
        if !missing_set.is_empty() {
            let archive_name = archive_path
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| String::from("archive"));
            let core_label = core.unwrap_or("selected core");
            return ArcadeCompatibility {
                compatibility_status: ArcadeCompatibilityStatus::BlockedMissingDependency,
                compatibility_reason: Some(format!(
                    "Arcade set {} is incomplete/incompatible for {}. Missing required entries in {}: {}",
                    title.unwrap_or(""), core_label,
                    archive_name,
                    missing_set.join(", ")
                )),
                blocked_cps3_title: false,
                missing_shared_bios_files: Vec::new(),
                missing_merged_entries: Vec::new(),
                missing_set_entries: missing_set,
            };
        }
    }

    ArcadeCompatibility {
        compatibility_status: ArcadeCompatibilityStatus::Ready,
        compatibility_reason: None,
        blocked_cps3_title: false,
        missing_shared_bios_files: Vec::new(),
        missing_merged_entries: Vec::new(),
        missing_set_entries: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cps3_titles_are_blocked() {
        let compatibility = get_arcade_compatibility(
            Some("ARCADE"),
            Some("sfiii3"),
            None,
            None,
            Path::new("/tmp"),
            Path::new("/tmp"),
        );
        assert_eq!(
            compatibility.compatibility_status,
            ArcadeCompatibilityStatus::BlockedMissingDependency
        );
        assert!(compatibility.blocked_cps3_title);
    }

    #[test]
    fn arcade_bios_lookup_supports_workspace_root_layout() {
        let dir = tempdir().unwrap();
        let bios_dir = dir.path().join("roms").join("arcade-mame2003");
        fs::create_dir_all(&bios_dir).unwrap();
        fs::write(bios_dir.join("neogeo.zip"), b"bios").unwrap();
        fs::write(bios_dir.join("qsound.zip"), b"bios").unwrap();
        fs::write(bios_dir.join("pgm.zip"), b"bios").unwrap();

        assert!(get_missing_arcade_shared_bios_files(dir.path(), None).is_empty());
        assert_eq!(get_arcade_bios_directory(dir.path(), None), bios_dir);
    }

    #[test]
    fn arcade_bios_lookup_supports_separate_bios_root_layout() {
        let dir = tempdir().unwrap();
        let rom_root = dir.path().join("roms");
        let bios_root = dir.path().join("bios");
        fs::create_dir_all(&rom_root).unwrap();
        fs::create_dir_all(&bios_root).unwrap();
        fs::write(bios_root.join("neogeo.zip"), b"bios").unwrap();
        fs::write(bios_root.join("qsound.zip"), b"bios").unwrap();
        fs::write(bios_root.join("pgm.zip"), b"bios").unwrap();

        assert!(get_missing_arcade_shared_bios_files(&rom_root, Some(&bios_root)).is_empty());
        assert_eq!(
            get_arcade_bios_directory(&rom_root, Some(&bios_root)),
            bios_root
        );
    }

    #[test]
    fn galaga_mame2003_romset_mismatch_is_blocked_before_launch() {
        let dir = tempdir().unwrap();
        let bios_dir = dir.path().join("roms").join("arcade-mame2003");
        fs::create_dir_all(&bios_dir).unwrap();
        fs::write(bios_dir.join("neogeo.zip"), b"bios").unwrap();
        fs::write(bios_dir.join("qsound.zip"), b"bios").unwrap();
        fs::write(bios_dir.join("pgm.zip"), b"bios").unwrap();

        let archive_path = bios_dir.join("galaga.zip");
        fs::write(
            &archive_path,
            b"gg1_10.4f gg1_11.4d gg1_1b.3p gg1_2b.3m prom-1.1d prom-2.5c prom-3.1c prom-4.2n",
        )
        .unwrap();

        let compatibility = get_arcade_compatibility(
            Some("ARCADE"),
            Some("galaga"),
            Some("mame2003"),
            Some("roms/arcade-mame2003/galaga.zip"),
            dir.path(),
            dir.path(),
        );

        assert_eq!(
            compatibility.compatibility_status,
            ArcadeCompatibilityStatus::BlockedMissingDependency
        );
        assert!(compatibility
            .compatibility_reason
            .unwrap_or_default()
            .contains("mame2003"));
        assert!(compatibility
            .missing_set_entries
            .iter()
            .any(|entry| entry == "04m_g01.bin"));
    }

    #[test]
    fn pcecd_bios_lookup_supports_case_insensitive_filenames() {
        let dir = tempdir().unwrap();
        let rom_root = dir.path().join("roms");
        let bios_root = dir.path().join("bios");
        fs::create_dir_all(rom_root.join("pcecd")).unwrap();
        fs::create_dir_all(bios_root.join("pcecd")).unwrap();
        fs::write(bios_root.join("pcecd").join("SYSCARD3.PCE"), b"bios").unwrap();

        let found = find_pcecd_bios_file(&rom_root, Some(&bios_root));
        assert!(found.is_some());
        assert_eq!(
            found.unwrap().file_name().and_then(|f| f.to_str()),
            Some("SYSCARD3.PCE")
        );
    }

    #[test]
    fn pcecd_bios_lookup_prefers_existing_candidate_directory() {
        let dir = tempdir().unwrap();
        let rom_root = dir.path().join("roms");
        let bios_root = dir.path().join("bios");
        fs::create_dir_all(rom_root.join("pcecd")).unwrap();
        fs::create_dir_all(&bios_root).unwrap();

        let preferred = get_pcecd_bios_directory(&rom_root, Some(&bios_root));
        assert_eq!(preferred, bios_root);
    }

    #[test]
    fn saturn_bios_lookup_supports_case_insensitive_filenames() {
        let dir = tempdir().unwrap();
        let rom_root = dir.path().join("roms");
        let bios_root = dir.path().join("bios");
        fs::create_dir_all(rom_root.join("saturn")).unwrap();
        fs::create_dir_all(bios_root.join("saturn")).unwrap();
        fs::write(bios_root.join("saturn").join("MPR-17933.BIN"), b"bios").unwrap();

        let found = find_saturn_bios_file(&rom_root, Some(&bios_root));
        assert!(found.is_some());
        assert_eq!(
            found.unwrap().file_name().and_then(|f| f.to_str()),
            Some("MPR-17933.BIN")
        );
    }

    #[test]
    fn saturn_bios_lookup_prefers_existing_candidate_directory() {
        let dir = tempdir().unwrap();
        let rom_root = dir.path().join("roms");
        let bios_root = dir.path().join("bios");
        fs::create_dir_all(rom_root.join("saturn")).unwrap();
        fs::create_dir_all(&bios_root).unwrap();

        let preferred = get_saturn_bios_directory(&rom_root, Some(&bios_root));
        assert_eq!(preferred, bios_root);
    }
}
