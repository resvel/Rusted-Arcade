use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

use super::*;

const BOOTSTRAP_SQL: &str = include_str!("../../../../sql/bootstrap.sql");
const LEGACY_BACKUP_SENTINEL: &str = ".native-backup-created";

pub(super) fn initialize_schema_if_needed(conn: &Connection) -> Result<()> {
    let has_user_table: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master WHERE type='table' AND name='User'
         )",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;

    if !has_user_table {
        conn.execute_batch(BOOTSTRAP_SQL)
            .context("failed to initialize sqlite schema from bootstrap.sql")?;
    } else {
        migrate_profile_columns(conn)?;
    }

    Ok(())
}

pub(super) fn create_backup_once(db_path: &Path) -> Result<()> {
    let Some(sentinel_path) = backup_sentinel_path(db_path) else {
        return Ok(());
    };

    if sentinel_path.exists() {
        return Ok(());
    }

    let legacy_sentinel_path = legacy_backup_sentinel_path(&sentinel_path);
    if migrate_matching_legacy_sentinel(&legacy_sentinel_path, &sentinel_path, db_path)? {
        return Ok(());
    }

    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let parent = sentinel_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_name = format!(
        "{}.bak-{}",
        db_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("arcade.db"),
        ts
    );
    let backup_path = parent.join(backup_name);

    fs::copy(db_path, &backup_path).with_context(|| {
        format!(
            "failed to create database backup {}",
            backup_path.to_string_lossy()
        )
    })?;

    fs::write(
        &sentinel_path,
        format!("{}\n", backup_path.to_string_lossy()),
    )
    .with_context(|| {
        format!(
            "failed to write backup sentinel {}",
            sentinel_path.to_string_lossy()
        )
    })?;

    Ok(())
}

fn backup_sentinel_path(db_path: &Path) -> Option<PathBuf> {
    let parent = db_path.parent()?;
    let db_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("arcade.db");
    Some(parent.join(format!("{LEGACY_BACKUP_SENTINEL}-{db_name}")))
}

fn legacy_backup_sentinel_path(sentinel_path: &Path) -> PathBuf {
    sentinel_path
        .parent()
        .map(|parent| parent.join(LEGACY_BACKUP_SENTINEL))
        .unwrap_or_else(|| PathBuf::from(LEGACY_BACKUP_SENTINEL))
}

fn migrate_matching_legacy_sentinel(
    legacy_sentinel_path: &Path,
    sentinel_path: &Path,
    db_path: &Path,
) -> Result<bool> {
    if !legacy_sentinel_path.exists() {
        return Ok(false);
    }

    let legacy_contents = fs::read_to_string(legacy_sentinel_path).with_context(|| {
        format!(
            "failed to read legacy backup sentinel {}",
            legacy_sentinel_path.to_string_lossy()
        )
    })?;
    let Some(backup_reference) = legacy_contents.lines().next().map(str::trim) else {
        return Ok(false);
    };
    if backup_reference.is_empty() || !backup_reference_matches_db(backup_reference, db_path) {
        return Ok(false);
    }

    fs::write(sentinel_path, format!("{backup_reference}\n")).with_context(|| {
        format!(
            "failed to migrate backup sentinel {}",
            sentinel_path.to_string_lossy()
        )
    })?;
    Ok(true)
}

fn backup_reference_matches_db(backup_reference: &str, db_path: &Path) -> bool {
    let Some(db_name) = db_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let expected_prefix = format!("{db_name}.bak-");
    Path::new(backup_reference)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with(&expected_prefix))
        .unwrap_or(false)
}

fn migrate_profile_columns(conn: &Connection) -> Result<()> {
    rename_column_if_present(conn, "SaveState", "userId", "profileId")?;
    rename_column_if_present(conn, "Favorite", "userId", "profileId")?;
    rename_column_if_present(conn, "GamepadMapping", "userId", "profileId")?;

    if !table_exists(conn, "GamepadMapping")? {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS \"GamepadMapping\" (
                \"id\" TEXT NOT NULL PRIMARY KEY,
                \"profileId\" TEXT NOT NULL,
                \"system\" TEXT NOT NULL,
                \"name\" TEXT NOT NULL,
                \"vendorId\" TEXT,
                \"productId\" TEXT,
                \"mappingJson\" TEXT NOT NULL,
                \"createdAt\" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                \"updatedAt\" DATETIME NOT NULL,
                CONSTRAINT \"GamepadMapping_profileId_fkey\" FOREIGN KEY (\"profileId\") REFERENCES \"User\" (\"id\") ON DELETE CASCADE ON UPDATE CASCADE
            )",
            [],
        )?;
    } else {
        if !table_has_column(conn, "GamepadMapping", "name")? {
            conn.execute(
                "ALTER TABLE \"GamepadMapping\" ADD COLUMN \"name\" TEXT",
                [],
            )?;
        }
        if !table_has_column(conn, "GamepadMapping", "vendorId")? {
            conn.execute(
                "ALTER TABLE \"GamepadMapping\" ADD COLUMN \"vendorId\" TEXT",
                [],
            )?;
        }
        if !table_has_column(conn, "GamepadMapping", "productId")? {
            conn.execute(
                "ALTER TABLE \"GamepadMapping\" ADD COLUMN \"productId\" TEXT",
                [],
            )?;
        }

        conn.execute(
            "UPDATE \"GamepadMapping\"
             SET system = UPPER(system), name = COALESCE(NULLIF(name, ''), ?1)",
            params![SYSTEM_DEFAULT_MAPPING_KEY],
        )?;
        conn.execute(
            "DELETE FROM \"GamepadMapping\"
             WHERE rowid NOT IN (
                 SELECT MAX(rowid)
                 FROM \"GamepadMapping\"
                 GROUP BY profileId, system, name
             )",
            [],
        )?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS \"SaveState_profileId_romId_idx\" ON \"SaveState\"(\"profileId\", \"romId\")",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS \"SaveState_profileId_romId_slot_key\" ON \"SaveState\"(\"profileId\", \"romId\", \"slot\")",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS \"Favorite_profileId_idx\" ON \"Favorite\"(\"profileId\")",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS \"Favorite_profileId_romId_key\" ON \"Favorite\"(\"profileId\", \"romId\")",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS \"Rom_title_nocase_idx\" ON \"Rom\"(\"title\" COLLATE NOCASE)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS \"Rom_system_title_nocase_idx\" ON \"Rom\"(\"system\", \"title\" COLLATE NOCASE)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS \"GamepadMapping_profileId_system_idx\" ON \"GamepadMapping\"(\"profileId\", \"system\")",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS \"GamepadMapping_profileId_system_name_key\" ON \"GamepadMapping\"(\"profileId\", \"system\", \"name\")",
        [],
    )?;

    Ok(())
}

fn rename_column_if_present(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) -> Result<()> {
    if !table_has_column(conn, table_name, old_name)?
        || table_has_column(conn, table_name, new_name)?
    {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE \"{table_name}\" RENAME COLUMN \"{old_name}\" TO \"{new_name}\""),
        [],
    )
    .with_context(|| format!("failed to rename {table_name}.{old_name} to {new_name}"))?;

    Ok(())
}

fn table_has_column(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table_name}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column_name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1
         )",
        params![table_name],
        |row| row.get::<_, i64>(0),
    )? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_backup_once_uses_a_separate_sentinel_for_each_database_file() {
        let dir = tempdir().expect("tempdir");
        let db_one = dir.path().join("arcade.db");
        let db_two = dir.path().join("alternate.db");
        fs::write(&db_one, b"one").expect("write db one");
        fs::write(&db_two, b"two").expect("write db two");

        create_backup_once(&db_one).expect("backup db one");
        create_backup_once(&db_two).expect("backup db two");

        let sentinel_one = backup_sentinel_path(&db_one).expect("sentinel one");
        let sentinel_two = backup_sentinel_path(&db_two).expect("sentinel two");
        assert!(sentinel_one.exists());
        assert!(sentinel_two.exists());
        assert_ne!(sentinel_one, sentinel_two);

        let backup_one = fs::read_to_string(&sentinel_one).expect("read sentinel one");
        let backup_two = fs::read_to_string(&sentinel_two).expect("read sentinel two");
        assert!(backup_one.contains("arcade.db.bak-"));
        assert!(backup_two.contains("alternate.db.bak-"));
    }

    #[test]
    fn create_backup_once_migrates_a_matching_legacy_sentinel_without_copying_again() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("arcade.db");
        let legacy_backup = dir.path().join("arcade.db.bak-legacy");
        let legacy_sentinel = dir.path().join(LEGACY_BACKUP_SENTINEL);
        fs::write(&db_path, b"db").expect("write db");
        fs::write(&legacy_backup, b"backup").expect("write backup");
        fs::write(&legacy_sentinel, format!("{}\n", legacy_backup.display()))
            .expect("write legacy sentinel");

        create_backup_once(&db_path).expect("migrate sentinel");

        let sentinel_path = backup_sentinel_path(&db_path).expect("sentinel");
        assert!(sentinel_path.exists());
        assert_eq!(
            fs::read_to_string(&sentinel_path).expect("read migrated sentinel"),
            format!("{}\n", legacy_backup.display())
        );
    }
}
