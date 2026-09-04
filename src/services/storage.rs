use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Result, params};

const CURRENT_SCHEMA_VERSION: i64 = 2;

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
            1 => {
                migrate_1_to_2(&transaction)?;
                version = 2;
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

fn migrate_1_to_2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS snippets (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            title      TEXT    NOT NULL,
            prefix     TEXT    NOT NULL DEFAULT '',
            content    TEXT    NOT NULL,
            tags       TEXT    NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_snippets_title ON snippets(title);
        CREATE INDEX IF NOT EXISTS idx_snippets_prefix ON snippets(prefix);",
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

// ── Snippets ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetRecord {
    pub id: i64,
    pub title: String,
    pub prefix: String,
    pub content: String,
    pub tags: Vec<String>,
}

pub fn snippets_load(config_dir: &Path) -> Vec<SnippetRecord> {
    let Ok(conn) = open(config_dir) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare(
        "SELECT id, title, prefix, content, tags FROM snippets ORDER BY id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        let tags_str: String = row.get(4)?;
        let tags = tags_str
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Ok(SnippetRecord {
            id: row.get(0)?,
            title: row.get(1)?,
            prefix: row.get(2)?,
            content: row.get(3)?,
            tags,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub fn snippet_insert(
    config_dir: &Path,
    title: &str,
    prefix: &str,
    content: &str,
    tags: &[String],
) -> Result<i64> {
    let conn = open(config_dir)?;
    let ts = now();
    conn.execute(
        "INSERT INTO snippets (title, prefix, content, tags, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![title, prefix, content, tags.join(", "), ts, ts],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn snippet_update(
    config_dir: &Path,
    id: i64,
    title: &str,
    prefix: &str,
    content: &str,
    tags: &[String],
) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "UPDATE snippets SET title = ?1, prefix = ?2, content = ?3, tags = ?4, updated_at = ?5 WHERE id = ?6",
        params![title, prefix, content, tags.join(", "), now(), id],
    )?;
    Ok(())
}

pub fn snippet_delete(config_dir: &Path, id: i64) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn snippet_delete_by_title_and_content(
    config_dir: &Path,
    title: &str,
    content: &str,
) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "DELETE FROM snippets WHERE title = ?1 AND content = ?2",
        params![title, content],
    )?;
    Ok(())
}

pub fn snippet_has_data(config_dir: &Path) -> bool {
    let Ok(conn) = open(config_dir) else {
        return false;
    };
    conn.query_row("SELECT COUNT(*) FROM snippets", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|n| n > 0)
    .unwrap_or(false)
}

// ── One-time migrations from text files ──────────────────────────────────────

pub fn migrate_snippets(
    config_dir: &Path,
    entries: &[(String, String, Vec<String>)],
) -> Result<()> {
    let conn = open(config_dir)?;
    let ts = now();
    for (title, content, tags) in entries {
        conn.execute(
            "INSERT INTO snippets (title, prefix, content, tags, created_at, updated_at) VALUES (?1, '', ?2, ?3, ?4, ?5)",
            params![title, content, tags.join(", "), ts, ts],
        )?;
    }
    Ok(())
}

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

    fn clipboard_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM clipboard", [], |row| row.get(0))
            .unwrap()
    }

    /// Retention contract: pruning keeps only the newest `limit` rows and
    /// drops the older ones, so the table stays bounded.
    #[test]
    fn clipboard_prune_keeps_only_newest_rows_within_limit() {
        let dir = test_dir("prune-newest");
        {
            let conn = open(&dir).unwrap();
            for i in 1..=105 {
                conn.execute(
                    "INSERT INTO clipboard (text, added_at) VALUES (?1, ?2)",
                    params![format!("item-{i}"), i],
                )
                .unwrap();
            }
        }

        clipboard_prune(&dir, 100).unwrap();

        let conn = open(&dir).unwrap();
        assert_eq!(clipboard_count(&conn), 100);
        // Rows 1..=5 (the oldest by added_at) were dropped; the newest 100,
        // added_at 6..=105, survive.
        let oldest: i64 = conn
            .query_row("SELECT MIN(added_at) FROM clipboard", [], |row| row.get(0))
            .unwrap();
        assert_eq!(oldest, 6);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn clipboard_prune_with_limit_above_row_count_removes_nothing() {
        let dir = test_dir("prune-noop");
        clipboard_insert(&dir, "only").unwrap();

        clipboard_prune(&dir, 100).unwrap();

        let conn = open(&dir).unwrap();
        assert_eq!(clipboard_count(&conn), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Legacy txt→SQLite migration deduplicates repeated entries via the
    /// UNIQUE constraint on `clipboard.text` (INSERT OR IGNORE skips them).
    #[test]
    fn migrate_clipboard_deduplicates_duplicate_entries() {
        let dir = test_dir("migrate-clipboard-dedup");
        migrate_clipboard(
            &dir,
            &["a".to_string(), "a".to_string(), "b".to_string()],
        )
        .unwrap();

        let conn = open(&dir).unwrap();
        assert_eq!(clipboard_count(&conn), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Same dedup contract for `usage`: `identity` is the PRIMARY KEY, so a
    /// duplicated recent identity is ignored and keeps its first count.
    #[test]
    fn migrate_usage_deduplicates_duplicate_recent_identities() {
        let dir = test_dir("migrate-usage-dedup");
        let mut frequencies = HashMap::new();
        frequencies.insert("x".to_string(), 3u32);
        migrate_usage(
            &dir,
            &["x".to_string(), "x".to_string(), "y".to_string()],
            &frequencies,
        )
        .unwrap();

        let conn = open(&dir).unwrap();
        let (count, x_count): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MAX(count), 0) FROM usage WHERE identity = 'x'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(x_count, 3);
        assert_eq!(usage_recent(&dir, 10).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn snippets_crud_operations() {
        let dir = test_dir("snippets-crud");
        assert!(!snippet_has_data(&dir));

        let id = snippet_insert(
            &dir,
            "Greeting",
            ":hi",
            "Hello, world!",
            &["welcome".to_string(), "intro".to_string()],
        )
        .unwrap();
        assert!(snippet_has_data(&dir));

        let loaded = snippets_load(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].title, "Greeting");
        assert_eq!(loaded[0].prefix, ":hi");
        assert_eq!(loaded[0].content, "Hello, world!");
        assert_eq!(loaded[0].tags, vec!["welcome", "intro"]);

        snippet_update(
            &dir,
            id,
            "Greeting Updated",
            ":hello",
            "Hello, updated world!",
            &["greeting".to_string()],
        )
        .unwrap();

        let updated = snippets_load(&dir);
        assert_eq!(updated[0].title, "Greeting Updated");
        assert_eq!(updated[0].prefix, ":hello");
        assert_eq!(updated[0].content, "Hello, updated world!");
        assert_eq!(updated[0].tags, vec!["greeting"]);

        snippet_delete(&dir, id).unwrap();
        assert!(!snippet_has_data(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrate_snippets_creates_valid_rows() {
        let dir = test_dir("snippets-migrate");
        let entries = vec![
            ("Snippet 1".to_string(), "echo 1".to_string(), vec!["tag1".to_string()]),
            ("Snippet 2".to_string(), "echo 2".to_string(), vec![]),
        ];

        migrate_snippets(&dir, &entries).unwrap();
        assert!(snippet_has_data(&dir));

        let loaded = snippets_load(&dir);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "Snippet 1");
        assert_eq!(loaded[0].content, "echo 1");
        assert_eq!(loaded[0].tags, vec!["tag1"]);
        assert_eq!(loaded[1].title, "Snippet 2");
        assert_eq!(loaded[1].content, "echo 2");
        assert!(loaded[1].tags.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}
