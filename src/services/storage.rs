use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Result, params};

const CURRENT_SCHEMA_VERSION: i64 = 1;

fn open(config_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(config_dir).ok();
    let db_path = config_dir.join("zeshicast.db");
    let mut conn = Connection::open(&db_path)?;
    secure_database_permissions(&db_path);
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    init(&mut conn)?;
    Ok(conn)
}

#[cfg(unix)]
fn secure_database_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn secure_database_permissions(_path: &Path) {}

fn init(conn: &mut Connection) -> Result<()> {
    migrate(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version = schema_version(conn)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "database schema version {version} is newer than supported {CURRENT_SCHEMA_VERSION}"
        )));
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = conn.transaction()?;
    let mut version = version;
    while version < CURRENT_SCHEMA_VERSION {
        match version {
            0 => {
                migrate_0_to_1(&transaction)?;
                version = 1;
            }
            _ => {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "no migration path from schema version {version}"
                )));
            }
        }
    }
    transaction.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    transaction.commit()
}

fn schema_version(conn: &Connection) -> Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn migrate_0_to_1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clipboard (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            text     TEXT    NOT NULL UNIQUE,
            added_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage (
            identity  TEXT    NOT NULL PRIMARY KEY,
            last_used INTEGER NOT NULL,
            count     INTEGER NOT NULL DEFAULT 1
        );
        CREATE INDEX IF NOT EXISTS idx_clipboard_added_at ON clipboard(added_at);
        CREATE INDEX IF NOT EXISTS idx_usage_last_used ON usage(last_used);",
    )
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Clipboard ────────────────────────────────────────────────────────────────

pub fn clipboard_load(config_dir: &Path) -> Vec<(String, i64)> {
    clipboard_load_with_limit(config_dir, 100)
}

pub fn clipboard_load_with_limit(config_dir: &Path, limit: usize) -> Vec<(String, i64)> {
    let Ok(conn) = open(config_dir) else {
        return Vec::new();
    };
    let mut stmt = match conn
        .prepare("SELECT text, added_at FROM clipboard ORDER BY added_at DESC LIMIT ?1")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub fn clipboard_insert(config_dir: &Path, text: &str) -> Result<()> {
    clipboard_insert_with_limit(config_dir, text, 100)
}

pub fn clipboard_insert_with_limit(config_dir: &Path, text: &str, limit: usize) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "INSERT OR REPLACE INTO clipboard (text, added_at) VALUES (?1, ?2)",
        params![text, now()],
    )?;
    // Keep the table bounded — only the newest entries are ever loaded (LIMIT
    // 100), so prune the rest to stop the db from growing without bound.
    conn.execute(
        "DELETE FROM clipboard WHERE id NOT IN (
            SELECT id FROM clipboard ORDER BY added_at DESC LIMIT ?1
        )",
        params![limit as i64],
    )?;
    Ok(())
}

pub fn clipboard_prune(config_dir: &Path, limit: usize) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "DELETE FROM clipboard WHERE id NOT IN (
            SELECT id FROM clipboard ORDER BY added_at DESC LIMIT ?1
        )",
        params![limit as i64],
    )?;
    Ok(())
}

pub fn clipboard_delete(config_dir: &Path, text: &str) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute("DELETE FROM clipboard WHERE text = ?1", params![text])?;
    Ok(())
}

pub fn clipboard_clear(config_dir: &Path) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute("DELETE FROM clipboard", [])?;
    Ok(())
}

pub fn clipboard_has_data(config_dir: &Path) -> bool {
    let Ok(conn) = open(config_dir) else {
        return false;
    };
    conn.query_row("SELECT COUNT(*) FROM clipboard", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|n| n > 0)
    .unwrap_or(false)
}

// ── Usage (recent + frequency) ───────────────────────────────────────────────

pub fn usage_record(config_dir: &Path, identity: &str) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "INSERT INTO usage (identity, last_used, count) VALUES (?1, ?2, 1)
         ON CONFLICT(identity) DO UPDATE SET last_used = ?2, count = count + 1",
        params![identity, now()],
    )?;
    Ok(())
}

pub fn usage_recent(config_dir: &Path, limit: usize) -> Vec<String> {
    let Ok(conn) = open(config_dir) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare("SELECT identity FROM usage ORDER BY last_used DESC LIMIT ?1")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![limit as i64], |row| row.get(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

pub fn usage_frequencies(config_dir: &Path) -> HashMap<String, u32> {
    let Ok(conn) = open(config_dir) else {
        return HashMap::new();
    };
    let mut stmt = match conn.prepare("SELECT identity, count FROM usage") {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub fn usage_has_data(config_dir: &Path) -> bool {
    let Ok(conn) = open(config_dir) else {
        return false;
    };
    conn.query_row("SELECT COUNT(*) FROM usage", [], |row| row.get::<_, i64>(0))
        .map(|n| n > 0)
        .unwrap_or(false)
}

// ── One-time migrations from text files ──────────────────────────────────────

pub fn migrate_clipboard(config_dir: &Path, entries: &[String]) -> Result<()> {
    let conn = open(config_dir)?;
    let base = now();
    for (i, text) in entries.iter().enumerate() {
        let ts = base - i as i64;
        conn.execute(
            "INSERT OR IGNORE INTO clipboard (text, added_at) VALUES (?1, ?2)",
            params![text, ts],
        )?;
    }
    Ok(())
}

pub fn migrate_usage(
    config_dir: &Path,
    recent: &[String],
    frequencies: &HashMap<String, u32>,
) -> Result<()> {
    let conn = open(config_dir)?;
    let base = now();
    for (i, identity) in recent.iter().enumerate() {
        let last_used = base - i as i64;
        let count = frequencies.get(identity).copied().unwrap_or(1);
        conn.execute(
            "INSERT OR IGNORE INTO usage (identity, last_used, count) VALUES (?1, ?2, ?3)",
            params![identity, last_used, count],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zeshicast-{name}-{}-{nanos}", std::process::id()))
    }

    #[cfg(unix)]
    #[test]
    fn sqlite_database_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = test_dir("sqlite-mode");
        clipboard_insert(&dir, "secret").unwrap();

        let mode = std::fs::metadata(dir.join("zeshicast.db"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fresh_db_has_current_schema_version() {
        let dir = test_dir("fresh-schema");
        let conn = open(&dir).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(index_exists(&conn, "idx_clipboard_added_at"));
        assert!(index_exists(&conn, "idx_usage_last_used"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_db_migrates_to_current_schema() {
        let dir = test_dir("old-schema");
        std::fs::create_dir_all(&dir).unwrap();
        {
            let conn = Connection::open(dir.join("zeshicast.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE clipboard (
                    id       INTEGER PRIMARY KEY AUTOINCREMENT,
                    text     TEXT    NOT NULL UNIQUE,
                    added_at INTEGER NOT NULL
                );
                CREATE TABLE usage (
                    identity  TEXT    NOT NULL PRIMARY KEY,
                    last_used INTEGER NOT NULL,
                    count     INTEGER NOT NULL DEFAULT 1
                );
                INSERT INTO clipboard (text, added_at) VALUES ('old', 1);
                INSERT INTO usage (identity, last_used, count) VALUES ('app:old', 1, 2);",
            )
            .unwrap();
            assert_eq!(schema_version(&conn).unwrap(), 0);
        }

        let conn = open(&dir).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(index_exists(&conn, "idx_clipboard_added_at"));
        let migrated_text = conn
            .query_row("SELECT text FROM clipboard", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        assert_eq!(migrated_text, "old");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn clipboard_added_at_index_exists() {
        let dir = test_dir("clipboard-index");
        let conn = open(&dir).unwrap();

        assert!(index_exists(&conn, "idx_clipboard_added_at"));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn index_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![name],
            |_| Ok(()),
        )
        .is_ok()
    }
}
