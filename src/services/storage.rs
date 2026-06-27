use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Result, params};

fn open(config_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(config_dir).ok();
    let conn = Connection::open(config_dir.join("zeshicast.db"))?;
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> Result<()> {
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
        );",
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
