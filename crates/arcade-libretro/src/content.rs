use super::*;

const ARCADE_SHARED_ARCHIVES: &[&str] = &["neogeo.zip", "qsound.zip", "pgm.zip"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadGameStrategy {
    PathOnly,
    PathAndData,
    DataOnly,
}

pub(super) struct PreparedGameContent {
    pub(super) path: CString,
    pub(super) data: Option<Vec<u8>>,
    pub(super) strategies: Vec<LoadGameStrategy>,
}

pub(super) struct LaunchSession {
    pub(super) launch_rom_path: PathBuf,
    pub(super) _temp_dir: TempDir,
}

pub(super) struct CoreRequirements {
    pub(super) need_fullpath: bool,
    pub(super) block_extract: bool,
    pub(super) valid_extensions: Vec<String>,
    pub(super) requires_hw_render: bool,
}

pub(super) fn prepare_launch_session(
    system: &str,
    core_name: &str,
    rom_path: &Path,
    bios_root: &Path,
) -> Result<Option<LaunchSession>> {
    prepare_launch_session_with_mode(
        system,
        core_name,
        rom_path,
        bios_root,
        should_stage_arcade_archives(),
    )
}

fn should_stage_arcade_archives() -> bool {
    std::env::var_os("ARCADE_STAGE_LAUNCH_ARCHIVES").is_some()
}

fn prepare_launch_session_with_mode(
    system: &str,
    core_name: &str,
    rom_path: &Path,
    bios_root: &Path,
    stage_arcade_archives: bool,
) -> Result<Option<LaunchSession>> {
    let extension = rom_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !system.eq_ignore_ascii_case("ARCADE")
        || !matches!(core_name, "fbneo" | "mame2003" | "mame2003_plus")
        || !extension.eq_ignore_ascii_case("zip")
    {
        return Ok(None);
    }
    if !stage_arcade_archives {
        return Ok(None);
    }

    let Some(file_name) = rom_path.file_name() else {
        return Ok(None);
    };
    let parent_dir = rom_path
        .parent()
        .ok_or_else(|| anyhow!("rom path has no parent directory: {}", rom_path.display()))?;

    let temp_dir = tempfile::Builder::new()
        .prefix("arcade-launch-")
        .tempdir()
        .context("failed to create arcade launch staging directory")?;
    let launch_rom_path = temp_dir.path().join(file_name);
    stage_launch_archive(rom_path, &launch_rom_path)?;

    if let Some(parent_archive) = resolve_arcade_parent_archive(rom_path) {
        let parent_path = parent_dir.join(parent_archive);
        if parent_path.exists() {
            stage_launch_archive(&parent_path, &temp_dir.path().join(parent_archive))?;
        }
    }

    for archive_name in ARCADE_SHARED_ARCHIVES {
        if let Some(archive_path) =
            resolve_arcade_shared_archive_source(parent_dir, bios_root, archive_name)
        {
            stage_launch_archive(&archive_path, &temp_dir.path().join(archive_name))?;
        }
    }

    Ok(Some(LaunchSession {
        launch_rom_path,
        _temp_dir: temp_dir,
    }))
}

fn resolve_arcade_parent_archive(rom_path: &Path) -> Option<&'static str> {
    let stem = rom_path.file_stem()?.to_str()?.trim().to_ascii_lowercase();
    match stem.as_str() {
        "mslugx" => Some("mslug.zip"),
        _ => None,
    }
}

fn resolve_arcade_shared_archive_source(
    rom_parent_dir: &Path,
    bios_root: &Path,
    archive_name: &str,
) -> Option<PathBuf> {
    let candidates = [
        rom_parent_dir.join(archive_name),
        bios_root.join(archive_name),
        bios_root.join("arcade-mame2003").join(archive_name),
        bios_root
            .join("roms")
            .join("arcade-mame2003")
            .join(archive_name),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn stage_launch_archive(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        return Ok(());
    }

    fs::copy(source, destination)
        .with_context(|| {
            format!(
                "failed to stage arcade archive {} -> {}",
                source.display(),
                destination.display()
            )
        })
        .map(|_| ())
}

pub(super) fn inspect_core_requirements(core_path: &Path) -> Result<CoreRequirements> {
    let library = unsafe { Library::new(core_path) }
        .with_context(|| format!("failed to load core {}", core_path.display()))?;
    let api = unsafe { load_api(&library)? };
    let mut system_info = RetroSystemInfo {
        library_name: std::ptr::null(),
        library_version: std::ptr::null(),
        valid_extensions: std::ptr::null(),
        need_fullpath: true,
        block_extract: false,
    };
    unsafe {
        (api.get_system_info)(&mut system_info as *mut RetroSystemInfo);
    }

    Ok(CoreRequirements {
        need_fullpath: system_info.need_fullpath,
        block_extract: system_info.block_extract,
        valid_extensions: supported_extensions_from_ptr(system_info.valid_extensions),
        requires_hw_render: core_requires_hw_render(core_path),
    })
}

pub(super) fn build_game_info(
    strategy: LoadGameStrategy,
    rom_c: &CString,
    rom_data: Option<&[u8]>,
) -> RetroGameInfo {
    let (path, data, size) = match strategy {
        LoadGameStrategy::PathOnly => (rom_c.as_ptr(), std::ptr::null(), 0),
        LoadGameStrategy::PathAndData => (
            rom_c.as_ptr(),
            rom_data
                .map(|bytes| bytes.as_ptr() as *const c_void)
                .unwrap_or(std::ptr::null()),
            rom_data.map(|bytes| bytes.len()).unwrap_or(0),
        ),
        LoadGameStrategy::DataOnly => (
            std::ptr::null(),
            rom_data
                .map(|bytes| bytes.as_ptr() as *const c_void)
                .unwrap_or(std::ptr::null()),
            rom_data.map(|bytes| bytes.len()).unwrap_or(0),
        ),
    };

    RetroGameInfo {
        path,
        data,
        size,
        meta: std::ptr::null(),
    }
}

fn load_strategies(need_fullpath: bool) -> &'static [LoadGameStrategy] {
    if need_fullpath {
        &[LoadGameStrategy::PathOnly]
    } else {
        &[
            LoadGameStrategy::PathOnly,
            LoadGameStrategy::PathAndData,
            LoadGameStrategy::DataOnly,
        ]
    }
}

pub(super) fn strategy_uses_data(strategy: LoadGameStrategy) -> bool {
    matches!(
        strategy,
        LoadGameStrategy::PathAndData | LoadGameStrategy::DataOnly
    )
}

fn core_requires_hw_render(core_path: &Path) -> bool {
    let mut info_path = core_path.to_path_buf();
    info_path.set_extension("info");
    fs::read_to_string(&info_path)
        .map(|contents| {
            contents.lines().any(|line| {
                let normalized = line.trim();
                normalized == "hw_render = \"true\"" || normalized.starts_with("required_hw_api = ")
            })
        })
        .unwrap_or(false)
}

pub(super) fn prepare_game_content(
    rom_path: &Path,
    requirements: &CoreRequirements,
) -> Result<PreparedGameContent> {
    let rom_path_cstring = CString::new(rom_path.to_string_lossy().as_bytes())
        .map_err(|_| anyhow!("rom path contains null bytes"))?;

    if should_extract_archive(rom_path, requirements) {
        let Some((entry_name, bytes)) =
            extract_supported_zip_entry(rom_path, &requirements.valid_extensions)?
        else {
            return Err(anyhow!(
                "archive does not contain a supported ROM for this core: {}",
                rom_path.display()
            ));
        };

        if requirements.need_fullpath {
            return Err(anyhow!(
                "core requires full-path loading for extracted archive content, which is not implemented yet: {}",
                rom_path.display()
            ));
        }

        return Ok(PreparedGameContent {
            path: entry_name,
            data: Some(bytes),
            strategies: vec![LoadGameStrategy::PathAndData, LoadGameStrategy::DataOnly],
        });
    }

    Ok(PreparedGameContent {
        path: rom_path_cstring,
        data: None,
        strategies: load_strategies(requirements.need_fullpath).to_vec(),
    })
}

fn supported_extensions_from_ptr(valid_extensions: *const c_char) -> Vec<String> {
    if valid_extensions.is_null() {
        return Vec::new();
    }

    unsafe { CStr::from_ptr(valid_extensions) }
        .to_string_lossy()
        .split('|')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn should_extract_archive(rom_path: &Path, requirements: &CoreRequirements) -> bool {
    matches!(
        rom_path.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("zip")
    ) && !requirements.block_extract
        && !requirements.valid_extensions.iter().any(|ext| ext == "zip")
}

fn extract_supported_zip_entry(
    archive_path: &Path,
    supported_extensions: &[String],
) -> Result<Option<(CString, Vec<u8>)>> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read zip archive {}", archive_path.display()))?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if !entry.is_file() {
            continue;
        }

        let Some(entry_name) = Path::new(entry.name())
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };

        let Some(extension) = Path::new(&entry_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
        else {
            continue;
        };

        if !supported_extensions
            .iter()
            .any(|supported| supported == &extension)
        {
            continue;
        }

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let entry_name =
            CString::new(entry_name).map_err(|_| anyhow!("archive entry contains null bytes"))?;
        return Ok(Some((entry_name, bytes)));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn arcade_launch_session_stages_parent_and_shared_archives() {
        let dir = tempdir().expect("tempdir");
        let rom_dir = dir.path().join("roms").join("arcade-mame2003");
        fs::create_dir_all(&rom_dir).expect("create rom dir");

        fs::write(rom_dir.join("mslugx.zip"), b"child").expect("write child");
        fs::write(rom_dir.join("mslug.zip"), b"parent").expect("write parent");
        fs::write(rom_dir.join("neogeo.zip"), b"bios").expect("write bios");

        let session = prepare_launch_session_with_mode(
            "ARCADE",
            "fbneo",
            &rom_dir.join("mslugx.zip"),
            &rom_dir,
            true,
        )
        .expect("stage launch session")
        .expect("launch session");

        assert_eq!(
            session
                .launch_rom_path
                .file_name()
                .and_then(|name| name.to_str()),
            Some("mslugx.zip")
        );
        assert!(session.launch_rom_path.exists());
        assert!(session
            .launch_rom_path
            .parent()
            .expect("launch dir")
            .join("mslug.zip")
            .exists());
        assert!(session
            .launch_rom_path
            .parent()
            .expect("launch dir")
            .join("neogeo.zip")
            .exists());
    }

    #[test]
    fn arcade_launch_session_stages_shared_archives_from_bios_root() {
        let dir = tempdir().expect("tempdir");
        let rom_dir = dir.path().join("roms").join("arcade-mame2003");
        let bios_dir = dir.path().join("bios");
        fs::create_dir_all(&rom_dir).expect("create rom dir");
        fs::create_dir_all(&bios_dir).expect("create bios dir");

        fs::write(rom_dir.join("mslugx.zip"), b"child").expect("write child");
        fs::write(rom_dir.join("mslug.zip"), b"parent").expect("write parent");
        fs::write(bios_dir.join("neogeo.zip"), b"bios").expect("write bios");

        let session = prepare_launch_session_with_mode(
            "ARCADE",
            "mame2003",
            &rom_dir.join("mslugx.zip"),
            &bios_dir,
            true,
        )
        .expect("stage launch session")
        .expect("launch session");

        assert!(session
            .launch_rom_path
            .parent()
            .expect("launch dir")
            .join("neogeo.zip")
            .exists());
    }

    #[test]
    fn arcade_launch_session_skips_staging_by_default() {
        let dir = tempdir().expect("tempdir");
        let rom_dir = dir.path().join("roms").join("arcade-mame2003");
        fs::create_dir_all(&rom_dir).expect("create rom dir");
        fs::write(rom_dir.join("mslug.zip"), b"child").expect("write child");

        let session = prepare_launch_session_with_mode(
            "ARCADE",
            "fbneo",
            &rom_dir.join("mslug.zip"),
            dir.path(),
            false,
        )
        .expect("prepare launch session");
        assert!(session.is_none());
    }
}
