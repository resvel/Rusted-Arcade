use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use arcade_domain::{
    AppConfig, Rom, RomCard, RomQuery, SaveLimits, SaveSlotData, SaveSlotSummary,
    SYSTEM_DEFAULT_MAPPING_KEY,
};
use chrono::{DateTime, Utc};
#[cfg(test)]
use rusqlite::params;
use rusqlite::Connection;
use thiserror::Error;
use uuid::Uuid;

mod database;
mod gamepad_mapping_repository;
mod rom_repository;
mod save_state_store;
mod schema;
mod time;

use self::schema::{create_backup_once, initialize_schema_if_needed};
use self::time::{now_sqlite, parse_db_datetime, parse_db_datetime_opt, to_cloud_version};

const LOCAL_PROFILE_ID: &str = "local-player";
pub const CLOUD_SLOT_MIN: i32 = 0;
pub const CLOUD_SLOT_MAX: i32 = 9;
pub const CLOUD_SLOT_COUNT: i32 = CLOUD_SLOT_MAX - CLOUD_SLOT_MIN + 1;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("version conflict")]
    VersionConflict(VersionConflictError),
    #[error("save state exceeds per-slot size limit")]
    SlotLimitExceeded,
    #[error("save state exceeds local profile size limit")]
    ProfileLimitExceeded,
    #[error("record not found")]
    NotFound,
    #[error("invalid slot")]
    InvalidSlot,
}

#[derive(Debug, Clone)]
pub struct VersionConflictError {
    pub current: SaveSlotSummary,
}

#[derive(Debug, Clone)]
pub struct GamepadMappingRecord {
    pub id: String,
    pub system: String,
    pub name: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub mapping_json: String,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanUpsertOutcome {
    Created,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Copy)]
pub struct ScannedRomInput<'a> {
    pub system: &'a str,
    pub emulator_core: &'a str,
    pub slug: &'a str,
    pub title: &'a str,
    pub stored_file_path: &'a str,
    pub absolute_file_path: Option<&'a str>,
    pub file_size: i64,
    pub checksum: Option<&'a str>,
    pub cover_path: Option<&'a str>,
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    pub save_state_root: PathBuf,
}

fn map_rom_card_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RomCard> {
    let title: String = row.get(3)?;
    let display_title: Option<String> = row.get(13)?;
    let rom = Rom {
        id: row.get(0)?,
        system: row.get(1)?,
        slug: row.get(2)?,
        title: title.clone(),
        file_path: row.get(4)?,
        emulator_core: row.get(5)?,
        cover_path: row.get(6)?,
        preview_video_path: row.get(7)?,
        preview_poster_path: row.get(8)?,
        preview_duration_sec: row.get(9)?,
        preview_updated_at: parse_db_datetime_opt(row.get::<_, Option<String>>(10)?),
        added_at: parse_db_datetime_opt(row.get::<_, Option<String>>(11)?),
        updated_at: parse_db_datetime_opt(row.get::<_, Option<String>>(12)?),
    };

    Ok(RomCard {
        rom,
        display_title: display_title.unwrap_or(title),
        release_year: row.get(14)?,
        manufacturer: row.get(15)?,
        genre: row.get(16)?,
        is_favorite: row.get::<_, i64>(17)? > 0,
    })
}

fn map_gamepad_mapping_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GamepadMappingRecord> {
    Ok(GamepadMappingRecord {
        id: row.get(0)?,
        system: row.get(1)?,
        name: row.get(2)?,
        vendor_id: row.get(3)?,
        product_id: row.get(4)?,
        mapping_json: row.get(5)?,
        updated_at: parse_db_datetime_opt(row.get::<_, Option<String>>(6)?),
    })
}

fn write_state_file_with_rollback(
    state_path: &Path,
    state_data: &[u8],
    commit: impl FnOnce() -> Result<(), DataError>,
) -> Result<(), DataError> {
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent).map_err(|_| DataError::NotFound)?;
    }

    let temp_path = state_path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    let backup_path = if state_path.exists() {
        Some(state_path.with_extension(format!("bak-{}", Uuid::new_v4().simple())))
    } else {
        None
    };

    if let Some(path) = backup_path.as_ref() {
        fs::rename(state_path, path).map_err(|_| DataError::NotFound)?;
    }

    if let Err(err) = fs::write(&temp_path, state_data).map_err(|_| DataError::NotFound) {
        restore_state_file(state_path, &temp_path, backup_path.as_deref());
        return Err(err);
    }

    if let Err(err) = fs::rename(&temp_path, state_path).map_err(|_| DataError::NotFound) {
        restore_state_file(state_path, &temp_path, backup_path.as_deref());
        return Err(err);
    }

    if let Err(err) = commit() {
        restore_state_file(state_path, &temp_path, backup_path.as_deref());
        return Err(err);
    }

    if let Some(path) = backup_path {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

fn restore_state_file(state_path: &Path, temp_path: &Path, backup_path: Option<&Path>) {
    let _ = fs::remove_file(temp_path);
    let _ = fs::remove_file(state_path);
    if let Some(path) = backup_path {
        let _ = fs::rename(path, state_path);
    }
}

fn sanitize_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        }
    }
    if out.is_empty() {
        String::from("unknown")
    } else {
        out
    }
}

fn normalize_label(label: Option<&str>) -> Option<String> {
    let mut value = label.unwrap_or("").trim().to_string();
    if value.is_empty() {
        return None;
    }
    if value.len() > 120 {
        value.truncate(120);
    }
    Some(value)
}

fn validate_slot(slot: i32) -> Result<()> {
    if (CLOUD_SLOT_MIN..=CLOUD_SLOT_MAX).contains(&slot) {
        Ok(())
    } else {
        Err(anyhow!("invalid slot"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(tmp: &TempDir) -> AppConfig {
        AppConfig {
            paths: arcade_domain::PathsConfig {
                rom_root: tmp.path().join("roms"),
                db_path: tmp.path().join("data").join("arcade.db"),
                save_state_root: tmp.path().join("data").join("save-states"),
                core_root: tmp.path().join("cores"),
                bios_root: tmp.path().join("bios"),
            },
            ..AppConfig::default()
        }
    }

    fn insert_rom(db: &Database, rom_id: &str) {
        let conn = db.conn.lock().expect("lock db");
        conn.execute(
            "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)\n             VALUES (?1, 'NES', ?2, 'Test ROM', ?3, ?4)",
            params![
                rom_id,
                format!("slug-{rom_id}"),
                format!("roms/nes/{rom_id}.nes"),
                now_sqlite(),
            ],
        )
        .expect("insert rom");
    }

    #[test]
    fn bootstrap_creates_local_profile_and_schema() {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&test_config(&tmp)).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");
        assert!(!profile_id.is_empty());
    }

    #[test]
    fn save_slot_roundtrip_and_delete() {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&test_config(&tmp)).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");
        insert_rom(&db, "rom-1");

        let limits = SaveLimits {
            slot_count: CLOUD_SLOT_COUNT,
            slot_max_bytes: 1024 * 1024,
            profile_max_bytes: 1024 * 1024 * 10,
        };

        db.save_save_slot(
            &profile_id,
            "rom-1",
            1,
            Some("slot label"),
            None,
            &[1, 2, 3, 4],
            &limits,
        )
        .expect("save slot");

        let loaded = db
            .load_save_slot(&profile_id, "rom-1", 1)
            .expect("load slot")
            .expect("slot present");
        assert_eq!(loaded.bytes, vec![1, 2, 3, 4]);

        {
            let conn = db.conn.lock().expect("lock db");
            let deleted = conn
                .execute(
                    "DELETE FROM \"SaveState\" WHERE profileId = ?1 AND romId = ?2 AND slot = ?3",
                    params![profile_id, "rom-1", 1],
                )
                .expect("delete slot");
            assert_eq!(deleted, 1);
        }

        let missing = db
            .load_save_slot(&profile_id, "rom-1", 1)
            .expect("load after delete");
        assert!(missing.is_none());
    }

    #[test]
    fn invalid_slot_rejected() {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&test_config(&tmp)).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");
        insert_rom(&db, "rom-2");

        let limits = SaveLimits {
            slot_count: CLOUD_SLOT_COUNT,
            slot_max_bytes: 1024,
            profile_max_bytes: 2048,
        };

        let err = db
            .save_save_slot(&profile_id, "rom-2", 99, None, None, &[0, 1], &limits)
            .expect_err("must fail");
        assert!(matches!(err, DataError::InvalidSlot));
    }

    #[test]
    fn write_state_file_rolls_back_new_files_on_commit_failure() {
        let tmp = TempDir::new().expect("tempdir");
        let state_path = tmp.path().join("slot-1.state");

        let err = write_state_file_with_rollback(&state_path, b"new", || Err(DataError::NotFound))
            .expect_err("commit should fail");

        assert!(matches!(err, DataError::NotFound));
        assert!(!state_path.exists());
    }

    #[test]
    fn write_state_file_restores_previous_contents_on_commit_failure() {
        let tmp = TempDir::new().expect("tempdir");
        let state_path = tmp.path().join("slot-1.state");
        fs::write(&state_path, b"old").expect("seed old state");

        let err = write_state_file_with_rollback(&state_path, b"new", || Err(DataError::NotFound))
            .expect_err("commit should fail");

        assert!(matches!(err, DataError::NotFound));
        assert_eq!(fs::read(&state_path).expect("restore old state"), b"old");
    }

    #[test]
    fn upsert_and_load_gamepad_mapping_roundtrip() {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&test_config(&tmp)).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");
        let mapping_json =
            serde_json::to_string(&arcade_domain::default_gamepad_mapping_for_system("NES"))
                .expect("serialize mapping");

        db.upsert_gamepad_mapping(
            &profile_id,
            "NES",
            SYSTEM_DEFAULT_MAPPING_KEY,
            None,
            None,
            &mapping_json,
        )
        .expect("save mapping");

        let loaded = db
            .load_effective_gamepad_mapping(&profile_id, "NES", None)
            .expect("load mapping")
            .expect("mapping present");
        assert_eq!(loaded.name, SYSTEM_DEFAULT_MAPPING_KEY);
        assert_eq!(loaded.mapping_json, mapping_json);
    }

    #[test]
    fn facade_smoke_test() {
        let tmp = TempDir::new().expect("tempdir");
        let config = test_config(&tmp);
        let db = Database::open(&config).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");
        insert_rom(&db, "rom-3");

        let roms = db
            .list_rom_cards(&profile_id, &RomQuery::default())
            .expect("list roms");
        assert_eq!(roms.len(), 1);

        let limits = SaveLimits {
            slot_count: CLOUD_SLOT_COUNT,
            slot_max_bytes: 1024 * 1024,
            profile_max_bytes: 1024 * 1024 * 10,
        };
        db.save_save_slot(&profile_id, "rom-3", 0, None, None, &[7, 8, 9], &limits)
            .expect("save slot");

        let mapping_json =
            serde_json::to_string(&arcade_domain::default_gamepad_mapping_for_system("NES"))
                .expect("serialize mapping");
        db.upsert_gamepad_mapping(
            &profile_id,
            "NES",
            SYSTEM_DEFAULT_MAPPING_KEY,
            None,
            None,
            &mapping_json,
        )
        .expect("save mapping");
    }

    #[test]
    fn list_rom_cards_orders_by_display_title_in_fast_path() {
        let tmp = TempDir::new().expect("tempdir");
        let db = Database::open(&test_config(&tmp)).expect("open db");
        let profile_id = db.ensure_local_profile_id().expect("ensure local profile");

        {
            let conn = db.conn.lock().expect("lock db");
            conn.execute(
                "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)
                 VALUES (?1, 'NES', ?2, ?3, ?4, ?5)",
                params![
                    "rom-zulu",
                    "slug-rom-zulu",
                    "Zulu Base",
                    "roms/nes/rom-zulu.nes",
                    now_sqlite(),
                ],
            )
            .expect("insert rom with metadata");
            conn.execute(
                "INSERT INTO \"Rom\" (id, system, slug, title, filePath, updatedAt)
                 VALUES (?1, 'NES', ?2, ?3, ?4, ?5)",
                params![
                    "rom-beta",
                    "slug-rom-beta",
                    "Beta Base",
                    "roms/nes/rom-beta.nes",
                    now_sqlite(),
                ],
            )
            .expect("insert rom without metadata");
            conn.execute(
                "INSERT INTO \"ArcadeMetadata\" (id, romId, displayTitle, matchMethod, confidence, updatedAt)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    "rom-zulu",
                    "Alpha Display",
                    "test",
                    1.0_f64,
                    now_sqlite(),
                ],
            )
            .expect("insert metadata");
        }

        let roms = db
            .list_rom_cards(&profile_id, &RomQuery::default())
            .expect("list roms");

        assert_eq!(roms.len(), 2);
        assert_eq!(roms[0].rom.id, "rom-zulu");
        assert_eq!(roms[1].rom.id, "rom-beta");
    }
}
