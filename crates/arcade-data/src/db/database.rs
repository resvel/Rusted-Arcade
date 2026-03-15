use std::fs;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use super::*;

impl Database {
    pub fn open(config: &AppConfig) -> Result<Self> {
        if let Some(parent) = config.paths.db_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create db parent directory {}", parent.display())
            })?;
        }

        if config.paths.db_path.exists() {
            create_backup_once(&config.paths.db_path)?;
        }

        let conn = Connection::open(&config.paths.db_path).with_context(|| {
            format!(
                "failed to open sqlite db {}",
                config.paths.db_path.display()
            )
        })?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("failed to enable sqlite foreign keys")?;
        initialize_schema_if_needed(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            save_state_root: config.paths.save_state_root.clone(),
        })
    }

    pub fn ensure_local_profile_id(&self) -> Result<String> {
        let conn = self.conn.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM \"User\" WHERE id = ?1 LIMIT 1",
                params![LOCAL_PROFILE_ID],
                |row| row.get(0),
            )
            .optional()
            .context("failed to fetch local profile")?;
        if let Some(id) = existing {
            return Ok(id);
        }

        let first_existing: Option<String> = conn
            .query_row(
                "SELECT id FROM \"User\" ORDER BY rowid ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("failed to fetch existing profile")?;
        if let Some(id) = first_existing {
            return Ok(id);
        }

        conn.execute(
            "INSERT INTO \"User\" (id, name) VALUES (?1, ?2)",
            params![LOCAL_PROFILE_ID, "Local Arcade"],
        )
        .context("failed to create local profile")?;

        Ok(String::from(LOCAL_PROFILE_ID))
    }
}
