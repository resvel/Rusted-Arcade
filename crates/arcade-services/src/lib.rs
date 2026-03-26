use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use arcade_data::{DataError, Database, ScanUpsertOutcome, ScannedRomInput};
use arcade_domain::{
    apply_device_preset_defaults, default_gamepad_mapping_for_system, get_arcade_compatibility,
    resolve_core, resolve_effective_core_override, resolve_path_from_root, AppConfig,
    CoverScrapePlatformIds, CoverScrapeRunOptions, CoverScrapeSettingsInput, CoverScrapingConfig,
    DetectedPadIdentity, ManageOperationKind, ManageOperationSummary, ManageProgressEvent,
    ManageRomStatus, ManageScope, ManagementConfig, PathsConfig, RomCard,
    RomQuery, SaveLimits, SaveSlotData, SaveSlotSummary,
    SavedGamepadMappingSummary, StoredGamepadMapping, SYSTEM_DEFAULT_MAPPING_KEY,
};
use sha1::{Digest, Sha1};
use tracing::warn;

const DEFAULT_SLOT_MAX_BYTES: i64 = 5 * 1024 * 1024;
const DEFAULT_PROFILE_MAX_BYTES: i64 = 25 * 1024 * 1024;

#[derive(Clone)]
pub struct NativeServices {
    config: Arc<Mutex<AppConfig>>,
    config_path: Arc<PathBuf>,
    db: Database,
    local_profile_id: Arc<str>,
}

#[derive(Debug)]
pub enum SaveOperationError {
    VersionConflict(SaveSlotSummary),
    SlotLimitExceeded,
    ProfileLimitExceeded,
    Other(anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub rom_id: String,
    pub display_title: String,
    pub system: String,
    pub rom_path: PathBuf,
    pub effective_core: Option<String>,
    pub resolved_core_name: String,
    pub status_message: String,
    pub active_core_note: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfigUpdateOutcome {
    pub restart_required: bool,
}

impl NativeServices {
    pub fn bootstrap(config: AppConfig, config_path: PathBuf, db: Database) -> Result<Self> {
        let local_profile_id = db.ensure_local_profile_id()?;
        Ok(Self {
            config: Arc::new(Mutex::new(config)),
            config_path: Arc::new(config_path),
            db,
            local_profile_id: Arc::<str>::from(local_profile_id),
        })
    }

    pub fn config(&self) -> Arc<AppConfig> {
        match self.config.lock() {
            Ok(config) => Arc::new(config.clone()),
            Err(poison) => Arc::new(poison.into_inner().clone()),
        }
    }

    pub fn list_roms(&self, query: &RomQuery) -> Result<Vec<RomCard>> {
        self.db.list_rom_cards(self.local_profile_id(), query)
    }

    pub fn list_favorites(&self) -> Result<Vec<RomCard>> {
        self.db.list_favorite_rom_cards(self.local_profile_id())
    }

    pub fn add_favorite(&self, rom_id: &str) -> Result<()> {
        self.db.add_favorite(self.local_profile_id(), rom_id)
    }

    pub fn remove_favorite(&self, rom_id: &str) -> Result<()> {
        self.db.remove_favorite(self.local_profile_id(), rom_id)
    }

    pub fn prepare_launch(&self, rom_id: &str) -> Result<LaunchPlan> {
        let config = self.current_config()?;
        let Some(rom_card) = self
            .db
            .get_rom_card_by_id(self.local_profile_id(), rom_id)?
        else {
            return Err(anyhow!("ROM not found."));
        };
        let rom = rom_card.rom;
        let rom_path = resolve_path_from_root(&rom.file_path, &config.paths.rom_root);
        if !rom_path.exists() {
            return Err(anyhow!("ROM file not found: {}", rom_path.display()));
        }
        let configured_core = config
            .preferred_core_for_system(&rom.system)
            .or(rom.emulator_core.as_deref());
        let effective_override =
            resolve_effective_core_override(&rom.system, configured_core, Some(&rom.title));
        let resolved_core_name = resolve_core(&rom.system, effective_override.as_deref());
        let active_core_note = native_arcade_core_note(&rom.system, Some(&resolved_core_name));

        let compatibility = get_arcade_compatibility(
            Some(&rom.system),
            Some(&rom.title),
            effective_override.as_deref(),
            Some(&rom.file_path),
            &config.paths.rom_root,
            &config.paths.bios_root,
        );

        if compatibility.compatibility_status != arcade_domain::ArcadeCompatibilityStatus::Ready {
            return Err(anyhow!(compatibility.compatibility_reason.unwrap_or_else(
                || String::from("Play blocked by compatibility policy.")
            )));
        }

        let display_title = rom_card.display_title;
        let status_message = if let Some(note) = active_core_note {
            format!(
                "Playing {} (core: {}) {}",
                display_title, resolved_core_name, note
            )
        } else {
            format!("Playing {} (core: {})", display_title, resolved_core_name)
        };

        Ok(LaunchPlan {
            rom_id: rom.id,
            display_title,
            system: rom.system,
            rom_path,
            effective_core: effective_override,
            resolved_core_name,
            status_message,
            active_core_note,
        })
    }

    pub fn save_limits(&self) -> SaveLimits {
        SaveLimits {
            slot_count: arcade_data::CLOUD_SLOT_COUNT,
            slot_max_bytes: DEFAULT_SLOT_MAX_BYTES,
            profile_max_bytes: DEFAULT_PROFILE_MAX_BYTES,
        }
    }

    pub fn load_save_slot(&self, rom_id: &str, slot: i32) -> Result<Option<SaveSlotData>> {
        self.db
            .load_save_slot(self.local_profile_id(), rom_id, slot)
    }

    pub fn save_save_slot(
        &self,
        rom_id: &str,
        slot: i32,
        label: Option<&str>,
        expected_version: Option<&str>,
        bytes: &[u8],
    ) -> std::result::Result<SaveSlotSummary, SaveOperationError> {
        match self.db.save_save_slot(
            self.local_profile_id(),
            rom_id,
            slot,
            label,
            expected_version,
            bytes,
            &self.save_limits(),
        ) {
            Ok(summary) => Ok(summary),
            Err(DataError::VersionConflict(conflict)) => {
                Err(SaveOperationError::VersionConflict(conflict.current))
            }
            Err(DataError::SlotLimitExceeded) => Err(SaveOperationError::SlotLimitExceeded),
            Err(DataError::ProfileLimitExceeded) => Err(SaveOperationError::ProfileLimitExceeded),
            Err(err) => Err(SaveOperationError::Other(anyhow!(err))),
        }
    }

    pub fn resolve_gamepad_mapping(
        &self,
        system: &str,
        device: Option<&DetectedPadIdentity>,
    ) -> Result<StoredGamepadMapping> {
        let system = normalize_system(system);

        if let Some(device) = device {
            if let Some(record) = self.db.load_gamepad_mapping(
                self.local_profile_id(),
                &system,
                &device.device_key,
            )? {
                match serde_json::from_str::<StoredGamepadMapping>(&record.mapping_json) {
                    Ok(mapping) => return Ok(mapping),
                    Err(err) => {
                        warn!(
                            system = %system,
                            mapping_key = %record.name,
                            "Ignoring invalid device-specific gamepad mapping: {err}"
                        );
                    }
                }
            }
        }

        let mut mapping = if let Some(record) = self.db.load_gamepad_mapping(
            self.local_profile_id(),
            &system,
            SYSTEM_DEFAULT_MAPPING_KEY,
        )? {
            match serde_json::from_str::<StoredGamepadMapping>(&record.mapping_json) {
                Ok(mapping) => mapping,
                Err(err) => {
                    warn!(
                        system = %system,
                        mapping_key = %record.name,
                        "Ignoring invalid system gamepad mapping: {err}"
                    );
                    default_gamepad_mapping_for_system(&system)
                }
            }
        } else {
            default_gamepad_mapping_for_system(&system)
        };

        if let Some(device) = device {
            apply_device_preset_defaults(&mut mapping, device);
        }

        Ok(mapping)
    }

    pub fn save_gamepad_mapping(
        &self,
        system: &str,
        mapping_key: &str,
        vendor_id: Option<&str>,
        product_id: Option<&str>,
        payload: &StoredGamepadMapping,
    ) -> Result<()> {
        let system = normalize_system(system);
        let mapping_json = serde_json::to_string(payload)?;
        self.db.upsert_gamepad_mapping(
            self.local_profile_id(),
            &system,
            mapping_key,
            vendor_id,
            product_id,
            &mapping_json,
        )?;
        Ok(())
    }

    pub fn list_gamepad_mappings(&self, system: &str) -> Result<Vec<SavedGamepadMappingSummary>> {
        let system = normalize_system(system);
        Ok(self
            .db
            .list_gamepad_mappings_for_system(self.local_profile_id(), &system)?
            .into_iter()
            .map(|record| SavedGamepadMappingSummary {
                id: record.id,
                system: record.system,
                name: record.name,
                vendor_id: record.vendor_id,
                product_id: record.product_id,
                updated_at: record.updated_at,
            })
            .collect())
    }

    pub fn update_cover_scrape_settings(&self, input: CoverScrapeSettingsInput) -> Result<()> {
        if input.default_limit == 0 {
            return Err(anyhow!("Default limit must be greater than zero."));
        }

        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;
        config.management = ManagementConfig::default();
        config.management.cover_scraping.tgdb_api_key = sanitize_api_key(input.tgdb_api_key);
        config.management.cover_scraping.default_limit = input.default_limit;
        config.management.cover_scraping.default_delay_ms = input.default_delay_ms;
        config.management.cover_scraping.platform_ids = CoverScrapePlatformIds {
            nes: input.nes_platform_ids,
            snes: input.snes_platform_ids,
            genesis: input.genesis_platform_ids,
            gb: input.gb_platform_ids,
            gba: input.gba_platform_ids,
            n64: input.n64_platform_ids,
            arcade: input.arcade_platform_ids,
        };
        config.save_to_path(self.config_path.as_ref())?;
        Ok(())
    }

    pub fn update_app_config_settings(
        &self,
        paths: PathsConfig,
        core_settings: HashMap<String, HashMap<String, String>>,
    ) -> Result<AppConfigUpdateOutcome> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;

        let restart_required = config.paths != paths;
        let mut saved_config = config.clone();
        saved_config.paths = paths;
        saved_config.emulation.core_settings = core_settings;

        fs::create_dir_all(&saved_config.paths.rom_root)?;
        saved_config.ensure_dirs()?;
        saved_config.save_to_path(self.config_path.as_ref())?;

        config.emulation = saved_config.emulation;
        if !restart_required {
            config.paths = saved_config.paths;
        }

        Ok(AppConfigUpdateOutcome { restart_required })
    }

    pub fn list_manage_roms(&self, scope: &ManageScope) -> Result<Vec<ManageRomStatus>> {
        let config = self.current_config()?;
        let mut items = self
            .list_all_roms(scope)?
            .into_iter()
            .map(|rom| {
                let path = resolve_path_from_root(&rom.rom.file_path, &config.paths.rom_root);
                ManageRomStatus {
                    rom_id: rom.rom.id,
                    title: rom.display_title,
                    system: rom.rom.system,
                    managed_path: rom.rom.file_path,
                    present_on_disk: path.exists(),
                    has_cover: rom
                        .rom
                        .cover_path
                        .as_deref()
                        .map(|value| !value.trim().is_empty())
                        .unwrap_or(false),
                }
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.system.cmp(&right.system).then_with(|| {
                left.title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase())
            })
        });
        Ok(items)
    }

    pub fn smart_scan_roms(
        &self,
        scope: &ManageScope,
        mut progress: impl FnMut(ManageProgressEvent),
    ) -> Result<ManageOperationSummary> {
        let config = self.current_config()?;
        let public_root = resolve_public_root(&config.paths.rom_root)?;
        let existing_roms = self.list_all_roms(&ManageScope::AllSystems)?;
        let mut slug_set = existing_roms
            .iter()
            .map(|rom| rom.rom.slug.clone())
            .collect::<HashSet<_>>();
        let mut path_to_slug = existing_roms
            .iter()
            .map(|rom| (rom.rom.file_path.clone(), rom.rom.slug.clone()))
            .collect::<HashMap<_, _>>();

        let scan_targets = scan_targets(scope);
        let mut files = Vec::new();
        for target in &scan_targets {
            let dir = resolve_system_directory(&config.paths.rom_root, target.folder)?;
            fs::create_dir_all(&dir)?;
            collect_files(&dir, &mut files)?;
        }

        let total = files.len();
        let mut summary = ManageOperationSummary {
            kind: Some(ManageOperationKind::SmartScan),
            ..ManageOperationSummary::default()
        };
        let arcade_cover_index = build_arcade_cover_index(&public_root)?;

        for (index, path) in files.iter().enumerate() {
            let Some(target) = target_for_path(path, &scan_targets) else {
                summary.skipped += 1;
                continue;
            };

            let ext = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
                .unwrap_or_default();
            if !target.extensions.iter().any(|candidate| *candidate == ext) {
                summary.skipped += 1;
                continue;
            }

            let metadata = fs::metadata(path)?;
            if let Some(max_bytes) = target.max_bytes {
                if metadata.len() > max_bytes {
                    summary.skipped += 1;
                    continue;
                }
            }

            let stored_file_path = to_stored_file_path(path, &config.paths.rom_root);
            let absolute_file_path = path.to_string_lossy().to_string();
            let existing_slug = path_to_slug.get(&stored_file_path).cloned();
            let title = title_from_path(path);
            let slug = unique_slug_for_scan(existing_slug, &title, &mut slug_set);
            path_to_slug.insert(stored_file_path.clone(), slug.clone());
            let checksum = hash_file(path)?;
            let cover_path = if target.system == "ARCADE" && target.emulator_core == "mame2003" {
                resolve_local_arcade_cover_path(&arcade_cover_index, &slug, &stored_file_path)
            } else {
                None
            };

            match self.db.upsert_scanned_rom(ScannedRomInput {
                system: target.system,
                emulator_core: target.emulator_core,
                slug: &slug,
                title: &title,
                stored_file_path: &stored_file_path,
                absolute_file_path: Some(&absolute_file_path),
                file_size: metadata.len() as i64,
                checksum: Some(&checksum),
                cover_path: cover_path.as_deref(),
            })? {
                ScanUpsertOutcome::Created => summary.created += 1,
                ScanUpsertOutcome::Updated => summary.updated += 1,
                ScanUpsertOutcome::Unchanged => summary.unchanged += 1,
            }

            progress(ManageProgressEvent {
                kind: ManageOperationKind::SmartScan,
                processed: index + 1,
                total: Some(total),
                message: format!("Scanned {}", path.display()),
            });
        }

        summary.message = format!(
            "Smart scan complete: {} created, {} updated, {} unchanged, {} skipped.",
            summary.created, summary.updated, summary.unchanged, summary.skipped
        );
        Ok(summary)
    }

    pub fn remove_roms_from_library(
        &self,
        rom_ids: &[String],
        mut progress: impl FnMut(ManageProgressEvent),
    ) -> Result<ManageOperationSummary> {
        let removed = self.db.delete_roms(rom_ids)?;
        progress(ManageProgressEvent {
            kind: ManageOperationKind::RemoveFromLibrary,
            processed: removed,
            total: Some(rom_ids.len()),
            message: format!("Removed {removed} ROMs from the library."),
        });
        Ok(ManageOperationSummary {
            kind: Some(ManageOperationKind::RemoveFromLibrary),
            removed,
            message: format!("Removed {removed} ROMs from the library."),
            ..ManageOperationSummary::default()
        })
    }

    pub fn scrape_covers(
        &self,
        run: CoverScrapeRunOptions,
        mut progress: impl FnMut(ManageProgressEvent),
    ) -> Result<ManageOperationSummary> {
        let config = self.current_config()?;
        let scrape_config = &config.management.cover_scraping;
        let api_key = scrape_config
            .tgdb_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("TheGamesDB API key is not configured."))?;
        if run.limit == 0 {
            return Err(anyhow!("Scrape limit must be greater than zero."));
        }

        let public_root = resolve_public_root(&config.paths.rom_root)?;
        let allowed_systems = run
            .systems
            .iter()
            .map(|system| normalize_system(system))
            .collect::<HashSet<_>>();
        let all_roms = self.list_all_roms(&ManageScope::AllSystems)?;
        let mut candidates = all_roms
            .into_iter()
            .filter(|rom| allowed_systems.contains(&normalize_system(&rom.rom.system)))
            .filter(|rom| {
                if run.missing_only {
                    rom.rom
                        .cover_path
                        .as_deref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.display_title.cmp(&right.display_title));
        if candidates.len() > run.limit {
            candidates.truncate(run.limit);
        }

        let total = candidates.len();
        let mut summary = ManageOperationSummary {
            kind: Some(ManageOperationKind::ScrapeMissingCovers),
            ..ManageOperationSummary::default()
        };

        for (index, rom) in candidates.iter().enumerate() {
            match scrape_cover_for_rom(api_key, scrape_config, rom, &public_root, &self.db) {
                Ok(true) => summary.updated += 1,
                Ok(false) => summary.skipped += 1,
                Err(_) => summary.failed += 1,
            }

            progress(ManageProgressEvent {
                kind: ManageOperationKind::ScrapeMissingCovers,
                processed: index + 1,
                total: Some(total),
                message: format!("Scraped cover candidates for {}", rom.display_title),
            });

            if run.delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(run.delay_ms));
            }
        }

        summary.message = format!(
            "Cover scraping complete: {} updated, {} skipped, {} failed.",
            summary.updated, summary.skipped, summary.failed
        );
        Ok(summary)
    }

    fn list_all_roms(&self, scope: &ManageScope) -> Result<Vec<RomCard>> {
        const PAGE_SIZE: usize = 500;

        let mut all = Vec::new();
        let mut offset = 0;

        loop {
            let query = RomQuery {
                system: match scope {
                    ManageScope::AllSystems => None,
                    ManageScope::System(system) => Some(normalize_system(system)),
                },
                limit: PAGE_SIZE,
                offset,
                ..RomQuery::default()
            };
            let page = self.list_roms(&query)?;
            let page_len = page.len();
            all.extend(page);
            if page_len < PAGE_SIZE {
                break;
            }
            offset += page_len;
        }

        Ok(all)
    }

    fn local_profile_id(&self) -> &str {
        &self.local_profile_id
    }

    fn current_config(&self) -> Result<AppConfig> {
        self.config
            .lock()
            .map(|config| config.clone())
            .map_err(|_| anyhow!("config lock poisoned"))
    }
}

fn native_arcade_core_note(system: &str, core_override: Option<&str>) -> Option<&'static str> {
    if !system.eq_ignore_ascii_case("ARCADE") {
        return None;
    }

    match core_override {
        Some("mame2003") | Some("mame2003_plus") => {
            Some("Using the MAME 2003 compatibility path for this arcade romset.")
        }
        Some("fbneo") => Some("Using the FBNeo compatibility path for this arcade romset."),
        _ => None,
    }
}

fn normalize_system(system: &str) -> String {
    let normalized = system.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        String::from("NES")
    } else {
        normalized
    }
}

#[derive(Clone, Copy)]
struct ScanTarget {
    folder: &'static str,
    system: &'static str,
    emulator_core: &'static str,
    extensions: &'static [&'static str],
    max_bytes: Option<u64>,
}

const SCAN_TARGETS: [ScanTarget; 8] = [
    ScanTarget {
        folder: "nes",
        system: "NES",
        emulator_core: "nes",
        extensions: &[".nes"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "snes",
        system: "SNES",
        emulator_core: "snes",
        extensions: &[".sfc", ".smc"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "genesis",
        system: "GENESIS",
        emulator_core: "segaMD",
        extensions: &[".gen", ".smd", ".md", ".bin"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "gb",
        system: "GB",
        emulator_core: "gb",
        extensions: &[".gb", ".gbc", ".zip"],
        max_bytes: Some(16 * 1024 * 1024),
    },
    ScanTarget {
        folder: "gba",
        system: "GBA",
        emulator_core: "gba",
        extensions: &[".gba", ".zip"],
        max_bytes: Some(32 * 1024 * 1024),
    },
    ScanTarget {
        folder: "n64",
        system: "N64",
        emulator_core: "n64",
        extensions: &[".n64", ".z64", ".v64", ".zip"],
        max_bytes: Some(96 * 1024 * 1024),
    },
    ScanTarget {
        folder: "arcade",
        system: "ARCADE",
        emulator_core: "arcade",
        extensions: &[".zip"],
        max_bytes: Some(150 * 1024 * 1024),
    },
    ScanTarget {
        folder: "arcade-mame2003",
        system: "ARCADE",
        emulator_core: "mame2003",
        extensions: &[".zip"],
        max_bytes: Some(150 * 1024 * 1024),
    },
];

fn sanitize_api_key(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn scan_targets(scope: &ManageScope) -> Vec<ScanTarget> {
    match scope {
        ManageScope::AllSystems => SCAN_TARGETS.to_vec(),
        ManageScope::System(system) => {
            let normalized = normalize_system(system);
            SCAN_TARGETS
                .iter()
                .copied()
                .filter(|target| target.system == normalized)
                .collect()
        }
    }
}

fn target_for_path(path: &Path, targets: &[ScanTarget]) -> Option<ScanTarget> {
    let normalized = format!(
        "/{}/",
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_matches('/')
            .to_ascii_lowercase()
    );

    targets
        .iter()
        .copied()
        .find(|target| normalized.contains(&format!("/{}/", target.folder)))
}

fn resolve_system_directory(rom_root: &Path, folder: &str) -> Result<PathBuf> {
    if rom_root.file_name().and_then(|name| name.to_str()) == Some("roms") {
        return Ok(rom_root.join(folder));
    }

    let direct = rom_root.join(folder);
    if direct.exists() {
        return Ok(direct);
    }

    let nested_root = rom_root.join("roms");
    if nested_root.exists() {
        return Ok(nested_root.join(folder));
    }

    Ok(direct)
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }

    Ok(())
}

fn to_stored_file_path(path: &Path, rom_root: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(rom_root) {
        return stripped.to_string_lossy().replace('\\', "/");
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(stripped) = path.strip_prefix(&cwd) {
            return stripped.to_string_lossy().replace('\\', "/");
        }
    }

    path.to_string_lossy().replace('\\', "/")
}

fn title_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("ROM");
    let mut out = String::with_capacity(stem.len());
    let mut previous_space = false;
    for ch in stem.chars() {
        let next = if ch == '_' || ch == '.' { ' ' } else { ch };
        if next.is_whitespace() {
            if !previous_space {
                out.push(' ');
            }
            previous_space = true;
        } else {
            out.push(next);
            previous_space = false;
        }
    }
    out.trim().to_string()
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut previous_hyphen = false;
    for ch in value.chars() {
        let normalized = ch.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            out.push(normalized);
            previous_hyphen = false;
        } else if !previous_hyphen {
            out.push('-');
            previous_hyphen = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        String::from("rom")
    } else {
        trimmed
    }
}

fn unique_slug_for_scan(
    existing_slug: Option<String>,
    title: &str,
    used_slugs: &mut HashSet<String>,
) -> String {
    if let Some(existing_slug) = existing_slug {
        used_slugs.insert(existing_slug.clone());
        return existing_slug;
    }

    let base = slugify(title);
    let mut candidate = base.clone();
    let mut suffix = 2;
    while used_slugs.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    used_slugs.insert(candidate.clone());
    candidate
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn resolve_public_root(rom_root: &Path) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let candidates = [
        cwd.join("public"),
        cwd.parent()
            .map(|parent| parent.join("public"))
            .unwrap_or_else(|| cwd.join("public")),
        rom_root.join("public"),
        rom_root
            .parent()
            .map(|parent| parent.join("public"))
            .unwrap_or_else(|| rom_root.join("public")),
    ];

    if let Some(existing) = candidates.iter().find(|candidate| candidate.exists()) {
        return Ok(existing.clone());
    }

    let fallback = candidates[0].clone();
    fs::create_dir_all(&fallback)?;
    Ok(fallback)
}

fn build_arcade_cover_index(public_root: &Path) -> Result<HashMap<String, String>> {
    let mut index = HashMap::new();
    let dir = public_root.join("covers").join("arcade-mame2003");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(index),
        Err(err) => return Err(err.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("png"))
            != Some(true)
        {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if stem.is_empty() {
            continue;
        }
        index.entry(stem).or_insert_with(|| {
            format!(
                "/covers/arcade-mame2003/{}.png",
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
            )
        });
    }

    Ok(index)
}

fn resolve_local_arcade_cover_path(
    cover_index: &HashMap<String, String>,
    slug: &str,
    stored_file_path: &str,
) -> Option<String> {
    let basename = Path::new(stored_file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    for key in [slug.to_ascii_lowercase(), basename] {
        if let Some(value) = cover_index.get(&key) {
            return Some(value.clone());
        }
    }
    None
}

fn sanitize_cover_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut bracket_depth = 0_u32;
    let mut paren_depth = 0_u32;
    for ch in title.chars() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            _ if bracket_depth == 0 && paren_depth == 0 => {
                if ch == '_' || ch == '.' {
                    out.push(' ');
                } else {
                    out.push(ch);
                }
            }
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_match_text(value: &str) -> String {
    let mut out = String::new();
    let mut previous_space = false;
    for ch in value.chars() {
        let normalized = ch.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            out.push(normalized);
            previous_space = false;
        } else if !previous_space {
            out.push(' ');
            previous_space = true;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn platform_ids_for_system<'a>(config: &'a CoverScrapingConfig, system: &str) -> &'a [u32] {
    match system {
        "NES" => &config.platform_ids.nes,
        "SNES" => &config.platform_ids.snes,
        "GENESIS" => &config.platform_ids.genesis,
        "GB" => &config.platform_ids.gb,
        "GBA" => &config.platform_ids.gba,
        "N64" => &config.platform_ids.n64,
        "ARCADE" => &config.platform_ids.arcade,
        _ => &[],
    }
}

fn scrape_cover_for_rom(
    api_key: &str,
    scrape_config: &CoverScrapingConfig,
    rom: &RomCard,
    public_root: &Path,
    db: &Database,
) -> Result<bool> {
    let clean_title = sanitize_cover_title(&rom.rom.title);
    let payload = tgdb_request(
        "Games/ByGameName",
        &[
            ("apikey", api_key),
            ("name", clean_title.as_str()),
            ("include", "boxart"),
        ],
    )?;

    let games = collect_games(&payload);
    let platform_ids = platform_ids_for_system(scrape_config, &normalize_system(&rom.rom.system));
    let best_game = pick_best_game(&clean_title, &games, platform_ids);
    let Some(game) = best_game else {
        return Ok(false);
    };

    let game_id = game
        .get("id")
        .and_then(|value| value.as_i64())
        .or_else(|| game.get("game_id").and_then(|value| value.as_i64()))
        .ok_or_else(|| anyhow!("TheGamesDB response missing game id"))?;

    let boxart_data = payload
        .get("include")
        .and_then(|value| value.get("boxart"))
        .and_then(|value| value.get("data"))
        .and_then(|value| value.get(game_id.to_string()))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let Some(boxart) = pick_front_boxart(&boxart_data) else {
        return Ok(false);
    };
    let Some(file_name) = boxart.get("filename").and_then(|value| value.as_str()) else {
        return Ok(false);
    };
    let base_url = payload
        .get("include")
        .and_then(|value| value.get("boxart"))
        .and_then(|value| value.get("base_url"))
        .and_then(|value| {
            value
                .get("original")
                .or_else(|| value.get("large"))
                .or_else(|| value.get("medium"))
                .or_else(|| value.get("thumb"))
                .or_else(|| value.get("small"))
        })
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("TheGamesDB response missing cover base URL"))?;

    let cover_url = format!("{base_url}{file_name}");
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.trim().is_empty())
        .unwrap_or("jpg");
    let system_folder = normalize_system(&rom.rom.system).to_ascii_lowercase();
    let cover_dir = public_root.join("covers").join(&system_folder);
    fs::create_dir_all(&cover_dir)?;
    let file_name = format!("{}.{}", rom.rom.slug, extension);
    let dest_path = cover_dir.join(&file_name);
    let mut bytes = Vec::new();
    ureq::get(&cover_url)
        .call()
        .map_err(|err| anyhow!("cover download failed: {err}"))?
        .into_reader()
        .read_to_end(&mut bytes)?;
    fs::write(&dest_path, bytes)?;

    let public_path = format!("/covers/{system_folder}/{file_name}");
    db.update_rom_cover_path(&rom.rom.id, &public_path)?;
    Ok(true)
}

fn tgdb_request(endpoint: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut request = ureq::get(&format!("https://api.thegamesdb.net/v1.1/{endpoint}"));
    for (key, value) in params {
        request = request.query(key, value);
    }
    let text = request
        .call()
        .map_err(|err| anyhow!("TheGamesDB request failed: {err}"))?
        .into_string()?;
    Ok(serde_json::from_str(&text)?)
}

fn collect_games(payload: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(games) = payload.get("data").and_then(|value| value.get("games")) else {
        return Vec::new();
    };
    if let Some(items) = games.as_array() {
        return items.clone();
    }
    games
        .as_object()
        .map(|items| items.values().cloned().collect())
        .unwrap_or_default()
}

fn score_game(rom_title: &str, game: &serde_json::Value, platform_ids: &[u32]) -> i64 {
    let rom_norm = normalize_match_text(rom_title);
    let game_name = game
        .get("game_title")
        .and_then(|value| value.as_str())
        .or_else(|| game.get("name").and_then(|value| value.as_str()))
        .unwrap_or("");
    let game_norm = normalize_match_text(game_name);
    if game_norm.is_empty() {
        return 0;
    }

    let mut score = 10_i64;
    if rom_norm == game_norm {
        score = 100;
    } else if game_norm.starts_with(&rom_norm) {
        score = 80;
    } else if rom_norm.starts_with(&game_norm) {
        score = 70;
    } else if game_norm.contains(&rom_norm) {
        score = 60;
    }

    let platform = game
        .get("platform")
        .and_then(|value| value.as_u64())
        .unwrap_or_default() as u32;
    if !platform_ids.is_empty() && platform_ids.contains(&platform) {
        score += 20;
    }

    score
}

fn pick_best_game<'a>(
    rom_title: &str,
    games: &'a [serde_json::Value],
    platform_ids: &[u32],
) -> Option<&'a serde_json::Value> {
    let mut best = None;
    let mut best_score = i64::MIN;
    for game in games {
        let score = score_game(rom_title, game, platform_ids);
        if score > best_score {
            best = Some(game);
            best_score = score;
        }
    }
    best
}

fn pick_front_boxart(items: &[serde_json::Value]) -> Option<&serde_json::Value> {
    items
        .iter()
        .find(|item| item.get("side").and_then(|value| value.as_str()) == Some("front"))
        .or_else(|| items.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcade_data::Database;
    use arcade_domain::{
        CanonicalButton, MappingEntry, N64PreferredCore, PathsConfig, NEXT_SAVE_SLOT_ACTION,
        QUICK_LOAD_ACTION, QUICK_SAVE_ACTION, SYSTEM_DEFAULT_MAPPING_KEY,
    };
    use chrono::Utc;
    use rusqlite::params;
    use tempfile::TempDir;

    fn make_config(tmp: &TempDir) -> AppConfig {
        AppConfig {
            paths: PathsConfig {
                rom_root: tmp.path().join("roms"),
                db_path: tmp.path().join("data").join("arcade.db"),
                save_state_root: tmp.path().join("data").join("save-states"),
                core_root: tmp.path().join("cores"),
                bios_root: tmp.path().join("bios"),
            },
            ..AppConfig::default()
        }
    }

    fn config_path_for(config: &AppConfig) -> PathBuf {
        config.paths.db_path.with_file_name("config.toml")
    }

    fn seed_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-1', 'NES', 'rom-1', 'Test Rom', 'roms/nes/test.nes', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert rom");
    }

    fn seed_n64_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-2', 'N64', 'rom-2', 'Test N64', 'roms/n64/test.z64', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert n64 rom");
    }

    fn seed_arcade_mslug_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, emulatorCore, updatedAt)\n             VALUES ('rom-arcade-1', 'ARCADE', 'mslug', 'mslug', 'roms/arcade-mame2003/mslug.zip', 'mame2003', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert arcade rom");
    }

    #[test]
    fn prepare_launch_allows_local_launches() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_rom(&config);
        let rom_path = config.paths.rom_root.join("nes").join("test.nes");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services.prepare_launch("rom-1").expect("prepare launch");
        assert_eq!(plan.rom_id, "rom-1");
        assert_eq!(plan.system, "NES");
        assert_eq!(plan.rom_path, rom_path);
        assert_eq!(plan.effective_core, None);
        assert_eq!(plan.resolved_core_name, "fceumm");
        assert_eq!(plan.status_message, "Playing Test Rom (core: fceumm)");
    }

    #[test]
    fn prepare_launch_rejects_missing_roms() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services.prepare_launch("missing").expect_err("must fail");
        assert_eq!(err.to_string(), "ROM not found.");
    }

    #[test]
    fn prepare_launch_rejects_missing_rom_file() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_rom(&config);
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services.prepare_launch("rom-1").expect_err("must fail");
        assert!(err.to_string().starts_with("ROM file not found: "));
    }

    #[test]
    fn prepare_launch_uses_configured_n64_preferred_core() {
        let tmp = TempDir::new().expect("tempdir");
        let mut config = make_config(&tmp);
        config.emulation.n64.preferred_core = N64PreferredCore::Mupen64plusNext;
        let db = Database::open(&config).expect("open db");
        seed_n64_rom(&config);
        {
            let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
            conn.execute(
                "UPDATE \"Rom\" SET emulatorCore = 'mupen64plus_next' WHERE id = 'rom-2'",
                [],
            )
            .expect("set explicit rom core");
        }
        let rom_path = config.paths.rom_root.join("n64").join("test.z64");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services.prepare_launch("rom-2").expect("prepare launch");
        assert_eq!(plan.system, "N64");
        assert_eq!(plan.effective_core, Some(String::from("mupen64plus_next")));
        assert_eq!(plan.resolved_core_name, "mupen64plus_next");
    }

    #[test]
    fn prepare_launch_prefers_fbneo_for_mslug_even_when_scanned_as_mame2003() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_arcade_mslug_rom(&config);

        let rom_dir = config.paths.rom_root.join("arcade-mame2003");
        std::fs::create_dir_all(&rom_dir).expect("create arcade rom dir");
        std::fs::write(
            rom_dir.join("mslug.zip"),
            b"201-p1.bin 201-s1.bin sp-s3.sp1 sm1.sm1 sfix.sfix 000-lo.lo",
        )
        .expect("write mslug rom");

        std::fs::create_dir_all(&config.paths.bios_root).expect("create bios dir");
        for bios_name in ["neogeo.zip", "pgm.zip", "qsound.zip"] {
            std::fs::write(config.paths.bios_root.join(bios_name), b"bios").expect("write bios");
        }

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services
            .prepare_launch("rom-arcade-1")
            .expect("prepare launch");
        assert_eq!(plan.system, "ARCADE");
        assert_eq!(plan.effective_core, Some(String::from("fbneo")));
        assert_eq!(plan.resolved_core_name, "fbneo");
    }

    #[test]
    fn device_mapping_overrides_system_default() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure profile");

        let mut system_mapping = default_gamepad_mapping_for_system("NES");
        system_mapping.actions.insert(
            String::from("A"),
            Some(MappingEntry::Button {
                button: CanonicalButton::South,
            }),
        );
        db.upsert_gamepad_mapping(
            &profile_id,
            "NES",
            SYSTEM_DEFAULT_MAPPING_KEY,
            None,
            None,
            &serde_json::to_string(&system_mapping).expect("serialize system mapping"),
        )
        .expect("save system mapping");

        let mut device_mapping = default_gamepad_mapping_for_system("NES");
        device_mapping.actions.insert(
            String::from("A"),
            Some(MappingEntry::Button {
                button: CanonicalButton::East,
            }),
        );
        db.upsert_gamepad_mapping(
            &profile_id,
            "NES",
            "054c:0268:PS3",
            Some("054c"),
            Some("0268"),
            &serde_json::to_string(&device_mapping).expect("serialize device mapping"),
        )
        .expect("save device mapping");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");
        let resolved = services
            .resolve_gamepad_mapping(
                "NES",
                Some(&DetectedPadIdentity {
                    device_key: String::from("054c:0268:PS3"),
                    name: String::from("PS3"),
                    vendor_id: Some(String::from("054c")),
                    product_id: Some(String::from("0268")),
                    mapping_name: Some(String::from("SdlMappings")),
                }),
            )
            .expect("resolve mapping");

        let entry = resolved
            .actions
            .get("A")
            .and_then(|entry| entry.clone())
            .expect("A binding");
        assert_eq!(
            entry,
            MappingEntry::Button {
                button: CanonicalButton::East
            }
        );
    }

    #[test]
    fn recognized_devices_get_preset_shortcuts_when_no_override_exists() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let resolved = services
            .resolve_gamepad_mapping(
                "NES",
                Some(&DetectedPadIdentity {
                    device_key: String::from("054c:0ce6:DualSense"),
                    name: String::from("DualSense Wireless Controller"),
                    vendor_id: Some(String::from("054c")),
                    product_id: Some(String::from("0ce6")),
                    mapping_name: Some(String::from("SdlMappings")),
                }),
            )
            .expect("resolve mapping");

        assert_eq!(
            resolved.actions.get(QUICK_SAVE_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::LeftThumb,
            }))
        );
        assert_eq!(
            resolved.actions.get(QUICK_LOAD_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::RightThumb,
            }))
        );
        assert_eq!(
            resolved.actions.get(NEXT_SAVE_SLOT_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }))
        );
    }

    #[test]
    fn cover_scrape_settings_update_persists_to_config() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");

        services
            .update_cover_scrape_settings(CoverScrapeSettingsInput {
                tgdb_api_key: Some(String::from("abc123")),
                default_limit: 25,
                default_delay_ms: 90,
                nes_platform_ids: vec![7],
                snes_platform_ids: vec![6],
                genesis_platform_ids: vec![18],
                gb_platform_ids: vec![4],
                gba_platform_ids: vec![5],
                n64_platform_ids: vec![3],
                arcade_platform_ids: vec![23],
            })
            .expect("save settings");

        let (saved, _) = AppConfig::load_or_create(Some(&config_path)).expect("reload config");
        assert_eq!(
            saved.management.cover_scraping.tgdb_api_key.as_deref(),
            Some("abc123")
        );
        assert_eq!(saved.management.cover_scraping.default_limit, 25);
        assert_eq!(saved.management.cover_scraping.default_delay_ms, 90);
        assert_eq!(saved.management.cover_scraping.platform_ids.nes, vec![7]);
    }

    #[test]
    fn app_config_path_changes_are_saved_for_restart_without_rebinding_runtime_paths() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");
        let updated_paths = PathsConfig {
            rom_root: tmp.path().join("next-roms"),
            db_path: tmp.path().join("next-data").join("arcade.db"),
            save_state_root: tmp.path().join("next-data").join("save-states"),
            core_root: tmp.path().join("next-cores"),
            bios_root: tmp.path().join("next-bios"),
        };

        let mut core_settings = std::collections::HashMap::new();
        let mut mupen_vars = std::collections::HashMap::new();
        mupen_vars.insert(
            "mupen64plus-parallel-rdp-upscaling".to_string(),
            "2x".to_string(),
        );
        core_settings.insert("mupen64plus_next".to_string(), mupen_vars);

        let outcome = services
            .update_app_config_settings(updated_paths.clone(), core_settings)
            .expect("save settings");

        assert!(outcome.restart_required);

        let active = services.config();
        assert_eq!(active.paths.rom_root, config.paths.rom_root);
        assert_eq!(active.paths.db_path, config.paths.db_path);

        let (saved, _) = AppConfig::load_or_create(Some(&config_path)).expect("reload config");
        assert_eq!(saved.paths.rom_root, updated_paths.rom_root);
        assert_eq!(saved.paths.db_path, updated_paths.db_path);
        assert_eq!(saved.paths.save_state_root, updated_paths.save_state_root);
        assert_eq!(saved.paths.core_root, updated_paths.core_root);
        assert_eq!(saved.paths.bios_root, updated_paths.bios_root);
    }

    #[test]
    fn app_config_without_path_changes_updates_runtime_immediately() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");

        let mut core_settings = std::collections::HashMap::new();
        let mut mupen_vars = std::collections::HashMap::new();
        mupen_vars.insert(
            "mupen64plus-parallel-rdp-upscaling".to_string(),
            "4x".to_string(),
        );
        core_settings.insert("mupen64plus_next".to_string(), mupen_vars);

        let outcome = services
            .update_app_config_settings(config.paths.clone(), core_settings)
            .expect("save settings");

        assert!(!outcome.restart_required);

        let active = services.config();
        assert_eq!(active.paths.rom_root, config.paths.rom_root);
        assert_eq!(
            active
                .emulation
                .core_settings
                .get("mupen64plus_next")
                .and_then(|m| m.get("mupen64plus-parallel-rdp-upscaling"))
                .map(|s| s.as_str()),
            Some("4x")
        );
    }

    #[test]
    fn remove_roms_from_library_deletes_rom_row() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_rom(&config);
        let services =
            NativeServices::bootstrap(config.clone(), config_path_for(&config), db.clone())
                .expect("bootstrap");

        let summary = services
            .remove_roms_from_library(&[String::from("rom-1")], |_| {})
            .expect("remove rom");
        assert_eq!(summary.removed, 1);
        assert!(db.get_rom_by_id("rom-1").expect("lookup").is_none());
    }

    #[test]
    fn scrape_covers_requires_api_key() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services
            .scrape_covers(
                CoverScrapeRunOptions {
                    systems: vec![String::from("NES")],
                    limit: 1,
                    delay_ms: 0,
                    missing_only: true,
                },
                |_| {},
            )
            .expect_err("missing key should fail");
        assert_eq!(err.to_string(), "TheGamesDB API key is not configured.");
    }

    #[test]
    fn target_for_path_matches_windows_style_paths() {
        let path = Path::new(r"C:\Arcade\roms\nes\test.nes");
        let target = target_for_path(path, &SCAN_TARGETS).expect("target");
        assert_eq!(target.system, "NES");
        assert_eq!(target.folder, "nes");
    }
}
