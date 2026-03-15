use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};

use super::*;

impl Database {
    pub fn list_rom_cards(&self, profile_id: &str, query: &RomQuery) -> Result<Vec<RomCard>> {
        let system = query.system.as_ref().map(|s| s.trim().to_uppercase());
        let system_ref = system.as_deref();
        let search = query
            .search
            .clone()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let search_pattern = format!("%{search}%");
        let alpha_raw = query
            .alpha
            .as_ref()
            .map(|value| value.trim().to_uppercase())
            .unwrap_or_default();
        let (alpha_mode, alpha_letter): (i64, Option<String>) =
            if alpha_raw.is_empty() || alpha_raw == "ALL" {
                (0, None)
            } else if alpha_raw == "0-9" {
                (1, None)
            } else if alpha_raw.len() == 1
                && alpha_raw
                    .chars()
                    .next()
                    .map(|ch| ch.is_ascii_alphabetic())
                    .unwrap_or(false)
            {
                (2, Some(alpha_raw.to_lowercase()))
            } else {
                (0, None)
            };
        let favorites_only = if query.favorites_only { 1_i64 } else { 0_i64 };
        let limit = if query.limit == 0 { 72 } else { query.limit } as i64;
        let offset = query.offset as i64;

        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let use_fast_query = search.is_empty() && alpha_mode == 0;
        let mut stmt = if use_fast_query {
            conn.prepare(
                "SELECT
                    r.id,
                    r.system,
                    r.slug,
                    r.title,
                    r.filePath,
                    r.emulatorCore,
                    r.coverPath,
                    r.previewVideoPath,
                    r.previewPosterPath,
                    r.previewDurationSec,
                    CAST(r.previewUpdatedAt AS TEXT) AS previewUpdatedAt,
                    CAST(r.addedAt AS TEXT) AS addedAt,
                    CAST(r.updatedAt AS TEXT) AS updatedAt,
                    am.displayTitle,
                    am.releaseYear,
                    am.manufacturer,
                    am.genre,
                    CASE WHEN f.id IS NULL THEN 0 ELSE 1 END AS isFavorite
                FROM \"Rom\" r
                LEFT JOIN \"ArcadeMetadata\" am ON am.romId = r.id
                LEFT JOIN \"Favorite\" f ON f.romId = r.id AND f.profileId = ?1
                WHERE (?2 IS NULL OR UPPER(r.system) = ?3)
                  AND (?4 = 0 OR f.id IS NOT NULL)
                ORDER BY lower(COALESCE(am.displayTitle, r.title)) ASC, lower(r.title) ASC
                LIMIT ?5 OFFSET ?6",
            )?
        } else {
            conn.prepare(
                "SELECT
                    r.id,
                    r.system,
                    r.slug,
                    r.title,
                    r.filePath,
                    r.emulatorCore,
                    r.coverPath,
                    r.previewVideoPath,
                    r.previewPosterPath,
                    r.previewDurationSec,
                    CAST(r.previewUpdatedAt AS TEXT) AS previewUpdatedAt,
                    CAST(r.addedAt AS TEXT) AS addedAt,
                    CAST(r.updatedAt AS TEXT) AS updatedAt,
                    am.displayTitle,
                    am.releaseYear,
                    am.manufacturer,
                    am.genre,
                    CASE WHEN f.id IS NULL THEN 0 ELSE 1 END AS isFavorite
                FROM \"Rom\" r
                LEFT JOIN \"ArcadeMetadata\" am ON am.romId = r.id
                LEFT JOIN \"Favorite\" f ON f.romId = r.id AND f.profileId = ?1
                WHERE (?2 IS NULL OR UPPER(r.system) = ?3)
                  AND (?4 = '' OR lower(COALESCE(am.displayTitle, r.title)) LIKE ?5 OR lower(r.title) LIKE ?5)
                  AND (?6 = 0 OR f.id IS NOT NULL)
                  AND (
                        ?7 = 0
                        OR (?7 = 1 AND substr(ltrim(COALESCE(am.displayTitle, r.title)), 1, 1) GLOB '[0-9]')
                        OR (?7 = 2 AND lower(substr(ltrim(COALESCE(am.displayTitle, r.title)), 1, 1)) = ?8)
                  )
                ORDER BY lower(COALESCE(am.displayTitle, r.title)) ASC, lower(r.title) ASC
                LIMIT ?9 OFFSET ?10",
            )?
        };

        let rows = if use_fast_query {
            stmt.query_map(
                params![
                    profile_id,
                    system_ref,
                    system_ref,
                    favorites_only,
                    limit,
                    offset
                ],
                map_rom_card_row,
            )?
        } else {
            stmt.query_map(
                params![
                    profile_id,
                    system_ref,
                    system_ref,
                    search,
                    search_pattern,
                    favorites_only,
                    alpha_mode,
                    alpha_letter,
                    limit,
                    offset
                ],
                map_rom_card_row,
            )?
        };

        let mut cards = Vec::new();
        for row in rows {
            cards.push(row?);
        }

        Ok(cards)
    }

    pub fn list_favorite_rom_cards(&self, profile_id: &str) -> Result<Vec<RomCard>> {
        const FAVORITES_PAGE_SIZE: usize = 500;

        let mut all_cards = Vec::new();
        let mut offset = 0;

        loop {
            let query = RomQuery {
                favorites_only: true,
                limit: FAVORITES_PAGE_SIZE,
                offset,
                ..RomQuery::default()
            };
            let page = self.list_rom_cards(profile_id, &query)?;
            let page_len = page.len();
            all_cards.extend(page);

            if page_len < FAVORITES_PAGE_SIZE {
                break;
            }

            offset += page_len;
        }

        Ok(all_cards)
    }

    pub fn get_rom_by_id(&self, rom_id: &str) -> Result<Option<Rom>> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, system, slug, title, filePath, emulatorCore, coverPath, previewVideoPath,
                    previewPosterPath, previewDurationSec,
                    CAST(previewUpdatedAt AS TEXT) AS previewUpdatedAt,
                    CAST(addedAt AS TEXT) AS addedAt,
                    CAST(updatedAt AS TEXT) AS updatedAt
             FROM \"Rom\" WHERE id = ?1 LIMIT 1",
        )?;

        stmt.query_row(params![rom_id], |row| {
            Ok(Rom {
                id: row.get(0)?,
                system: row.get(1)?,
                slug: row.get(2)?,
                title: row.get(3)?,
                file_path: row.get(4)?,
                emulator_core: row.get(5)?,
                cover_path: row.get(6)?,
                preview_video_path: row.get(7)?,
                preview_poster_path: row.get(8)?,
                preview_duration_sec: row.get(9)?,
                preview_updated_at: parse_db_datetime_opt(row.get::<_, Option<String>>(10)?),
                added_at: parse_db_datetime_opt(row.get::<_, Option<String>>(11)?),
                updated_at: parse_db_datetime_opt(row.get::<_, Option<String>>(12)?),
            })
        })
        .optional()
        .context("failed to get rom by id")
    }

    pub fn get_rom_card_by_id(&self, profile_id: &str, rom_id: &str) -> Result<Option<RomCard>> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT
                r.id,
                r.system,
                r.slug,
                r.title,
                r.filePath,
                r.emulatorCore,
                r.coverPath,
                r.previewVideoPath,
                r.previewPosterPath,
                r.previewDurationSec,
                CAST(r.previewUpdatedAt AS TEXT) AS previewUpdatedAt,
                CAST(r.addedAt AS TEXT) AS addedAt,
                CAST(r.updatedAt AS TEXT) AS updatedAt,
                am.displayTitle,
                am.releaseYear,
                am.manufacturer,
                am.genre,
                CASE WHEN f.id IS NULL THEN 0 ELSE 1 END AS isFavorite
             FROM \"Rom\" r
             LEFT JOIN \"ArcadeMetadata\" am ON am.romId = r.id
             LEFT JOIN \"Favorite\" f ON f.romId = r.id AND f.profileId = ?1
             WHERE r.id = ?2
             LIMIT 1",
        )?;

        stmt.query_row(params![profile_id, rom_id], map_rom_card_row)
            .optional()
            .context("failed to get rom card by id")
    }

    pub fn add_favorite(&self, profile_id: &str, rom_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        conn.execute(
            "INSERT OR IGNORE INTO \"Favorite\" (id, profileId, romId, createdAt)
             VALUES (?1, ?2, ?3, ?4)",
            params![Uuid::new_v4().to_string(), profile_id, rom_id, now_sqlite()],
        )
        .context("failed to add favorite")?;
        Ok(())
    }

    pub fn remove_favorite(&self, profile_id: &str, rom_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        conn.execute(
            "DELETE FROM \"Favorite\" WHERE profileId = ?1 AND romId = ?2",
            params![profile_id, rom_id],
        )
        .context("failed to remove favorite")?;
        Ok(())
    }

    pub fn upsert_scanned_rom(&self, input: ScannedRomInput<'_>) -> Result<ScanUpsertOutcome> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, filePath, system, emulatorCore, slug, title, fileSize, checksum, coverPath
             FROM \"Rom\"
             WHERE filePath = ?1 OR (?2 IS NOT NULL AND filePath = ?2)
             LIMIT 1",
        )?;

        let existing = stmt
            .query_row(
                params![input.stored_file_path, input.absolute_file_path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .context("failed to query scanned rom")?;

        if let Some((
            rom_id,
            existing_file_path,
            existing_system,
            existing_core,
            existing_slug,
            existing_title,
            existing_size,
            existing_checksum,
            existing_cover,
        )) = existing
        {
            let next_cover = if existing_cover.as_deref().unwrap_or("").trim().is_empty() {
                input
                    .cover_path
                    .map(str::to_string)
                    .or(existing_cover.clone())
            } else {
                existing_cover.clone()
            };
            let unchanged = existing_file_path == input.stored_file_path
                && existing_system == input.system
                && existing_core.as_deref() == Some(input.emulator_core)
                && existing_slug == input.slug
                && existing_title == input.title
                && existing_size == Some(input.file_size)
                && existing_checksum.as_deref() == input.checksum
                && existing_cover == next_cover;
            if unchanged {
                return Ok(ScanUpsertOutcome::Unchanged);
            }

            conn.execute(
                "UPDATE \"Rom\"
                 SET system = ?1,
                     emulatorCore = ?2,
                     slug = ?3,
                     title = ?4,
                     filePath = ?5,
                     fileSize = ?6,
                     checksum = ?7,
                     coverPath = ?8,
                     updatedAt = ?9
                 WHERE id = ?10",
                params![
                    input.system,
                    input.emulator_core,
                    input.slug,
                    input.title,
                    input.stored_file_path,
                    input.file_size,
                    input.checksum,
                    next_cover,
                    now_sqlite(),
                    rom_id,
                ],
            )
            .context("failed to update scanned rom")?;

            return Ok(ScanUpsertOutcome::Updated);
        }

        conn.execute(
            "INSERT INTO \"Rom\" (
                id, system, emulatorCore, slug, title, filePath, fileSize, checksum, coverPath, updatedAt
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                input.system,
                input.emulator_core,
                input.slug,
                input.title,
                input.stored_file_path,
                input.file_size,
                input.checksum,
                input.cover_path,
                now_sqlite(),
            ],
        )
        .context("failed to insert scanned rom")?;

        Ok(ScanUpsertOutcome::Created)
    }

    pub fn update_rom_cover_path(&self, rom_id: &str, cover_path: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        conn.execute(
            "UPDATE \"Rom\" SET coverPath = ?1, updatedAt = ?2 WHERE id = ?3",
            params![cover_path, now_sqlite(), rom_id],
        )
        .context("failed to update rom cover path")?;
        Ok(())
    }

    pub fn delete_roms(&self, rom_ids: &[String]) -> Result<usize> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let tx = conn.unchecked_transaction()?;
        let mut removed = 0_usize;
        for rom_id in rom_ids {
            removed += tx
                .execute("DELETE FROM \"Rom\" WHERE id = ?1", params![rom_id])
                .context("failed to delete rom")?;
        }
        tx.commit().context("failed to commit rom deletes")?;
        Ok(removed)
    }
}
