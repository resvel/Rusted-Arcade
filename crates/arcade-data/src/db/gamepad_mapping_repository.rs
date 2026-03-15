use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};

use super::*;

impl Database {
    pub fn load_gamepad_mapping(
        &self,
        profile_id: &str,
        system: &str,
        mapping_key: &str,
    ) -> Result<Option<GamepadMappingRecord>> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, system, name, vendorId, productId, mappingJson, CAST(updatedAt AS TEXT) AS updatedAt
             FROM \"GamepadMapping\"
             WHERE profileId = ?1 AND system = ?2 AND name = ?3
             ORDER BY updatedAt DESC, rowid DESC
             LIMIT 1",
        )?;

        stmt.query_row(
            params![profile_id, system, mapping_key],
            map_gamepad_mapping_row,
        )
        .optional()
        .context("failed to load gamepad mapping")
    }

    pub fn load_effective_gamepad_mapping(
        &self,
        profile_id: &str,
        system: &str,
        device_key: Option<&str>,
    ) -> Result<Option<GamepadMappingRecord>> {
        if let Some(device_key) = device_key {
            if let Some(mapping) = self.load_gamepad_mapping(profile_id, system, device_key)? {
                return Ok(Some(mapping));
            }
        }

        self.load_gamepad_mapping(profile_id, system, SYSTEM_DEFAULT_MAPPING_KEY)
    }

    pub fn list_gamepad_mappings_for_system(
        &self,
        profile_id: &str,
        system: &str,
    ) -> Result<Vec<GamepadMappingRecord>> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, system, name, vendorId, productId, mappingJson, CAST(updatedAt AS TEXT) AS updatedAt
             FROM \"GamepadMapping\"
             WHERE profileId = ?1 AND system = ?2
             ORDER BY updatedAt DESC, rowid DESC",
        )?;

        let rows = stmt.query_map(params![profile_id, system], map_gamepad_mapping_row)?;
        let mut mappings = Vec::new();
        for row in rows {
            mappings.push(row?);
        }
        Ok(mappings)
    }

    pub fn upsert_gamepad_mapping(
        &self,
        profile_id: &str,
        system: &str,
        mapping_key: &str,
        vendor_id: Option<&str>,
        product_id: Option<&str>,
        mapping_json: &str,
    ) -> Result<GamepadMappingRecord> {
        {
            let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM \"GamepadMapping\"
                     WHERE profileId = ?1 AND system = ?2 AND name = ?3
                     LIMIT 1",
                    params![profile_id, system, mapping_key],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to query existing gamepad mapping")?;
            let now = now_sqlite();

            if let Some(id) = existing {
                conn.execute(
                    "UPDATE \"GamepadMapping\"
                     SET vendorId = ?1, productId = ?2, mappingJson = ?3, updatedAt = ?4
                     WHERE id = ?5",
                    params![vendor_id, product_id, mapping_json, &now, &id],
                )
                .context("failed to update gamepad mapping")?;
            } else {
                conn.execute(
                    "INSERT INTO \"GamepadMapping\" (
                        id, profileId, system, name, vendorId, productId, mappingJson, createdAt, updatedAt
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        profile_id,
                        system,
                        mapping_key,
                        vendor_id,
                        product_id,
                        mapping_json,
                        &now,
                    ],
                )
                .context("failed to insert gamepad mapping")?;
            }
        }

        self.load_gamepad_mapping(profile_id, system, mapping_key)?
            .ok_or_else(|| anyhow!("failed to reload saved gamepad mapping"))
    }
}
