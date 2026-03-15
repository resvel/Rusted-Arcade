use std::fs;

use anyhow::{anyhow, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use super::*;

impl Database {
    pub fn load_save_slot(
        &self,
        profile_id: &str,
        rom_id: &str,
        slot: i32,
    ) -> Result<Option<SaveSlotData>> {
        validate_slot(slot)?;

        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT label, sizeBytes, CAST(updatedAt AS TEXT) AS updatedAt, statePath
             FROM \"SaveState\"
             WHERE profileId = ?1 AND romId = ?2 AND slot = ?3
             LIMIT 1",
        )?;

        let row = stmt
            .query_row(params![profile_id, rom_id, slot], |row| {
                let updated_at_raw: String = row.get(2)?;
                let updated_at = parse_db_datetime(&updated_at_raw).unwrap_or_else(Utc::now);
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    updated_at,
                    row.get::<_, String>(3)?,
                ))
            })
            .optional()?;

        let Some((label, size_bytes, updated_at, state_path)) = row else {
            return Ok(None);
        };

        let bytes = match fs::read(&state_path) {
            Ok(b) => b,
            Err(_) => {
                conn.execute(
                    "DELETE FROM \"SaveState\" WHERE profileId = ?1 AND romId = ?2 AND slot = ?3",
                    params![profile_id, rom_id, slot],
                )?;
                return Ok(None);
            }
        };

        Ok(Some(SaveSlotData {
            slot,
            label,
            size_bytes,
            updated_at,
            version: to_cloud_version(updated_at),
            bytes,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_save_slot(
        &self,
        profile_id: &str,
        rom_id: &str,
        slot: i32,
        label: Option<&str>,
        expected_version: Option<&str>,
        state_data: &[u8],
        limits: &SaveLimits,
    ) -> Result<SaveSlotSummary, DataError> {
        validate_slot(slot).map_err(|_| DataError::InvalidSlot)?;

        if state_data.is_empty() {
            return Err(DataError::NotFound);
        }

        if state_data.len() as i64 > limits.slot_max_bytes {
            return Err(DataError::SlotLimitExceeded);
        }

        let mut conn = self.conn.lock().map_err(|_| DataError::NotFound)?;

        let existing: Option<(Option<String>, i64, String, String)> = conn
            .query_row(
                "SELECT label, sizeBytes, CAST(updatedAt AS TEXT) AS updatedAt, statePath
                 FROM \"SaveState\"
                 WHERE profileId = ?1 AND romId = ?2 AND slot = ?3",
                params![profile_id, rom_id, slot],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|_| DataError::NotFound)?;

        if let Some(expected) = expected_version {
            let Some((existing_label, existing_size, updated_at_raw, _)) = existing.clone() else {
                return Err(DataError::VersionConflict(VersionConflictError {
                    current: SaveSlotSummary {
                        slot,
                        has_state: false,
                        label: None,
                        size_bytes: 0,
                        updated_at: None,
                        version: None,
                    },
                }));
            };

            let current_updated_at =
                parse_db_datetime(&updated_at_raw).ok_or(DataError::NotFound)?;
            let current_version = to_cloud_version(current_updated_at);
            if current_version != expected {
                return Err(DataError::VersionConflict(VersionConflictError {
                    current: SaveSlotSummary {
                        slot,
                        has_state: true,
                        label: existing_label,
                        size_bytes: existing_size,
                        updated_at: Some(current_updated_at),
                        version: Some(current_version),
                    },
                }));
            }
        }

        let current_total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(sizeBytes), 0) FROM \"SaveState\" WHERE profileId = ?1",
                params![profile_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let existing_size = existing.as_ref().map(|(_, size, _, _)| *size).unwrap_or(0);
        let projected_total = current_total - existing_size + state_data.len() as i64;
        if projected_total > limits.profile_max_bytes {
            return Err(DataError::ProfileLimitExceeded);
        }

        let normalized_label = normalize_label(label);
        let now = Utc::now();
        let now_db = now_sqlite();
        let has_existing_row = existing.is_some();

        let state_path = self
            .save_state_root
            .join(sanitize_path_segment(profile_id))
            .join(sanitize_path_segment(rom_id))
            .join(format!("slot-{slot}.state"));

        write_state_file_with_rollback(&state_path, state_data, || {
            let tx = conn.transaction().map_err(|_| DataError::NotFound)?;
            if has_existing_row {
                tx.execute(
                    "UPDATE \"SaveState\"
                     SET label = ?1, sizeBytes = ?2, statePath = ?3, updatedAt = ?4
                     WHERE profileId = ?5 AND romId = ?6 AND slot = ?7",
                    params![
                        normalized_label.as_deref(),
                        state_data.len() as i64,
                        state_path.to_string_lossy().to_string(),
                        &now_db,
                        profile_id,
                        rom_id,
                        slot
                    ],
                )
                .map_err(|_| DataError::NotFound)?;
            } else {
                tx.execute(
                    "INSERT INTO \"SaveState\" (
                        id, profileId, romId, slot, label, sizeBytes, statePath, createdAt, updatedAt
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        profile_id,
                        rom_id,
                        slot,
                        normalized_label.as_deref(),
                        state_data.len() as i64,
                        state_path.to_string_lossy().to_string(),
                        &now_db,
                    ],
                )
                .map_err(|_| DataError::NotFound)?;
            }

            tx.commit().map_err(|_| DataError::NotFound)?;
            Ok(())
        })?;

        Ok(SaveSlotSummary {
            slot,
            has_state: true,
            label: normalized_label,
            size_bytes: state_data.len() as i64,
            updated_at: Some(now),
            version: Some(to_cloud_version(now)),
        })
    }
}
