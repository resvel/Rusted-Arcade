use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use arcade_data::{DataError, Database, ScanUpsertOutcome, ScannedRomInput};
use arcade_domain::{
    default_gamepad_mapping_for_system, dependency_buildbot_base_url, dependency_manifest,
    get_arcade_compatibility, get_dolphin_sys_directory, normalize_core, resolve_core,
    resolve_effective_core_override, resolve_path_from_root, scan_dependency_report, AppConfig,
    CoverScrapePlatformIds, CoverScrapeRunOptions, CoverScrapeSettingsInput, CoverScrapingConfig,
    DependencyReport, DependencySource, DetectedPadIdentity, DreamcastInputMode,
    LocalCoverRelinkRunOptions, ManageOperationKind, ManageOperationSummary, ManageProgressEvent,
    ManageRomStatus, ManageScope, ManagementConfig, N64CpuCoreMode, N64PrimaryStick, PathsConfig,
    RomCard, RomQuery, SaveLimits, SaveSlotData, SaveSlotSummary, SavedGamepadMappingSummary,
    StoredGamepadMapping, PCECD_ACCEPTED_BIOS_FILES, SATURN_ACCEPTED_BIOS_FILES,
    SYSTEM_DEFAULT_MAPPING_KEY,
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
    pub cover_path: Option<String>,
    pub preview_poster_path: Option<String>,
    pub promote_core_on_success: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfigUpdateOutcome {
    pub restart_required: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyInstallRequest {
    pub component_id: String,
    pub local_source: Option<PathBuf>,
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

    pub fn n64_primary_stick(&self) -> N64PrimaryStick {
        match self.config.lock() {
            Ok(config) => config.emulation.n64.primary_stick,
            Err(poison) => poison.into_inner().emulation.n64.primary_stick,
        }
    }

    pub fn dreamcast_input_mode(&self) -> DreamcastInputMode {
        match self.config.lock() {
            Ok(config) => config.emulation.dreamcast.input_mode,
            Err(poison) => poison.into_inner().emulation.dreamcast.input_mode,
        }
    }

    pub fn n64_cpu_core_mode(&self) -> N64CpuCoreMode {
        match self.config.lock() {
            Ok(config) => config.emulation.n64.cpu_core_mode,
            Err(poison) => poison.into_inner().emulation.n64.cpu_core_mode,
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
        self.prepare_launch_inner(rom_id, None)
    }

    pub fn prepare_launch_with_core_override(
        &self,
        rom_id: &str,
        core_override: &str,
    ) -> Result<LaunchPlan> {
        let normalized = normalize_core(core_override)
            .ok_or_else(|| anyhow!("Unsupported core override: {core_override}"))?;
        self.prepare_launch_inner(rom_id, Some(normalized.as_str()))
    }

    pub fn set_rom_core_override(&self, rom_id: &str, core_override: &str) -> Result<()> {
        let normalized = normalize_core(core_override)
            .ok_or_else(|| anyhow!("Unsupported core override: {core_override}"))?;
        self.db.update_rom_core(rom_id, &normalized)
    }

    pub fn persist_successful_retry_core(&self, plan: &LaunchPlan) -> Result<bool> {
        let Some(core) = plan.promote_core_on_success.as_deref() else {
            return Ok(false);
        };
        if !plan.system.eq_ignore_ascii_case("ARCADE") {
            return Ok(false);
        }
        self.set_rom_core_override(&plan.rom_id, core)?;
        Ok(true)
    }

    fn prepare_launch_inner(
        &self,
        rom_id: &str,
        one_shot_core_override: Option<&str>,
    ) -> Result<LaunchPlan> {
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
        ensure_system_launch_dependencies(&rom.system, &rom_path, &config.paths)?;
        let configured_core = one_shot_core_override.or_else(|| {
            config
                .preferred_core_for_system(&rom.system)
                .or(rom.emulator_core.as_deref())
        });
        let effective_override = if one_shot_core_override.is_some() {
            configured_core.and_then(normalize_core)
        } else {
            resolve_effective_core_override(&rom.system, configured_core, Some(&rom.title))
        };
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
            resolved_core_name: resolved_core_name.clone(),
            status_message,
            active_core_note,
            cover_path: rom.cover_path,
            preview_poster_path: rom.preview_poster_path,
            promote_core_on_success: one_shot_core_override.map(|_| resolved_core_name),
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

        let mapping = if let Some(record) = self.db.load_gamepad_mapping(
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
            psx: input.psx_platform_ids,
            ps2: input.ps2_platform_ids,
            dreamcast: input.dreamcast_platform_ids,
            gamecube: input.gamecube_platform_ids,
            saturn: input.saturn_platform_ids,
            dos: input.dos_platform_ids,
            pcecd: input.pcecd_platform_ids,
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

    pub fn update_n64_primary_stick(&self, primary_stick: N64PrimaryStick) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;

        config.emulation.n64.primary_stick = primary_stick;
        config.save_to_path(self.config_path.as_ref())?;
        Ok(())
    }

    pub fn update_dreamcast_input_mode(&self, input_mode: DreamcastInputMode) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;

        config.emulation.dreamcast.input_mode = input_mode;
        config.save_to_path(self.config_path.as_ref())?;
        Ok(())
    }

    pub fn update_n64_cpu_core_mode(&self, cpu_core_mode: N64CpuCoreMode) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;

        config.emulation.n64.cpu_core_mode = cpu_core_mode;
        config.save_to_path(self.config_path.as_ref())?;
        Ok(())
    }

    pub fn list_manage_roms(&self, scope: &ManageScope) -> Result<Vec<ManageRomStatus>> {
        let config = self.current_config()?;
        let cover_root = resolve_cover_root(&config.paths)?;
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
                        .and_then(|value| resolve_local_cover_asset_path(&cover_root, value))
                        .is_some(),
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
        let cover_root = resolve_cover_root(&config.paths)?;
        let local_cover_index = build_local_cover_index(&cover_root)?;
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
        let arcade_cover_index = build_arcade_cover_index(&cover_root)?;

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
            let checksum = match hash_file(path) {
                Ok(checksum) => checksum,
                Err(err) => {
                    summary.failed += 1;
                    progress(ManageProgressEvent {
                        kind: ManageOperationKind::SmartScan,
                        processed: index + 1,
                        total: Some(total),
                        message: format!("Failed to scan {}: {err}", path.display()),
                    });
                    continue;
                }
            };
            let cover_path = resolve_local_cover_path_for_scan(
                &local_cover_index,
                &arcade_cover_index,
                target.system,
                &slug,
                &stored_file_path,
            );

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
            "Smart scan complete: {} created, {} updated, {} unchanged, {} skipped, {} failed.",
            summary.created, summary.updated, summary.unchanged, summary.skipped, summary.failed
        );
        Ok(summary)
    }

    pub fn relink_local_covers(
        &self,
        run: LocalCoverRelinkRunOptions,
        mut progress: impl FnMut(ManageProgressEvent),
    ) -> Result<ManageOperationSummary> {
        let config = self.current_config()?;
        let cover_root = resolve_cover_root(&config.paths)?;
        let local_cover_index = build_local_cover_index(&cover_root)?;
        let arcade_cover_index = build_arcade_cover_index(&cover_root)?;
        let allowed_systems = run
            .systems
            .iter()
            .map(|system| normalize_system(system))
            .collect::<HashSet<_>>();
        let mut candidates = self
            .list_all_roms(&ManageScope::AllSystems)?
            .into_iter()
            .filter(|rom| allowed_systems.contains(&normalize_system(&rom.rom.system)))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.display_title.cmp(&right.display_title));

        let total = candidates.len();
        let mut summary = ManageOperationSummary {
            kind: Some(ManageOperationKind::RelinkLocalCovers),
            ..ManageOperationSummary::default()
        };

        for (index, rom) in candidates.iter().enumerate() {
            let has_cover_path = rom
                .rom
                .cover_path
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            // Enforce fill-missing-only globally to avoid overwriting user-linked covers.
            if has_cover_path {
                if !run.missing_only {
                    // Explicitly keep existing behavior: relink never overwrites current coverPath.
                }
                summary.skipped += 1;
            } else if let Some(cover_path) = resolve_local_cover_path_for_scan(
                &local_cover_index,
                &arcade_cover_index,
                &rom.rom.system,
                &rom.rom.slug,
                &rom.rom.file_path,
            ) {
                summary.matched += 1;
                match self.db.update_rom_cover_path(&rom.rom.id, &cover_path) {
                    Ok(()) => summary.updated += 1,
                    Err(_) => summary.failed += 1,
                }
            } else {
                summary.missing += 1;
            }

            progress(ManageProgressEvent {
                kind: ManageOperationKind::RelinkLocalCovers,
                processed: index + 1,
                total: Some(total),
                message: format!("Relinked local cover for {}", rom.display_title),
            });
        }

        summary.message = format!(
            "Local cover relink complete: {} matched, {} updated, {} missing, {} skipped, {} failed.",
            summary.matched, summary.updated, summary.missing, summary.skipped, summary.failed
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

        let cover_root = resolve_cover_root(&config.paths)?;
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
            match scrape_cover_for_rom(api_key, scrape_config, rom, &cover_root, &self.db) {
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

    pub fn dependency_report(&self) -> Result<DependencyReport> {
        let config = self.current_config()?;
        Ok(scan_dependency_report(&config.paths))
    }

    pub fn runtime_setup_welcome_completed(&self) -> bool {
        self.config().management.runtime_setup.welcome_completed
    }

    pub fn mark_runtime_setup_welcome_completed(&self) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;
        config.management.runtime_setup.welcome_completed = true;
        config.save_to_path(self.config_path.as_ref())
    }

    pub fn prepare_safe_runtime_setup(&self) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow!("config lock poisoned"))?;
        config.emulation.apply_platform_defaults();
        config.management.runtime_setup.welcome_completed = true;
        config.ensure_dirs()?;
        config.save_to_path(self.config_path.as_ref())
    }

    pub fn open_dependency_target(&self, component_id: &str) -> Result<()> {
        let config = self.current_config()?;
        let component = dependency_manifest(&config.paths)
            .into_iter()
            .find(|component| component.id == component_id)
            .ok_or_else(|| anyhow!("unknown dependency component: {component_id}"))?;
        let path = if component.target_path.is_file()
            || component
                .target_path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some()
        {
            component
                .target_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| component.target_path.clone())
        } else {
            component.target_path.clone()
        };
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        open_path_in_finder(&path)
    }

    pub fn open_rom_folder(&self, system: Option<&str>) -> Result<()> {
        let config = self.current_config()?;
        let path = match system {
            Some(system) if !system.eq_ignore_ascii_case("ALL") => {
                let scope = ManageScope::System(system.to_ascii_uppercase());
                let target = scan_targets(&scope)
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("unknown setup system: {system}"))?;
                resolve_system_directory(&config.paths.rom_root, target.folder)?
            }
            _ => config.paths.rom_root,
        };
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        open_path_in_finder(&path)
    }

    pub fn install_dependency(
        &self,
        request: DependencyInstallRequest,
        progress: impl Fn(ManageProgressEvent),
    ) -> Result<ManageOperationSummary> {
        let config = self.current_config()?;
        let component = dependency_manifest(&config.paths)
            .into_iter()
            .find(|component| component.id == request.component_id)
            .ok_or_else(|| anyhow!("unknown dependency component: {}", request.component_id))?;

        progress(ManageProgressEvent {
            kind: ManageOperationKind::InstallDependency,
            processed: 0,
            total: Some(1),
            message: format!("Installing {}...", component.title),
        });

        match &component.source {
            DependencySource::LibretroBuildbot { file_name } => {
                install_buildbot_core(file_name, &component.target_path)?;
            }
            DependencySource::Homebrew { package_name } => {
                install_homebrew_package(package_name)?;
            }
            DependencySource::UpstreamDownload { url } => {
                install_upstream_download(url, &component.target_path)?;
            }
            DependencySource::ExternalGuided { url } => {
                open_url(url)?;
                return Ok(ManageOperationSummary {
                    kind: Some(ManageOperationKind::InstallDependency),
                    skipped: 1,
                    message: format!(
                        "Opened upstream source for {}. Import the downloaded file when ready.",
                        component.title
                    ),
                    ..ManageOperationSummary::default()
                });
            }
            DependencySource::LocalImport => {
                let Some(source) = request.local_source.as_ref() else {
                    return Err(anyhow!(
                        "{} needs a local file or folder import.",
                        component.title
                    ));
                };
                import_local_dependency(source, &component.target_path)?;
            }
            DependencySource::UserProvided => {
                return Err(anyhow!(
                    "{} is user-provided. Place the files in {}.",
                    component.title,
                    component.target_path.display()
                ));
            }
        }

        if component
            .target_path
            .extension()
            .and_then(|ext| ext.to_str())
            == Some("dylib")
        {
            ad_hoc_codesign(&component.target_path)?;
        }

        let report = scan_dependency_report(&config.paths);
        let ready = report
            .components
            .iter()
            .find(|status| status.component.id == component.id)
            .map(|status| status.ready())
            .unwrap_or(false);

        if !ready {
            return Err(anyhow!(
                "installed {}, but validation still reports it missing",
                component.title
            ));
        }

        progress(ManageProgressEvent {
            kind: ManageOperationKind::InstallDependency,
            processed: 1,
            total: Some(1),
            message: format!("Installed {}.", component.title),
        });

        Ok(ManageOperationSummary {
            kind: Some(ManageOperationKind::InstallDependency),
            updated: 1,
            message: format!("Installed {}.", component.title),
            ..ManageOperationSummary::default()
        })
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

fn is_pcecd_disc_content(rom_path: &Path) -> bool {
    let extension = rom_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    matches!(
        extension.as_deref(),
        Some("chd") | Some("cue") | Some("ccd") | Some("toc") | Some("m3u")
    )
}

fn ensure_system_launch_dependencies(
    system: &str,
    rom_path: &Path,
    paths: &PathsConfig,
) -> Result<()> {
    if system.eq_ignore_ascii_case("PCECD") {
        // HuCard-side content does not require a CD system card BIOS.
        if !is_pcecd_disc_content(rom_path) {
            return Ok(());
        }

        if arcade_domain::find_pcecd_bios_file(&paths.rom_root, Some(&paths.bios_root)).is_some() {
            return Ok(());
        }

        let preferred_bios_dir =
            arcade_domain::get_pcecd_bios_directory(&paths.rom_root, Some(&paths.bios_root));
        let accepted = PCECD_ACCEPTED_BIOS_FILES.join(", ");
        return Err(anyhow!(
            "Missing PCE-CD BIOS in {}. Add one of: {}",
            preferred_bios_dir.display(),
            accepted
        ));
    }

    if system.eq_ignore_ascii_case("SATURN") {
        if arcade_domain::find_saturn_bios_file(&paths.rom_root, Some(&paths.bios_root)).is_some() {
            return Ok(());
        }

        let preferred_bios_dir =
            arcade_domain::get_saturn_bios_directory(&paths.rom_root, Some(&paths.bios_root));
        let accepted = SATURN_ACCEPTED_BIOS_FILES.join(", ");
        return Err(anyhow!(
            "Missing Saturn BIOS in {}. Add one of: {}",
            preferred_bios_dir.display(),
            accepted
        ));
    }

    if system.eq_ignore_ascii_case("GAMECUBE") {
        if arcade_domain::find_dolphin_sys_directory(&paths.rom_root, Some(&paths.bios_root))
            .is_some()
        {
            return Ok(());
        }

        let preferred_sys_dir = get_dolphin_sys_directory(&paths.rom_root, Some(&paths.bios_root));
        return Err(anyhow!(
            "Missing Dolphin Sys folder in {}. Place Dolphin's Data/Sys folder at bios/dolphin-emu/Sys.",
            preferred_sys_dir.display()
        ));
    }

    Ok(())
}

fn install_buildbot_core(file_name: &str, target_path: &Path) -> Result<()> {
    let base_url = dependency_buildbot_base_url().ok_or_else(|| {
        anyhow!("no libretro buildbot source is configured for this architecture")
    })?;
    let url = format!("{base_url}/{file_name}.zip");
    let bytes = download_bytes(&url)?;
    let parent = target_path
        .parent()
        .ok_or_else(|| anyhow!("target path has no parent: {}", target_path.display()))?;
    fs::create_dir_all(parent)?;

    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).with_context(|| format!("failed to read {url}"))?;
    let mut extracted = false;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(name) = Path::new(file.name())
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if name != file_name {
            continue;
        }
        let mut out = fs::File::create(target_path)
            .with_context(|| format!("failed to create {}", target_path.display()))?;
        std::io::copy(&mut file, &mut out)?;
        extracted = true;
        break;
    }

    if extracted {
        Ok(())
    } else {
        Err(anyhow!("{url} did not contain {file_name}"))
    }
}

fn install_upstream_download(url: &str, target_path: &Path) -> Result<()> {
    let bytes = download_bytes(url)?;
    let parent = target_path
        .parent()
        .ok_or_else(|| anyhow!("target path has no parent: {}", target_path.display()))?;
    fs::create_dir_all(parent)?;
    fs::write(target_path, bytes)
        .with_context(|| format!("failed to write {}", target_path.display()))
}

fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ureq::get(url)
        .call()
        .map_err(|err| anyhow!("download failed from {url}: {err}"))?
        .into_reader()
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn install_homebrew_package(package_name: &str) -> Result<()> {
    let brew = find_brew_executable()
        .ok_or_else(|| anyhow!("Homebrew is not installed or brew was not found in PATH."))?;
    let status = Command::new(&brew)
        .arg("install")
        .arg(package_name)
        .status()
        .with_context(|| format!("failed to run {}", brew.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "brew install {package_name} failed with status {status}"
        ))
    }
}

fn find_brew_executable() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/opt/homebrew/bin/brew"),
        PathBuf::from("/usr/local/bin/brew"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("brew"))
                    .find(|candidate| candidate.is_file())
            })
        })
}

fn import_local_dependency(source: &Path, target_path: &Path) -> Result<()> {
    if source.is_dir() {
        copy_directory(source, target_path)
    } else {
        let parent = target_path
            .parent()
            .ok_or_else(|| anyhow!("target path has no parent: {}", target_path.display()))?;
        fs::create_dir_all(parent)?;
        fs::copy(source, target_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                target_path.display()
            )
        })?;
        Ok(())
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn ad_hoc_codesign(path: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let status = Command::new("/usr/bin/codesign")
        .arg("--force")
        .arg("--sign")
        .arg("-")
        .arg(path)
        .status()
        .with_context(|| format!("failed to codesign {}", path.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "codesign failed for {} with status {status}",
            path.display()
        ))
    }
}

fn open_path_in_finder(path: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        let status = Command::new("/usr/bin/open")
            .arg(path)
            .status()
            .with_context(|| format!("failed to open {}", path.display()))?;
        if status.success() {
            return Ok(());
        }
        return Err(anyhow!("open failed for {}", path.display()));
    }
    Ok(())
}

fn open_url(url: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        let status = Command::new("/usr/bin/open")
            .arg(url)
            .status()
            .with_context(|| format!("failed to open {url}"))?;
        if status.success() {
            return Ok(());
        }
        return Err(anyhow!("open failed for {url}"));
    }
    Ok(())
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

const SCAN_TARGETS: [ScanTarget; 15] = [
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
    ScanTarget {
        folder: "psx",
        system: "PSX",
        emulator_core: "psx",
        extensions: &[".cue", ".img", ".iso", ".pbp", ".chd"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "ps2",
        system: "PS2",
        emulator_core: "ps2",
        extensions: &[".iso", ".chd", ".gz", ".cso", ".bin"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "dreamcast",
        system: "DREAMCAST",
        emulator_core: "dreamcast",
        extensions: &[".cdi", ".gdi", ".chd"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "gamecube",
        system: "GAMECUBE",
        emulator_core: "dolphin",
        extensions: &[".iso", ".gcm", ".rvz", ".gcz", ".wbfs", ".ciso", ".tgc"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "saturn",
        system: "SATURN",
        emulator_core: "saturn",
        extensions: &[".chd", ".cue", ".ccd", ".toc", ".m3u"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "pcecd",
        system: "PCECD",
        emulator_core: "pcecd",
        extensions: &[".chd", ".cue", ".ccd", ".toc", ".m3u", ".pce", ".sgx"],
        max_bytes: None,
    },
    ScanTarget {
        folder: "dos",
        system: "DOS",
        emulator_core: "dos",
        extensions: &[".zip", ".exe", ".com", ".bat"],
        max_bytes: None,
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

fn resolve_cover_root(paths: &PathsConfig) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let runtime_root = paths
        .rom_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| paths.rom_root.clone());
    let candidates = [
        runtime_root.join("covers"),
        paths.rom_root.join("covers"),
        runtime_root.join("public").join("covers"),
        cwd.join("covers"),
        cwd.join("public").join("covers"),
        cwd.parent()
            .map(|parent| parent.join("covers"))
            .unwrap_or_else(|| cwd.join("covers")),
        cwd.parent()
            .map(|parent| parent.join("public").join("covers"))
            .unwrap_or_else(|| cwd.join("public").join("covers")),
    ];

    if let Some(existing) = candidates.iter().find(|candidate| candidate.exists()) {
        return Ok(existing.clone());
    }

    let fallback = candidates[0].clone();
    fs::create_dir_all(&fallback)?;
    Ok(fallback)
}

const LOCAL_COVER_EXTENSIONS: [&str; 4] = ["jpg", "png", "jpeg", "webp"];

#[derive(Default)]
struct LocalCoverIndex {
    systems: HashMap<String, SystemCoverIndex>,
}

#[derive(Default)]
struct SystemCoverIndex {
    exact_paths: HashMap<String, String>,
}

fn build_local_cover_index(cover_root: &Path) -> Result<LocalCoverIndex> {
    let entries = match fs::read_dir(cover_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LocalCoverIndex::default())
        }
        Err(err) => return Err(err.into()),
    };

    let mut index = LocalCoverIndex::default();
    for entry in entries {
        let entry = entry?;
        let system_dir = entry.path();
        if !system_dir.is_dir() {
            continue;
        }
        let system_key = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if system_key.trim().is_empty() {
            continue;
        }

        let system_entries = match fs::read_dir(&system_dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };

        let system_index = index.systems.entry(system_key.clone()).or_default();
        for file in system_entries {
            let file = file?;
            let path = file.path();
            if !path.is_file() {
                continue;
            }

            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase);
            let Some(extension) = extension else {
                continue;
            };
            if !LOCAL_COVER_EXTENSIONS.contains(&extension.as_str()) {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase);
            let Some(stem) = stem else {
                continue;
            };
            if stem.trim().is_empty() {
                continue;
            }

            let file_name = path.file_name().and_then(|value| value.to_str());
            let Some(file_name) = file_name else {
                continue;
            };
            let candidate_cover_path = format!("/covers/{system_key}/{file_name}");
            if let Some(existing_cover_path) = system_index.exact_paths.get(&stem) {
                let existing_rank = cover_extension_rank_from_stored_path(existing_cover_path);
                let candidate_rank = cover_extension_rank(extension.as_str());
                if candidate_rank > existing_rank {
                    continue;
                }
                if candidate_rank == existing_rank
                    && candidate_cover_path.to_ascii_lowercase()
                        >= existing_cover_path.to_ascii_lowercase()
                {
                    continue;
                }
            }
            system_index.exact_paths.insert(stem, candidate_cover_path);
        }
    }

    Ok(index)
}

fn cover_extension_rank(extension: &str) -> usize {
    LOCAL_COVER_EXTENSIONS
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(extension))
        .unwrap_or(usize::MAX)
}

fn cover_extension_rank_from_stored_path(path: &str) -> usize {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(cover_extension_rank)
        .unwrap_or(usize::MAX)
}

fn resolve_local_cover_path_for_scan(
    local_cover_index: &LocalCoverIndex,
    arcade_cover_index: &HashMap<String, String>,
    system: &str,
    slug: &str,
    stored_file_path: &str,
) -> Option<String> {
    if let Some(path) = resolve_local_cover_path_for_slug(local_cover_index, system, slug) {
        return Some(path);
    }
    if normalize_system(system) == "ARCADE" {
        return resolve_local_arcade_cover_path(arcade_cover_index, slug, stored_file_path);
    }
    None
}

fn resolve_local_cover_path_for_slug(
    local_cover_index: &LocalCoverIndex,
    system: &str,
    slug: &str,
) -> Option<String> {
    let system_key = normalize_system(system).to_ascii_lowercase();
    let system_index = local_cover_index.systems.get(&system_key)?;
    let slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        return None;
    }

    if let Some(path) = system_index.exact_paths.get(&slug) {
        return Some(path.clone());
    }

    if let Some(base_slug) = strip_trailing_numeric_suffix(&slug) {
        if let Some(path) = system_index.exact_paths.get(base_slug) {
            return Some(path.clone());
        }
    }

    let prefix = format!("{slug}-");
    let mut candidate = None;
    for stem in system_index.exact_paths.keys() {
        if !stem.starts_with(&prefix) {
            continue;
        }
        if candidate.is_some() {
            return None;
        }
        candidate = Some(stem.as_str());
    }
    candidate
        .and_then(|stem| system_index.exact_paths.get(stem))
        .cloned()
}

fn strip_trailing_numeric_suffix(slug: &str) -> Option<&str> {
    let (base, suffix) = slug.rsplit_once('-')?;
    if base.trim().is_empty() || suffix.trim().is_empty() {
        return None;
    }
    if !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(base)
}

fn resolve_local_cover_asset_path(cover_root: &Path, raw_path: &str) -> Option<PathBuf> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let direct = PathBuf::from(trimmed);
    if direct.is_absolute() && direct.exists() {
        return Some(direct);
    }

    let relative = trimmed
        .trim_start_matches('/')
        .strip_prefix("covers/")
        .unwrap_or_else(|| trimmed.trim_start_matches('/'));
    let candidate = cover_root.join(relative);
    candidate.exists().then_some(candidate)
}

fn build_arcade_cover_index(cover_root: &Path) -> Result<HashMap<String, String>> {
    let mut index = HashMap::new();
    let dir = cover_root.join("arcade-mame2003");
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
        "PSX" => &config.platform_ids.psx,
        "PS2" => &config.platform_ids.ps2,
        "DREAMCAST" => &config.platform_ids.dreamcast,
        "GAMECUBE" => &config.platform_ids.gamecube,
        "SATURN" => &config.platform_ids.saturn,
        "DOS" => &config.platform_ids.dos,
        "PCECD" => &config.platform_ids.pcecd,
        _ => &[],
    }
}

fn scrape_cover_for_rom(
    api_key: &str,
    scrape_config: &CoverScrapingConfig,
    rom: &RomCard,
    cover_root: &Path,
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
    let cover_dir = cover_root.join(&system_folder);
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

    let cover_path = format!("/covers/{system_folder}/{file_name}");
    db.update_rom_cover_path(&rom.rom.id, &cover_path)?;
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
        CanonicalButton, DreamcastInputMode, LocalCoverRelinkRunOptions, MappingEntry,
        N64CpuCoreMode, N64PreferredCore, N64PrimaryStick, PathsConfig, RomQuery,
        NEXT_SAVE_SLOT_ACTION, QUICK_LOAD_ACTION, QUICK_SAVE_ACTION, SYSTEM_DEFAULT_MAPPING_KEY,
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

    fn seed_pcecd_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-pcecd-1', 'PCECD', 'dracula-x', 'Dracula X', 'roms/pcecd/Dracula X.chd', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert pcecd rom");
    }

    fn seed_pcecd_hucard_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-pcecd-hucard-1', 'PCECD', 'bonk', 'Bonk', 'roms/pcecd/Bonk.pce', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert pcecd hucard rom");
    }

    fn seed_saturn_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-saturn-1', 'SATURN', 'dracula-x-saturn', 'Dracula X (Saturn)', 'roms/saturn/Dracula X (Saturn).chd', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert saturn rom");
    }

    fn seed_gamecube_rom(config: &AppConfig) {
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES ('rom-gamecube-1', 'GAMECUBE', 'f-zero-gx', 'F-Zero GX', 'roms/gamecube/F-Zero GX.rvz', ?1)",
            params![Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .expect("insert gamecube rom");
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
    fn prepare_launch_blocks_pcecd_without_bios() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_pcecd_rom(&config);
        let rom_path = config.paths.rom_root.join("pcecd").join("Dracula X.chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services
            .prepare_launch("rom-pcecd-1")
            .expect_err("launch should fail without bios");
        assert!(err.to_string().contains("Missing PCE-CD BIOS"));
        assert!(err.to_string().contains("syscard3.pce"));
    }

    #[test]
    fn prepare_launch_allows_pcecd_with_bios() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_pcecd_rom(&config);
        let rom_path = config.paths.rom_root.join("pcecd").join("Dracula X.chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");

        let bios_dir = config.paths.bios_root.join("pcecd");
        std::fs::create_dir_all(&bios_dir).expect("create bios dir");
        std::fs::write(bios_dir.join("SYSCARD3.PCE"), b"bios").expect("write bios");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services
            .prepare_launch("rom-pcecd-1")
            .expect("launch should pass with bios");
        assert_eq!(plan.system, "PCECD");
        assert_eq!(plan.resolved_core_name, "mednafen_pce_fast");
    }

    #[test]
    fn prepare_launch_allows_pcecd_hucard_without_bios() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_pcecd_hucard_rom(&config);
        let rom_path = config.paths.rom_root.join("pcecd").join("Bonk.pce");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"hucard-rom").expect("write rom");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services
            .prepare_launch("rom-pcecd-hucard-1")
            .expect("hucard launch should pass without bios");
        assert_eq!(plan.system, "PCECD");
        assert_eq!(plan.resolved_core_name, "mednafen_pce_fast");
    }

    #[test]
    fn prepare_launch_blocks_saturn_without_bios() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_saturn_rom(&config);
        let rom_path = config
            .paths
            .rom_root
            .join("saturn")
            .join("Dracula X (Saturn).chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services
            .prepare_launch("rom-saturn-1")
            .expect_err("launch should fail without bios");
        assert!(err.to_string().contains("Missing Saturn BIOS"));
        assert!(err.to_string().contains("sega_101.bin"));
    }

    #[test]
    fn prepare_launch_allows_saturn_with_bios() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_saturn_rom(&config);
        let rom_path = config
            .paths
            .rom_root
            .join("saturn")
            .join("Dracula X (Saturn).chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");

        let bios_dir = config.paths.bios_root.join("saturn");
        std::fs::create_dir_all(&bios_dir).expect("create bios dir");
        std::fs::write(bios_dir.join("SEGA_101.BIN"), b"bios").expect("write bios");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services
            .prepare_launch("rom-saturn-1")
            .expect("launch should pass with bios");
        assert_eq!(plan.system, "SATURN");
        assert_eq!(plan.resolved_core_name, "mednafen_saturn");
    }

    #[test]
    fn prepare_launch_blocks_gamecube_without_dolphin_sys() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_gamecube_rom(&config);
        let rom_path = config.paths.rom_root.join("gamecube").join("F-Zero GX.rvz");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let err = services
            .prepare_launch("rom-gamecube-1")
            .expect_err("launch should fail without Dolphin Sys");
        assert!(err.to_string().contains("Missing Dolphin Sys folder"));
        assert!(err.to_string().contains("bios/dolphin-emu/Sys"));
    }

    #[test]
    fn prepare_launch_allows_gamecube_with_dolphin_sys() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_gamecube_rom(&config);
        let rom_path = config.paths.rom_root.join("gamecube").join("F-Zero GX.rvz");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");
        std::fs::create_dir_all(
            config
                .paths
                .bios_root
                .join("dolphin-emu")
                .join("Sys")
                .join("GC"),
        )
        .expect("create Dolphin GC dir");
        std::fs::create_dir_all(
            config
                .paths
                .bios_root
                .join("dolphin-emu")
                .join("Sys")
                .join("GameSettings"),
        )
        .expect("create Dolphin GameSettings dir");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let plan = services
            .prepare_launch("rom-gamecube-1")
            .expect("launch should pass with Dolphin Sys");
        assert_eq!(plan.system, "GAMECUBE");
        assert_eq!(plan.resolved_core_name, "dolphin");
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
    fn prepare_launch_with_retry_core_overrides_scan_core_for_this_attempt() {
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
            .prepare_launch_with_core_override("rom-arcade-1", "mame2003_plus")
            .expect("prepare launch with retry core");

        assert_eq!(plan.system, "ARCADE");
        assert_eq!(plan.effective_core, Some(String::from("mame2003_plus")));
        assert_eq!(plan.resolved_core_name, "mame2003_plus");
    }

    #[test]
    fn set_rom_core_override_persists_successful_retry_choice() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        seed_arcade_mslug_rom(&config);

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");
        services
            .set_rom_core_override("rom-arcade-1", "fbneo")
            .expect("save override");

        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        let saved: String = conn
            .query_row(
                "SELECT emulatorCore FROM \"Rom\" WHERE id = 'rom-arcade-1'",
                [],
                |row| row.get(0),
            )
            .expect("read saved core");

        assert_eq!(saved, "fbneo");
    }

    #[test]
    fn persist_successful_retry_core_makes_next_launch_use_that_core() {
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
        let retry_plan = services
            .prepare_launch_with_core_override("rom-arcade-1", "fbneo")
            .expect("prepare retry launch");

        assert!(
            services
                .persist_successful_retry_core(&retry_plan)
                .expect("persist retry core"),
            "retry launches should save their successful core"
        );

        let next_plan = services
            .prepare_launch("rom-arcade-1")
            .expect("prepare next launch");
        assert_eq!(next_plan.resolved_core_name, "fbneo");
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
    fn recognized_devices_do_not_get_implicit_shortcuts_without_saved_mapping() {
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

        assert_eq!(resolved.actions.get(QUICK_SAVE_ACTION), Some(&None));
        assert_eq!(resolved.actions.get(QUICK_LOAD_ACTION), Some(&None));
        assert_eq!(resolved.actions.get(NEXT_SAVE_SLOT_ACTION), Some(&None));
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
                psx_platform_ids: vec![10],
                ps2_platform_ids: vec![11],
                dreamcast_platform_ids: vec![16],
                gamecube_platform_ids: vec![2],
                saturn_platform_ids: vec![22],
                dos_platform_ids: vec![1],
                pcecd_platform_ids: vec![4955],
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
        assert_eq!(
            saved.management.cover_scraping.platform_ids.pcecd,
            vec![4955]
        );
        assert_eq!(
            saved.management.cover_scraping.platform_ids.saturn,
            vec![22]
        );
        assert_eq!(
            saved.management.cover_scraping.platform_ids.gamecube,
            vec![2]
        );
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
    fn update_n64_primary_stick_persists_to_config() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");

        services
            .update_n64_primary_stick(N64PrimaryStick::Right)
            .expect("save n64 primary stick");

        let active = services.config();
        assert_eq!(active.emulation.n64.primary_stick, N64PrimaryStick::Right);

        let (saved, _) = AppConfig::load_or_create(Some(&config_path)).expect("reload config");
        assert_eq!(saved.emulation.n64.primary_stick, N64PrimaryStick::Right);
    }

    #[test]
    fn update_dreamcast_input_mode_persists_to_config() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");

        services
            .update_dreamcast_input_mode(DreamcastInputMode::AnalogPlusDpad)
            .expect("save dreamcast input mode");

        let active = services.config();
        assert_eq!(
            active.emulation.dreamcast.input_mode,
            DreamcastInputMode::AnalogPlusDpad
        );

        let (saved, _) = AppConfig::load_or_create(Some(&config_path)).expect("reload config");
        assert_eq!(
            saved.emulation.dreamcast.input_mode,
            DreamcastInputMode::AnalogPlusDpad
        );
    }

    #[test]
    fn update_n64_cpu_core_mode_persists_to_config() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let config_path = config_path_for(&config);
        let db = Database::open(&config).expect("open db");
        let services =
            NativeServices::bootstrap(config.clone(), config_path.clone(), db).expect("bootstrap");

        services
            .update_n64_cpu_core_mode(N64CpuCoreMode::DynamicRecompiler)
            .expect("save n64 cpu core mode");

        let active = services.config();
        assert_eq!(
            active.emulation.n64.cpu_core_mode,
            N64CpuCoreMode::DynamicRecompiler
        );

        let (saved, _) = AppConfig::load_or_create(Some(&config_path)).expect("reload config");
        assert_eq!(
            saved.emulation.n64.cpu_core_mode,
            N64CpuCoreMode::DynamicRecompiler
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
    fn local_cover_matcher_supports_exact_alias_and_unique_variant() {
        let tmp = TempDir::new().expect("tempdir");
        let cover_root = tmp.path().join("covers");
        let covers_dir = cover_root.join("nes");
        std::fs::create_dir_all(&covers_dir).expect("create cover dir");
        std::fs::write(covers_dir.join("contra-u.jpg"), b"cover").expect("write contra cover");
        std::fs::write(covers_dir.join("mario-u-a1.png"), b"cover").expect("write mario variant");
        std::fs::write(covers_dir.join("zelda-u-a1.jpg"), b"cover").expect("write zelda variant");
        std::fs::write(covers_dir.join("zelda-u-a2.jpg"), b"cover").expect("write zelda variant");

        let index = build_local_cover_index(&cover_root).expect("build index");
        assert_eq!(
            resolve_local_cover_path_for_slug(&index, "NES", "contra-u"),
            Some(String::from("/covers/nes/contra-u.jpg"))
        );
        assert_eq!(
            resolve_local_cover_path_for_slug(&index, "NES", "contra-u-2"),
            Some(String::from("/covers/nes/contra-u.jpg"))
        );
        assert_eq!(
            resolve_local_cover_path_for_slug(&index, "NES", "mario-u"),
            Some(String::from("/covers/nes/mario-u-a1.png"))
        );
        assert_eq!(
            resolve_local_cover_path_for_slug(&index, "NES", "zelda-u"),
            None
        );
        assert_eq!(
            resolve_local_cover_path_for_slug(&index, "NES", "super-mario-u"),
            None
        );
    }

    #[test]
    fn relink_local_covers_updates_only_missing_cover_paths() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let conn = rusqlite::Connection::open(&config.paths.db_path).expect("open sqlite");
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)
             VALUES ('rom-missing', 'NES', 'contra-u', 'Contra (U)', 'roms/nes/Contra (U).nes', ?1)",
            params![now.clone()],
        )
        .expect("insert rom-missing");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, coverPath, updatedAt)
             VALUES ('rom-existing', 'NES', 'mario-u', 'Mario (U)', 'roms/nes/Mario (U).nes', '/covers/nes/existing.jpg', ?1)",
            params![now],
        )
        .expect("insert rom-existing");

        let covers_dir = tmp.path().join("covers").join("nes");
        std::fs::create_dir_all(&covers_dir).expect("create cover dir");
        std::fs::write(covers_dir.join("contra-u.jpg"), b"cover").expect("write cover");
        std::fs::write(covers_dir.join("existing.jpg"), b"cover").expect("write existing cover");

        let services =
            NativeServices::bootstrap(config.clone(), config_path_for(&config), db.clone())
                .expect("bootstrap");

        let summary = services
            .relink_local_covers(
                LocalCoverRelinkRunOptions {
                    systems: vec![String::from("NES")],
                    missing_only: true,
                },
                |_| {},
            )
            .expect("relink");
        assert_eq!(summary.kind, Some(ManageOperationKind::RelinkLocalCovers));
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.updated, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.failed, 0);

        let missing = db
            .get_rom_by_id("rom-missing")
            .expect("get missing")
            .expect("missing rom");
        assert_eq!(
            missing.cover_path.as_deref(),
            Some("/covers/nes/contra-u.jpg")
        );

        let existing = db
            .get_rom_by_id("rom-existing")
            .expect("get existing")
            .expect("existing rom");
        assert_eq!(
            existing.cover_path.as_deref(),
            Some("/covers/nes/existing.jpg")
        );
    }

    #[test]
    fn smart_scan_auto_links_local_cover_paths() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_path = config.paths.rom_root.join("nes").join("Test Rom (U).nes");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"rom").expect("write rom");
        let covers_dir = tmp.path().join("covers").join("nes");
        std::fs::create_dir_all(&covers_dir).expect("create cover dir");
        std::fs::write(covers_dir.join("test-rom-u.jpg"), b"cover").expect("write cover");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("NES")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("NES")),
                ..RomQuery::default()
            })
            .expect("list roms");
        let scanned = cards
            .iter()
            .find(|card| card.rom.slug == "test-rom-u")
            .expect("scanned rom");
        assert_eq!(
            scanned.rom.cover_path.as_deref(),
            Some("/covers/nes/test-rom-u.jpg")
        );
    }

    #[test]
    fn smart_scan_accepts_ps2_bin_images() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_path = config.paths.rom_root.join("ps2").join("Test PS2 Game.bin");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"ps2-rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("PS2")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));
        assert_eq!(summary.created, 1);

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("PS2")),
                ..RomQuery::default()
            })
            .expect("list roms");
        let scanned = cards
            .iter()
            .find(|card| card.rom.slug == "test-ps2-game")
            .expect("scanned rom");
        assert_eq!(scanned.rom.system, "PS2");
        assert_eq!(scanned.rom.file_path, "ps2/Test PS2 Game.bin");
    }

    #[test]
    fn smart_scan_imports_pcecd_chd_images() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_path = config.paths.rom_root.join("pcecd").join("Dracula X.chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"pcecd-rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("PCECD")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));
        assert_eq!(summary.created, 1);

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("PCECD")),
                ..RomQuery::default()
            })
            .expect("list roms");
        let scanned = cards
            .iter()
            .find(|card| card.rom.slug == "dracula-x")
            .expect("scanned rom");
        assert_eq!(scanned.rom.system, "PCECD");
        assert_eq!(scanned.rom.file_path, "pcecd/Dracula X.chd");
    }

    #[test]
    fn smart_scan_imports_gamecube_images_and_skips_wii_homebrew_formats() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_dir = config.paths.rom_root.join("gamecube");
        std::fs::create_dir_all(&rom_dir).expect("create rom dir");
        for name in ["F-Zero GX.rvz", "Mario Sunshine.iso", "Metroid Prime.gcm"] {
            std::fs::write(rom_dir.join(name), b"gamecube-rom").expect("write gamecube rom");
        }
        std::fs::write(rom_dir.join("Wii Homebrew.dol"), b"wii-homebrew").expect("write dol");

        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("GAMECUBE")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));
        assert_eq!(summary.created, 3);
        assert_eq!(summary.skipped, 1);

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("GAMECUBE")),
                ..RomQuery::default()
            })
            .expect("list roms");
        assert_eq!(cards.len(), 3);
        assert!(cards
            .iter()
            .any(|card| card.rom.file_path == "gamecube/F-Zero GX.rvz"));
        assert!(cards
            .iter()
            .any(|card| card.rom.file_path == "gamecube/Mario Sunshine.iso"));
        assert!(cards
            .iter()
            .any(|card| card.rom.file_path == "gamecube/Metroid Prime.gcm"));
    }

    #[test]
    fn smart_scan_imports_pcecd_hucard_images() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_path = config.paths.rom_root.join("pcecd").join("Bonk.pce");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"pce-hucard-rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("PCECD")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));
        assert_eq!(summary.created, 1);

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("PCECD")),
                ..RomQuery::default()
            })
            .expect("list roms");
        let scanned = cards
            .iter()
            .find(|card| card.rom.slug == "bonk")
            .expect("scanned rom");
        assert_eq!(scanned.rom.system, "PCECD");
        assert_eq!(scanned.rom.file_path, "pcecd/Bonk.pce");
    }

    #[cfg(unix)]
    #[test]
    fn smart_scan_counts_unreadable_supported_files_as_failed() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_dir = config.paths.rom_root.join("pcecd");
        std::fs::create_dir_all(&rom_dir).expect("create rom dir");
        let readable_rom = rom_dir.join("Bonk.pce");
        let unreadable_rom = rom_dir.join("Unreadable.cue");
        std::fs::write(&readable_rom, b"pce-hucard-rom").expect("write readable rom");
        std::fs::write(&unreadable_rom, b"cue-rom").expect("write unreadable rom");
        std::fs::set_permissions(&unreadable_rom, std::fs::Permissions::from_mode(0o000))
            .expect("remove read permission");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("PCECD")), |_| {})
            .expect("smart scan should continue past unreadable files");
        assert_eq!(summary.created, 1);
        assert_eq!(summary.failed, 1);
        assert!(summary.message.contains("1 failed"));

        std::fs::set_permissions(&unreadable_rom, std::fs::Permissions::from_mode(0o644))
            .expect("restore read permission for temp cleanup");
    }

    #[test]
    fn smart_scan_imports_saturn_chd_images() {
        let tmp = TempDir::new().expect("tempdir");
        let config = make_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let rom_path = config
            .paths
            .rom_root
            .join("saturn")
            .join("Dracula X (Saturn).chd");
        std::fs::create_dir_all(rom_path.parent().expect("rom parent")).expect("create rom dir");
        std::fs::write(&rom_path, b"saturn-rom").expect("write rom");
        let services = NativeServices::bootstrap(config.clone(), config_path_for(&config), db)
            .expect("bootstrap");

        let summary = services
            .smart_scan_roms(&ManageScope::System(String::from("SATURN")), |_| {})
            .expect("smart scan");
        assert_eq!(summary.kind, Some(ManageOperationKind::SmartScan));
        assert_eq!(summary.created, 1);

        let cards = services
            .list_roms(&RomQuery {
                system: Some(String::from("SATURN")),
                ..RomQuery::default()
            })
            .expect("list roms");
        let scanned = cards
            .iter()
            .find(|card| card.rom.slug == "dracula-x-saturn")
            .expect("scanned rom");
        assert_eq!(scanned.rom.system, "SATURN");
        assert_eq!(scanned.rom.file_path, "saturn/Dracula X (Saturn).chd");
    }

    #[test]
    fn target_for_path_matches_windows_style_paths() {
        let path = Path::new(r"C:\Arcade\roms\nes\test.nes");
        let target = target_for_path(path, &SCAN_TARGETS).expect("target");
        assert_eq!(target.system, "NES");
        assert_eq!(target.folder, "nes");
    }
}
