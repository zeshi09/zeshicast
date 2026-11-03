use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::Arc;

use rusqlite::{Connection, Result, params};

use super::crypto::Cipher;

const CURRENT_SCHEMA_VERSION: i64 = 6;

fn open(config_dir: &Path) -> Result<Connection> {
    if let Err(error) = std::fs::create_dir_all(config_dir) {
        log::warn!("could not create {}: {error}", config_dir.display());
    }
    let db_path = config_dir.join("zeshicast.db");
    let mut conn = Connection::open(&db_path)?;
    // P4.4: this used to be a silent `let _ =`. The database holds clipboard
    // history, so failing to restrict it to 0600 is worth a journal entry.
    if let Err(error) = secure_database_permissions(&db_path) {
        log::warn!("could not restrict {} to 0600: {error}", db_path.display());
    }
    // Q6.3: freed pages are zeroed, so a value that was rewritten sealed (or
    // deleted) does not linger readable in the file.
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA secure_delete = ON;")?;
    init(&mut conn)?;
    secure_sidecar_permissions(&db_path);
    Ok(conn)
}

#[cfg(unix)]
fn secure_database_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn secure_database_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// WAL/SHM sidecars carry the same clipboard history as the database, so they
/// need the same 0600. SQLite usually inherits the DB's permissions, but that is
/// not guaranteed across versions (N-7).
fn secure_sidecar_permissions(db_path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let mut name = db_path.as_os_str().to_os_string();
        name.push(suffix);
        let sidecar = std::path::PathBuf::from(name);
        if sidecar.exists()
            && let Err(error) = secure_database_permissions(&sidecar)
        {
            log::warn!("could not restrict {} to 0600: {error}", sidecar.display());
        }
    }
}

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
            2 => {
                migrate_2_to_3(&transaction)?;
                version = 3;
            }
            3 => {
                migrate_3_to_4(&transaction)?;
                version = 4;
            }
            4 => {
                migrate_4_to_5(&transaction)?;
                version = 5;
            }
            5 => {
                migrate_5_to_6(&transaction)?;
                version = 6;
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

fn migrate_2_to_3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS extension_audit (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            extension  TEXT    NOT NULL,
            capability TEXT    NOT NULL,
            detail     TEXT    NOT NULL DEFAULT '',
            outcome    TEXT    NOT NULL,
            at         INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_extension_audit_extension ON extension_audit(extension, id);",
    )
}

/// Q6.1: the file index survives restarts. `path` is the raw OS bytes, so a
/// non-UTF-8 name reopens exactly; `mtime` (nanoseconds) is kept for folders
/// and tells which ones changed while the daemon was not running.
fn migrate_3_to_4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS files (
            path       BLOB    NOT NULL PRIMARY KEY,
            name_lower TEXT    NOT NULL,
            is_dir     INTEGER NOT NULL,
            mtime      INTEGER NOT NULL DEFAULT 0
        );",
    )
}

/// Q6.3: clipboard and snippet values are sealed (see `services::crypto`).
/// `meta` remembers that a key was set up; `text_hash` (a keyed
/// fingerprint) replaces the plaintext UNIQUE lookup for de-duplication.
/// Existing rows are sealed by the first keyed open, not here: a migration
/// runs without the key.
fn migrate_4_to_5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT NOT NULL PRIMARY KEY,
            value BLOB NOT NULL
        );
        ALTER TABLE clipboard ADD COLUMN text_hash BLOB;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_text_hash ON clipboard(text_hash);",
    )
}

/// Q6.4: a clipboard entry keeps its original format next to the text it
/// is searched and shown by: `text/html` for rich text, `text/uri-list` for
/// copied files. `extra` is sealed like `text`.
fn migrate_5_to_6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE clipboard ADD COLUMN extra_mime TEXT;
         ALTER TABLE clipboard ADD COLUMN extra BLOB;",
    )
}

fn meta_get(conn: &Connection, key: &str) -> Option<Vec<u8>> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

fn meta_set(conn: &Connection, key: &str, value: &[u8]) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}

fn key_error(error: super::db_key::KeyError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn seal_error(error: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn open_error(error: super::crypto::CryptoError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(error.to_string().into())
}

/// A connection plus the database's cipher; seals rows still stored in
/// plaintext (written before Q6.3, or by a legacy import) on the way.
fn open_keyed(config_dir: &Path) -> Result<(Connection, Arc<Cipher>)> {
    let conn = open(config_dir)?;
    let cipher =
        super::db_key::cipher_for(config_dir, meta_get(&conn, "key_check"), |check, store| {
            meta_set(&conn, "key_check", check)
                .and_then(|()| meta_set(&conn, "key_store", store.label().as_bytes()))
                .map_err(std::io::Error::other)
        })
        .map_err(key_error)?;
    seal_plaintext_rows(&conn, &cipher)?;
    Ok((conn, cipher))
}

fn seal_plaintext_rows(conn: &Connection, cipher: &Cipher) -> Result<()> {
    let clipboard: Vec<(i64, String)> = conn
        .prepare("SELECT id, text FROM clipboard WHERE typeof(text) = 'text'")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_>>()?;
    let snippets: Vec<(i64, [String; 4])> = conn
        .prepare(
            "SELECT id, title, prefix, content, tags FROM snippets WHERE typeof(content) = 'text'",
        )?
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                [row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?],
            ))
        })?
        .collect::<Result<_>>()?;
    if clipboard.is_empty() && snippets.is_empty() {
        return Ok(());
    }
    for (id, text) in &clipboard {
        conn.execute(
            "UPDATE clipboard SET text = ?1, text_hash = ?2 WHERE id = ?3",
            params![
                cipher.seal(text.as_bytes()).map_err(seal_error)?,
                cipher.fingerprint(text.as_bytes()),
                id
            ],
        )?;
    }
    for (id, fields) in &snippets {
        let sealed = fields
            .iter()
            .map(|field| cipher.seal(field.as_bytes()).map_err(seal_error))
            .collect::<Result<Vec<_>>>()?;
        conn.execute(
            "UPDATE snippets SET title = ?1, prefix = ?2, content = ?3, tags = ?4 WHERE id = ?5",
            params![sealed[0], sealed[1], sealed[2], sealed[3], id],
        )?;
    }
    // The plaintext versions are gone from the pages (`secure_delete`); also
    // drop them from the WAL.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

/// Delete the encrypted data and its key, so the next start sets up a new
/// one: the way out when the key is lost (`--reset-encrypted-data`).
pub fn reset_encrypted_data(config_dir: &Path) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute_batch(
        "DELETE FROM clipboard;
         DELETE FROM snippets;
         DELETE FROM meta WHERE key IN ('key_check', 'key_store');
         PRAGMA wal_checkpoint(TRUNCATE);",
    )?;
    drop(conn);
    super::db_key::delete_key(config_dir);
    Ok(())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Clipboard ────────────────────────────────────────────────────────────────

/// Newest first. Fails (rather than returning a partial list) when the key
/// is missing or does not open a row.
pub fn clipboard_load_with_limit(config_dir: &Path, limit: usize) -> Result<Vec<(String, i64)>> {
    let (conn, cipher) = open_keyed(config_dir)?;
    let mut stmt =
        conn.prepare("SELECT text, added_at FROM clipboard ORDER BY added_at DESC LIMIT ?1")?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(sealed, added_at)| Ok((cipher.open_text(&sealed).map_err(open_error)?, added_at)))
        .collect()
}

#[cfg(test)]
pub fn clipboard_insert_with_limit(config_dir: &Path, text: &str, limit: usize) -> Result<()> {
    clipboard_insert_entry(config_dir, text, None, limit)
}

/// Insert (or move to the top) `text`, with its original format when it
/// had one; a later copy of the same text replaces the format.
pub fn clipboard_insert_entry(
    config_dir: &Path,
    text: &str,
    extra: Option<(&str, &[u8])>,
    limit: usize,
) -> Result<()> {
    let (conn, cipher) = open_keyed(config_dir)?;
    let extra_sealed = match extra {
        Some((_, data)) => Some(cipher.seal(data).map_err(seal_error)?),
        None => None,
    };
    conn.execute(
        "INSERT INTO clipboard (text, text_hash, added_at, extra_mime, extra)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(text_hash) DO UPDATE SET text = excluded.text,
             added_at = excluded.added_at, extra_mime = excluded.extra_mime,
             extra = excluded.extra",
        params![
            cipher.seal(text.as_bytes()).map_err(seal_error)?,
            cipher.fingerprint(text.as_bytes()),
            now(),
            extra.map(|(mime, _)| mime),
            extra_sealed
        ],
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

/// The original format stored with `text`: `(mime, bytes)`.
pub fn clipboard_extra(config_dir: &Path, text: &str) -> Result<Option<(String, Vec<u8>)>> {
    let (conn, cipher) = open_keyed(config_dir)?;
    let row: Option<(Option<String>, Option<Vec<u8>>)> = conn
        .query_row(
            "SELECT extra_mime, extra FROM clipboard WHERE text_hash = ?1",
            params![cipher.fingerprint(text.as_bytes())],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    match row {
        Some((Some(mime), Some(sealed))) => {
            Ok(Some((mime, cipher.open(&sealed).map_err(open_error)?)))
        }
        _ => Ok(None),
    }
}

pub fn clipboard_delete(config_dir: &Path, text: &str) -> Result<()> {
    let (conn, cipher) = open_keyed(config_dir)?;
    conn.execute(
        "DELETE FROM clipboard WHERE text_hash = ?1",
        params![cipher.fingerprint(text.as_bytes())],
    )?;
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

// ── Extension audit log (Q5.4) ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    pub extension: String,
    pub capability: String,
    pub detail: String,
    pub outcome: String,
    pub at: i64,
}

/// Append one row and keep only the newest `limit`. The log is written
/// whatever the private mode says: it is a security record, not history.
pub fn audit_insert(
    config_dir: &Path,
    extension: &str,
    capability: &str,
    detail: &str,
    outcome: &str,
    limit: usize,
) -> Result<()> {
    let conn = open(config_dir)?;
    conn.execute(
        "INSERT INTO extension_audit (extension, capability, detail, outcome, at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![extension, capability, detail, outcome, now()],
    )?;
    conn.execute(
        "DELETE FROM extension_audit WHERE id NOT IN (
            SELECT id FROM extension_audit ORDER BY id DESC LIMIT ?1
        )",
        params![limit as i64],
    )?;
    Ok(())
}

/// Newest first; `extension = None` reads every extension's rows.
pub fn audit_recent(config_dir: &Path, extension: Option<&str>, limit: usize) -> Vec<AuditRow> {
    let Ok(conn) = open(config_dir) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare(
        "SELECT extension, capability, detail, outcome, at FROM extension_audit
         WHERE ?1 IS NULL OR extension = ?1 ORDER BY id DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![extension, limit as i64], |row| {
        Ok(AuditRow {
            extension: row.get(0)?,
            capability: row.get(1)?,
            detail: row.get(2)?,
            outcome: row.get(3)?,
            at: row.get(4)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

// ── File index (Q6.1) ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub mtime: i64,
}

fn path_bytes(path: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

pub fn files_load(config_dir: &Path) -> Vec<FileRow> {
    use std::os::unix::ffi::OsStringExt;
    let Ok(conn) = open(config_dir) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare("SELECT path, is_dir, mtime FROM files") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| {
        Ok(FileRow {
            path: std::ffi::OsString::from_vec(row.get::<_, Vec<u8>>(0)?).into(),
            is_dir: row.get(1)?,
            mtime: row.get(2)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

/// Upsert `rows` and delete `removed` in one transaction; `replace` empties
/// the table first (a full rebuild).
pub fn files_apply(
    config_dir: &Path,
    rows: &[FileRow],
    removed: &[std::path::PathBuf],
    replace: bool,
) -> Result<()> {
    let mut conn = open(config_dir)?;
    let transaction = conn.transaction()?;
    if replace {
        transaction.execute("DELETE FROM files", [])?;
    }
    {
        let mut delete = transaction.prepare("DELETE FROM files WHERE path = ?1")?;
        for path in removed {
            delete.execute(params![path_bytes(path)])?;
        }
        let mut upsert = transaction.prepare(
            "INSERT INTO files (path, name_lower, is_dir, mtime) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET is_dir = ?3, mtime = ?4",
        )?;
        for row in rows {
            let name_lower = row
                .path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            upsert.execute(params![
                path_bytes(&row.path),
                name_lower,
                row.is_dir,
                row.mtime
            ])?;
        }
    }
    transaction.commit()
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

/// Every snippet, decrypted. Fails when the key is missing or wrong.
pub fn snippets_load(config_dir: &Path) -> Result<Vec<SnippetRecord>> {
    let (conn, cipher) = open_keyed(config_dir)?;
    let mut stmt =
        conn.prepare("SELECT id, title, prefix, content, tags FROM snippets ORDER BY id ASC")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                [
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ],
            ))
        })?
        .collect::<Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, sealed)| {
            let [title, prefix, content, tags] = sealed;
            let open = |blob: &[u8]| cipher.open_text(blob).map_err(open_error);
            let tags = open(&tags)?
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            Ok(SnippetRecord {
                id,
                title: open(&title)?,
                prefix: open(&prefix)?,
                content: open(&content)?,
                tags,
            })
        })
        .collect()
}

fn seal_snippet(
    cipher: &Cipher,
    title: &str,
    prefix: &str,
    content: &str,
    tags: &[String],
) -> Result<[Vec<u8>; 4]> {
    let seal = |text: &str| cipher.seal(text.as_bytes()).map_err(seal_error);
    Ok([
        seal(title)?,
        seal(prefix)?,
        seal(content)?,
        seal(&tags.join(", "))?,
    ])
}

pub fn snippet_insert(
    config_dir: &Path,
    title: &str,
    prefix: &str,
    content: &str,
    tags: &[String],
) -> Result<i64> {
    let (conn, cipher) = open_keyed(config_dir)?;
    let [title, prefix, content, tags] = seal_snippet(&cipher, title, prefix, content, tags)?;
    let ts = now();
    conn.execute(
        "INSERT INTO snippets (title, prefix, content, tags, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![title, prefix, content, tags, ts, ts],
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
    let (conn, cipher) = open_keyed(config_dir)?;
    let [title, prefix, content, tags] = seal_snippet(&cipher, title, prefix, content, tags)?;
    conn.execute(
        "UPDATE snippets SET title = ?1, prefix = ?2, content = ?3, tags = ?4, updated_at = ?5 WHERE id = ?6",
        params![title, prefix, content, tags, now(), id],
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
    // Sealed values differ per row, so match after decrypting.
    let ids: Vec<i64> = snippets_load(config_dir)?
        .into_iter()
        .filter(|snippet| snippet.title == title && snippet.content == content)
        .map(|snippet| snippet.id)
        .collect();
    let conn = open(config_dir)?;
    for id in ids {
        conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
    }
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
        clipboard_insert_with_limit(&dir, "secret", 100).unwrap();

        let mode = std::fs::metadata(dir.join("zeshicast.db"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        // WAL/SHM sidecars (created while the connection is open) must match.
        let conn = open(&dir).unwrap();
        conn.execute(
            "INSERT INTO clipboard (text, added_at) VALUES (?1, ?2)",
            params!["sidecar-secret", now()],
        )
        .unwrap();
        for suffix in ["-wal", "-shm"] {
            let path = dir.join(format!("zeshicast.db{suffix}"));
            let meta = std::fs::metadata(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(
                meta.permissions().mode() & 0o777,
                0o600,
                "{}",
                path.display()
            );
        }
        drop(conn);
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

    /// Every byte of the database and its sidecars, as someone who copied
    /// the files (but not the key) would see them.
    fn raw_database(dir: &Path) -> Vec<u8> {
        ["zeshicast.db", "zeshicast.db-wal", "zeshicast.db-shm"]
            .iter()
            .flat_map(|name| std::fs::read(dir.join(name)).unwrap_or_default())
            .collect()
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }

    #[test]
    fn the_database_file_does_not_reveal_clipboard_or_snippets() {
        let dir = test_dir("sealed");
        clipboard_insert_with_limit(&dir, "token-ghp_SECRETCLIP", 100).unwrap();
        snippet_insert(
            &dir,
            "Bank PIN",
            ":pin",
            "SECRETSNIPPET 4821",
            &["private".into()],
        )
        .unwrap();
        assert_eq!(
            clipboard_load_with_limit(&dir, 10).unwrap()[0].0,
            "token-ghp_SECRETCLIP"
        );
        assert_eq!(
            snippets_load(&dir).unwrap()[0].content,
            "SECRETSNIPPET 4821"
        );

        let raw = raw_database(&dir);
        for secret in ["SECRETCLIP", "SECRETSNIPPET", "Bank PIN", ":pin", "private"] {
            assert!(
                !contains(&raw, secret),
                "`{secret}` is readable in the file"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_clipboard_entry_keeps_its_original_format_sealed() {
        let dir = test_dir("extra-format");
        let html = "<b>QUARTERLYHTML</b>";
        clipboard_insert_entry(&dir, "QUARTERLY", Some(("text/html", html.as_bytes())), 100)
            .unwrap();
        clipboard_insert_with_limit(&dir, "plain only", 100).unwrap();
        assert_eq!(
            clipboard_extra(&dir, "QUARTERLY").unwrap(),
            Some(("text/html".to_string(), html.as_bytes().to_vec()))
        );
        assert_eq!(clipboard_extra(&dir, "plain only").unwrap(), None);
        assert!(!contains(&raw_database(&dir), "QUARTERLYHTML"));

        // Copied again as plain text: the old format does not stick.
        clipboard_insert_with_limit(&dir, "QUARTERLY", 100).unwrap();
        assert_eq!(clipboard_extra(&dir, "QUARTERLY").unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicates_and_deletes_work_on_sealed_text() {
        let dir = test_dir("sealed-dedupe");
        clipboard_insert_with_limit(&dir, "same", 100).unwrap();
        clipboard_insert_with_limit(&dir, "other", 100).unwrap();
        clipboard_insert_with_limit(&dir, "same", 100).unwrap();
        let texts: Vec<_> = clipboard_load_with_limit(&dir, 10)
            .unwrap()
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        clipboard_delete(&dir, "same").unwrap();
        assert_eq!(clipboard_load_with_limit(&dir, 10).unwrap().len(), 1);

        let id = snippet_insert(&dir, "t", "", "c", &[]).unwrap();
        snippet_insert(&dir, "keep", "", "c", &[]).unwrap();
        snippet_delete_by_title_and_content(&dir, "t", "c").unwrap();
        let left = snippets_load(&dir).unwrap();
        assert_eq!(left.len(), 1);
        assert_ne!(left[0].id, id);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plaintext_rows_from_before_encryption_are_read_and_sealed() {
        let dir = test_dir("sealed-legacy");
        std::fs::create_dir_all(&dir).unwrap();
        {
            // A schema-4 database: clipboard and snippets in plaintext.
            let conn = Connection::open(dir.join("zeshicast.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE clipboard (id INTEGER PRIMARY KEY AUTOINCREMENT,
                     text TEXT NOT NULL UNIQUE, added_at INTEGER NOT NULL);
                 CREATE TABLE usage (identity TEXT NOT NULL PRIMARY KEY,
                     last_used INTEGER NOT NULL, count INTEGER NOT NULL DEFAULT 1);
                 CREATE TABLE snippets (id INTEGER PRIMARY KEY AUTOINCREMENT,
                     title TEXT NOT NULL, prefix TEXT NOT NULL DEFAULT '',
                     content TEXT NOT NULL, tags TEXT NOT NULL DEFAULT '',
                     created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                 CREATE TABLE extension_audit (id INTEGER PRIMARY KEY AUTOINCREMENT,
                     extension TEXT NOT NULL, capability TEXT NOT NULL,
                     detail TEXT NOT NULL DEFAULT '', outcome TEXT NOT NULL, at INTEGER NOT NULL);
                 CREATE TABLE files (path BLOB NOT NULL PRIMARY KEY, name_lower TEXT NOT NULL,
                     is_dir INTEGER NOT NULL, mtime INTEGER NOT NULL DEFAULT 0);
                 INSERT INTO clipboard (text, added_at) VALUES ('OLDCLIPVALUE', 1);
                 INSERT INTO snippets (title, prefix, content, tags, created_at, updated_at)
                     VALUES ('Old', ':o', 'OLDSNIPVALUE', 'a, b', 1, 1);
                 PRAGMA user_version = 4;",
            )
            .unwrap();
        }
        assert_eq!(
            clipboard_load_with_limit(&dir, 10).unwrap()[0].0,
            "OLDCLIPVALUE"
        );
        let snippet = &snippets_load(&dir).unwrap()[0];
        assert_eq!(
            (snippet.content.as_str(), snippet.tags.len()),
            ("OLDSNIPVALUE", 2)
        );

        let raw = raw_database(&dir);
        assert!(
            !contains(&raw, "OLDCLIPVALUE"),
            "the old plaintext is gone from the file"
        );
        assert!(!contains(&raw, "OLDSNIPVALUE"));
        // Still de-duplicated against the sealed copy.
        clipboard_insert_with_limit(&dir, "OLDCLIPVALUE", 100).unwrap();
        assert_eq!(clipboard_load_with_limit(&dir, 10).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_its_key_the_database_gives_nothing_back() {
        let dir = test_dir("sealed-lost");
        clipboard_insert_with_limit(&dir, "secret", 100).unwrap();
        snippet_insert(&dir, "s", "", "secret", &[]).unwrap();
        super::super::db_key::forget(&dir);
        std::fs::remove_file(dir.join(super::super::db_key::KEY_FILE)).unwrap();

        let error = clipboard_load_with_limit(&dir, 10).unwrap_err().to_string();
        assert!(error.contains("--reset-encrypted-data"), "{error}");
        assert!(snippets_load(&dir).is_err());
        assert!(
            clipboard_insert_with_limit(&dir, "new", 100).is_err(),
            "no new key is made over data sealed with the old one"
        );
        assert!(!dir.join(super::super::db_key::KEY_FILE).exists());

        reset_encrypted_data(&dir).unwrap();
        clipboard_insert_with_limit(&dir, "fresh start", 100).unwrap();
        assert_eq!(
            clipboard_load_with_limit(&dir, 10).unwrap()[0].0,
            "fresh start"
        );
        assert!(snippets_load(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_rows_round_trip_including_non_utf8_names() {
        use std::os::unix::ffi::OsStrExt;
        let dir = test_dir("files-rows");
        let odd = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/u/caf\xe9.txt"));
        let rows = vec![
            FileRow {
                path: "/home/u/Downloads".into(),
                is_dir: true,
                mtime: 42,
            },
            FileRow {
                path: odd.clone(),
                is_dir: false,
                mtime: 0,
            },
        ];
        files_apply(&dir, &rows, &[], true).unwrap();
        let mut loaded = files_load(&dir);
        loaded.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().any(|row| row.path == odd));

        files_apply(&dir, &[], &[odd], false).unwrap();
        assert_eq!(files_load(&dir).len(), 1);
        files_apply(&dir, &[], &[], true).unwrap();
        assert!(files_load(&dir).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn audit_log_keeps_the_newest_rows_per_retention() {
        let dir = test_dir("audit-retention");
        for i in 0..7 {
            audit_insert(&dir, "ext.a", "shell", &format!("run {i}"), "granted", 5).unwrap();
        }
        audit_insert(&dir, "ext.b", "open_url", "https://x", "denied", 5).unwrap();

        let all = audit_recent(&dir, None, 100);
        assert_eq!(all.len(), 5, "pruned to the limit");
        assert_eq!(all[0].extension, "ext.b", "newest first");
        assert_eq!(all[0].outcome, "denied");
        assert_eq!(all[4].detail, "run 3", "the oldest rows went first");

        let only_a = audit_recent(&dir, Some("ext.a"), 100);
        assert_eq!(only_a.len(), 4);
        assert!(only_a.iter().all(|row| row.extension == "ext.a"));
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
        clipboard_insert_with_limit(&dir, "only", 100).unwrap();

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
        migrate_clipboard(&dir, &["a".to_string(), "a".to_string(), "b".to_string()]).unwrap();

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

        let loaded = snippets_load(&dir).unwrap();
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

        let updated = snippets_load(&dir).unwrap();
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
            (
                "Snippet 1".to_string(),
                "echo 1".to_string(),
                vec!["tag1".to_string()],
            ),
            ("Snippet 2".to_string(), "echo 2".to_string(), vec![]),
        ];

        migrate_snippets(&dir, &entries).unwrap();
        assert!(snippet_has_data(&dir));

        let loaded = snippets_load(&dir).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "Snippet 1");
        assert_eq!(loaded[0].content, "echo 1");
        assert_eq!(loaded[0].tags, vec!["tag1"]);
        assert_eq!(loaded[1].title, "Snippet 2");
        assert_eq!(loaded[1].content, "echo 2");
        assert!(loaded[1].tags.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failing_to_restrict_the_database_is_reported_not_swallowed() {
        // The failure used to disappear into `let _ =`; now it reaches the
        // caller, which logs it.
        assert!(
            secure_database_permissions(Path::new("/nonexistent/zeshicast.db")).is_err(),
            "a missing file is an error"
        );

        let dir = test_dir("db-permissions");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("zeshicast.db");
        std::fs::write(&file, b"").unwrap();
        assert!(secure_database_permissions(&file).is_ok());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
