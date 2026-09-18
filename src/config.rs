use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const EXPORT_SECRETS_PREFERENCE_KEY: &str = "export_include_secrets";
const EXPORT_HISTORY_PREFERENCE_KEY: &str = "export_include_history";

/// Resolve the `--include-history` CLI flag against the global preferences.
///
/// Clipboard/usage history (`zeshicast.db`, `calc_history.json`) is *not*
/// exported by default: a "safe" export is meant to be shareable (M-12).
pub fn resolve_include_history(
    cli_flag: Option<bool>,
    preferences: &HashMap<String, String>,
) -> bool {
    cli_flag.unwrap_or_else(|| preference_bool(preferences, EXPORT_HISTORY_PREFERENCE_KEY, false))
}

/// Resolve the `--include-secrets` CLI flag against the global preferences.
/// An explicit CLI value wins; without one the `export_include_secrets`
/// preference decides (missing/unparseable preference means secrets excluded).
pub fn resolve_include_secrets(
    cli_flag: Option<bool>,
    preferences: &HashMap<String, String>,
) -> bool {
    cli_flag.unwrap_or_else(|| preference_bool(preferences, EXPORT_SECRETS_PREFERENCE_KEY, false))
}

/// Load the global preferences.toml used by CLI export resolution.
pub fn load_global_preferences(config_dir: &Path) -> HashMap<String, String> {
    load_preferences_or_default(&config_dir.join("preferences.toml"))
}

pub fn export_config_with_options(
    config_dir: &Path,
    dest: &Path,
    include_secrets: bool,
    include_history: bool,
) -> io::Result<()> {
    if include_secrets {
        return export_config_dir(config_dir, dest);
    }

    let parent = config_dir.parent().unwrap_or(config_dir);
    let staging = parent.join(format!(
        ".zeshicast-export-{}-{}",
        std::process::id(),
        unix_now()
    ));
    let staged_config = staging.join(config_dir.file_name().unwrap_or_default());
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staged_config)?;

    let result = (|| {
        copy_config_sanitized(config_dir, &staged_config, Path::new(""), include_history)?;
        sanitize_export_preferences(&staged_config.join("preferences.toml"))?;
        export_config_dir(&staged_config, dest)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

fn export_config_dir(config_dir: &Path, dest: &Path) -> io::Result<()> {
    let dest_file = fs::File::create(dest)?;
    let enc = flate2::write::GzEncoder::new(dest_file, flate2::Compression::default());
    let mut builder = tar::Builder::new(enc);
    // Append the tree by hand instead of `append_dir_all`: the latter follows
    // symlinks, so a link pointing at `/etc/shadow` would be archived as its
    // target's content (M-12).
    let base = config_dir.parent().unwrap_or(Path::new("/"));
    append_tree(&mut builder, base, config_dir)?;
    builder.finish()?;
    Ok(())
}

/// Archive `dir` (relative to `base`) without ever following a symlink.
fn append_tree<W: io::Write>(
    builder: &mut tar::Builder<W>,
    base: &Path,
    dir: &Path,
) -> io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        // `file_type` comes from the directory entry, so it reports a symlink
        // instead of resolving it.
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(base)
            .map_err(|_| io::Error::other("path outside the config directory"))?
            .to_path_buf();
        let metadata = entry.metadata()?;

        if file_type.is_dir() {
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&metadata);
            builder.append_data(&mut header, &relative, io::empty())?;
            append_tree(builder, base, &path)?;
        } else if file_type.is_file() {
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&metadata);
            let mut file = fs::File::open(&path)?;
            builder.append_data(&mut header, &relative, &mut file)?;
        }
    }
    Ok(())
}

/// Clipboard/usage history files, excluded from a "safe" export.
const HISTORY_FILES: [&str; 4] = [
    "zeshicast.db",
    "zeshicast.db-wal",
    "zeshicast.db-shm",
    "calc_history.json",
];

fn copy_config_sanitized(
    src: &Path,
    dest: &Path,
    relative: &Path,
    include_history: bool,
) -> io::Result<()> {
    if !src.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        let child_relative = relative.join(&name);
        let path = entry.path();
        let dest_path = dest.join(&name);
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_config_sanitized(&path, &dest_path, &child_relative, include_history)?;
        } else if file_type.is_file() {
            if !include_history && HISTORY_FILES.contains(&name.as_str()) {
                continue;
            }
            if is_command_toml(&child_relative) {
                // `[env]` values are frequently tokens, so the safe export
                // drops the table (M-12).
                let content = fs::read_to_string(&path).unwrap_or_default();
                fs::write(&dest_path, strip_env_table(&content))?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }
    }
    Ok(())
}

/// `commands/*.toml` (custom commands) and `extensions/*/extension.toml`.
fn is_command_toml(relative: &Path) -> bool {
    relative.extension().is_some_and(|ext| ext == "toml")
        && matches!(
            relative
                .components()
                .next()
                .and_then(|component| match component {
                    std::path::Component::Normal(name) => name.to_str(),
                    _ => None,
                }),
            Some("commands" | "extensions")
        )
}

/// Remove the `[env]` table (and any `[env.*]` sub-table) from a command TOML.
fn strip_env_table(content: &str) -> String {
    let mut stripped = String::with_capacity(content.len());
    let mut in_env = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_env = trimmed.starts_with("[env]")
                || trimmed.starts_with("[env.")
                || trimmed.starts_with("[ env]");
            if in_env {
                continue;
            }
        }
        if in_env {
            continue;
        }
        stripped.push_str(line);
        stripped.push('\n');
    }
    stripped
}

fn sanitize_export_preferences(path: &Path) -> io::Result<()> {
    // Strict on purpose: a file we cannot parse must not be rewritten as an
    // empty one (the exported copy would silently lose every preference).
    let mut preferences = load_preferences(path)?;
    preferences.retain(|key, _| !is_secret_preference_key(key));
    preferences.remove(EXPORT_SECRETS_PREFERENCE_KEY);
    write_preferences(path, &preferences)
}

fn is_secret_preference_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.ends_with("_api_key")
        || key.contains("secret")
        || key.contains("token")
        || key.contains("password")
}

fn preference_bool(preferences: &HashMap<String, String>, key: &str, default_value: bool) -> bool {
    preferences
        .get(key)
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        })
        .unwrap_or(default_value)
}

pub fn import_config(src: &Path, config_dir: &Path) -> io::Result<()> {
    import_config_with_limits(src, config_dir, ImportLimits::default())
}

/// Limits for importing an untrusted archive (M-11).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImportLimits {
    pub(crate) archive_bytes: u64,
    pub(crate) members: usize,
    pub(crate) unpacked_bytes: u64,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            archive_bytes: 64 * 1024 * 1024,
            members: 4096,
            unpacked_bytes: 256 * 1024 * 1024,
        }
    }
}

pub(crate) fn import_config_with_limits(
    src: &Path,
    config_dir: &Path,
    limits: ImportLimits,
) -> io::Result<()> {
    // Untrusted archive: validate *and* extract from one decoder into an
    // isolated staging dir, reject symlinks, then atomically swap. Validating a
    // member list separately and unpacking it afterwards decoded the archive
    // twice and left a TOCTOU window between the checks and the writes.
    let parent = config_dir.parent().unwrap_or(config_dir);
    fs::create_dir_all(parent)?;
    let temp_staging = tempfile::Builder::new()
        .prefix(".zeshicast-import-")
        .tempdir_in(parent)?;
    let staging = temp_staging.path();

    extract_archive(src, staging, limits)?;

    let imported = staging.join(config_dir.file_name().unwrap_or_default());
    if !imported.is_dir() {
        return Err(io::Error::other("archive missing zeshicast/ directory"));
    }
    reject_symlinks(&imported)?;

    // Swap into place: move the old config aside, promote the import, drop
    // the backup. On failure, restore the backup.
    let backup = parent.join(format!(".zeshicast-backup-{}", unix_now()));
    let had_old = config_dir.exists();
    if had_old {
        fs::rename(config_dir, &backup)?;
    }
    match fs::rename(&imported, config_dir) {
        Ok(()) => {
            if had_old {
                let _ = fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(err) => {
            if had_old {
                let _ = fs::rename(&backup, config_dir);
            }
            Err(err)
        }
    }
}

/// Check one archive member for safety (path traversal, absolute paths, links).
/// Returns `Ok(false)` for members that should be skipped entirely.
fn member_is_extractable(path: &Path, entry_type: tar::EntryType) -> io::Result<()> {
    use std::path::Component;

    let raw = path.to_string_lossy();
    let member = raw.trim_end_matches('/').trim().to_string();
    if member.is_empty() {
        return Ok(());
    }
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        return Err(io::Error::other(format!(
            "archive contains a symlink/hardlink: {member}"
        )));
    }
    if path.is_absolute() {
        return Err(io::Error::other(format!("unsafe absolute path: {member}")));
    }
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(root)) if root == "zeshicast" => {}
        _ => {
            return Err(io::Error::other(format!(
                "member outside zeshicast/: {member}"
            )));
        }
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
    {
        return Err(io::Error::other(format!("unsafe path component: {member}")));
    }
    Ok(())
}

/// Validate and write every member of `src` in one pass, bounded by `limits`.
///
/// The byte counter wraps the *decompressed* stream, so a member whose declared
/// size lies (a decompression bomb) is stopped after `unpacked_bytes + 1` bytes
/// have been written instead of filling the disk.
fn extract_archive(src: &Path, staging: &Path, limits: ImportLimits) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let compressed = fs::metadata(src)?.len();
    if compressed > limits.archive_bytes {
        return Err(io::Error::other(format!(
            "archive is too large: {compressed} bytes > {} bytes",
            limits.archive_bytes
        )));
    }

    let file = fs::File::open(src)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut members = 0usize;
    let mut unpacked: u64 = 0;

    for entry in archive.entries()? {
        let mut entry = entry?;
        members += 1;
        if members > limits.members {
            return Err(io::Error::other(format!(
                "archive has more than {} members",
                limits.members
            )));
        }

        let path = entry.path()?.into_owned();
        let entry_type = entry.header().entry_type();
        member_is_extractable(&path, entry_type)?;
        if path
            .to_string_lossy()
            .trim_end_matches('/')
            .trim()
            .is_empty()
        {
            continue;
        }

        let out_path = staging.join(&path);
        if entry_type.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if !entry_type.is_file() {
            // Devices, FIFOs and sockets are not part of our format.
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let remaining = limits.unpacked_bytes.saturating_sub(unpacked);
        let mut out = fs::File::create(&out_path)?;
        let written = io::copy(&mut (&mut entry).take(remaining + 1), &mut out)?;
        unpacked += written;
        if unpacked > limits.unpacked_bytes {
            return Err(io::Error::other(format!(
                "archive unpacks to more than {} bytes",
                limits.unpacked_bytes
            )));
        }
        let mode = entry.header().mode()? & 0o777;
        if mode != 0 {
            fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
        }
    }

    if members == 0 {
        return Err(io::Error::other("empty archive"));
    }
    Ok(())
}

/// Defense in depth: refuse any symlink in the extracted tree (our exports never
/// contain symlinks, and a symlink could redirect writes outside config_dir).
fn reject_symlinks(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(io::Error::other(format!(
                "archive contains a symlink: {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            reject_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn load_aliases(path: &Path) -> HashMap<String, String> {
    load_lines(path)
        .into_iter()
        .filter_map(|line| {
            let (alias, target) = line.split_once('=')?;
            let alias = normalize_alias(alias.trim());
            if alias.is_empty() {
                return None;
            }
            Some((alias, target.trim().to_string()))
        })
        .collect()
}

pub(crate) fn append_alias(config_dir: &Path, alias: &str, target: &str) -> io::Result<()> {
    fs::create_dir_all(config_dir)?;
    let path = config_dir.join("aliases.txt");
    let mut content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{alias} = {target}\n"));
    write_file_atomic(&path, content.as_bytes(), 0o600)
}

pub(crate) fn normalize_alias(alias: &str) -> String {
    alias
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn write_preferences(
    path: &Path,
    preferences: &HashMap<String, String>,
) -> io::Result<()> {
    let mut content = String::new();
    let mut keys: Vec<&str> = preferences.keys().map(|k| k.as_str()).collect();
    keys.sort();
    for key in keys {
        let value = &preferences[key];
        let escaped = escape_toml_string(value);
        content.push_str(&format!("{key} = \"{escaped}\"\n"));
    }
    write_file_atomic(path, content.as_bytes(), 0o600)
}

/// Escape a value for a TOML basic string (M-10).
///
/// Writing a bare newline, carriage return or control byte used to produce an
/// invalid file, and the next load then saw *no* preferences at all.
fn escape_toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if (control as u32) < 0x20 || control == '\u{7f}' => {
                escaped.push_str(&format!("\\u{:04X}", control as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

/// Load `preferences.toml`, reporting corruption instead of silently ignoring it.
///
/// A missing file is not an error (first run). Anything else — unreadable or
/// unparseable — is returned to the caller so it can refuse to overwrite a file
/// it could not read (M-10).
pub(crate) fn load_preferences(path: &Path) -> io::Result<HashMap<String, String>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    let table = content.parse::<toml::Table>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })?;

    Ok(table
        .iter()
        .filter_map(|(key, value)| toml_value_string(value).map(|value| (key.clone(), value)))
        .collect())
}

/// Best-effort variant for callers that only want a view of the preferences
/// (fonts, CLI defaults): log and fall back to the defaults.
pub(crate) fn load_preferences_or_default(path: &Path) -> HashMap<String, String> {
    match load_preferences(path) {
        Ok(preferences) => preferences,
        Err(error) => {
            log::warn!("failed to read preferences: {error}");
            HashMap::new()
        }
    }
}

/// Load preferences and, when the file is corrupt, keep a timestamped copy
/// before returning the defaults.
///
/// The bool is `false` when the file was corrupt: the caller must then refuse to
/// write, so a single bad edit cannot erase every preference (M-10).
pub(crate) fn load_preferences_with_backup(path: &Path) -> (HashMap<String, String>, bool) {
    match load_preferences(path) {
        Ok(preferences) => (preferences, true),
        Err(error) => {
            let backup = back_up_corrupt_file(path);
            match backup {
                Ok(Some(backup)) => log::warn!(
                    "failed to read preferences: {error}; using defaults and refusing to write (backup: {})",
                    backup.display()
                ),
                Ok(None) => log::warn!(
                    "failed to read preferences: {error}; using defaults and refusing to write (backup not needed)"
                ),
                Err(backup_error) => log::warn!(
                    "failed to read preferences: {error}; using defaults and refusing to write (backup failed: {backup_error})"
                ),
            }
            (HashMap::new(), false)
        }
    }
}

/// Copy a corrupt file aside as `<name>.bad-<ts>`, leaving the original in place
/// so the user can repair it. Returns `None` when the file is already gone, and
/// keeps only the first backup per file to avoid piling up snapshots.
pub(crate) fn back_up_corrupt_file(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preferences.toml".to_string());
    let prefix = format!("{name}.bad-");
    if let Ok(entries) = fs::read_dir(parent)
        && entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(prefix.as_str())
        })
    {
        return Ok(None);
    }

    let backup = path.with_file_name(format!("{name}.bad-{}", unix_now()));
    fs::copy(path, &backup)?;
    Ok(Some(backup))
}

pub(crate) fn load_frequencies(path: &Path) -> HashMap<String, u32> {
    load_lines(path)
        .into_iter()
        .filter_map(|line| {
            let (identity, count) = line.rsplit_once(':')?;
            let count = count.parse::<u32>().ok()?;
            Some((identity.to_string(), count))
        })
        .collect()
}

pub(crate) fn toml_value_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(value) => Some(value.trim().to_string()),
        toml::Value::Integer(value) => Some(value.to_string()),
        toml::Value::Float(value) => Some(value.to_string()),
        toml::Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(crate) fn load_lines(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub(crate) fn write_lines(path: &Path, lines: &[String]) -> io::Result<()> {
    let mut content = Vec::new();
    for line in lines {
        writeln!(&mut content, "{line}")?;
    }
    write_file_atomic(path, &content, 0o600)
}

pub(crate) fn write_file_atomic(path: &Path, content: &[u8], mode: u32) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("path has no file name"))?
        .to_string_lossy();

    let mut temp_file = tempfile::Builder::new()
        .prefix(&format!(".{file_name}.tmp-"))
        .tempfile_in(parent)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp_file
            .as_file_mut()
            .set_permissions(fs::Permissions::from_mode(mode))?;
    }

    temp_file.write_all(content)?;
    temp_file.as_file_mut().sync_all()?;
    temp_file.persist(path).map_err(|e| e.error)?;
    sync_parent_dir(parent)?;
    Ok(())
}

fn sync_parent_dir(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(feature = "gui")]
pub(crate) fn format_time_ago(ts: i64) -> String {
    let now = unix_now();
    let delta = now.saturating_sub(ts);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{} min ago", delta / 60)
    } else if delta < 86400 {
        format!("{} h ago", delta / 3600)
    } else {
        format!("{} d ago", delta / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "zeshicast-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ))
    }

    #[test]
    fn export_preferences_sanitizer_removes_secret_keys() {
        let dir = test_dir("export-sanitize");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.toml");
        write_preferences(
            &path,
            &HashMap::from([
                ("ai_api_key".to_string(), "sk-secret".to_string()),
                ("custom_token".to_string(), "token".to_string()),
                ("db_password".to_string(), "password".to_string()),
                ("ai_model".to_string(), "llama".to_string()),
                ("export_include_secrets".to_string(), "true".to_string()),
            ]),
        )
        .unwrap();

        sanitize_export_preferences(&path).unwrap();
        let preferences = load_preferences(&path).expect("exported preferences parse");

        assert_eq!(
            preferences.get("ai_model").map(String::as_str),
            Some("llama")
        );
        assert!(!preferences.contains_key("ai_api_key"));
        assert!(!preferences.contains_key("custom_token"));
        assert!(!preferences.contains_key("db_password"));
        assert!(!preferences.contains_key("export_include_secrets"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolve_include_secrets_preference_true_without_flag_includes_secrets() {
        let preferences = HashMap::from([(
            EXPORT_SECRETS_PREFERENCE_KEY.to_string(),
            "true".to_string(),
        )]);
        assert!(resolve_include_secrets(None, &preferences));
    }

    #[test]
    fn resolve_include_secrets_explicit_flag_overrides_preference() {
        let preferences = HashMap::from([(
            EXPORT_SECRETS_PREFERENCE_KEY.to_string(),
            "true".to_string(),
        )]);
        assert!(!resolve_include_secrets(Some(false), &preferences));
        assert!(resolve_include_secrets(Some(true), &preferences));
    }

    #[test]
    fn resolve_include_secrets_defaults_to_excluded() {
        assert!(!resolve_include_secrets(None, &HashMap::new()));
        // Unparseable preference values fall back to excluded as well.
        let garbage = HashMap::from([(
            EXPORT_SECRETS_PREFERENCE_KEY.to_string(),
            "maybe".to_string(),
        )]);
        assert!(!resolve_include_secrets(None, &garbage));
    }

    #[cfg(unix)]
    #[test]
    fn write_preferences_creates_0600_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = test_dir("preferences-mode");
        let path = dir.join("preferences.toml");
        write_preferences(
            &path,
            &HashMap::from([("ai_api_key".to_string(), "secret".to_string())]),
        )
        .unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preference_value_with_newline_round_trips() {
        let dir = test_dir("preferences-escape");
        let path = dir.join("preferences.toml");
        let value = "one\ntwo\ttabbed\rreturn \"quoted\" \\ backslash \u{7}\u{8}".to_string();
        write_preferences(
            &path,
            &HashMap::from([("ai_prompt".to_string(), value.clone())]),
        )
        .unwrap();

        // The raw file must stay single-line: a literal newline used to make the
        // whole file invalid TOML, and the next load then saw no preferences at
        // all (M-10).
        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 1, "raw file: {raw:?}");
        assert!(raw.contains("\\n") && raw.contains("\\t") && raw.contains("\\r"));
        assert!(raw.contains("\\u0007") && raw.contains("\\u0008"));

        let loaded = load_preferences(&path).expect("escaped file must parse");
        assert_eq!(loaded.get("ai_prompt"), Some(&value));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_preferences_are_backed_up_not_overwritten() {
        let dir = test_dir("preferences-corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.toml");
        let corrupt = "ai_model = \"llama\nbroken";
        fs::write(&path, corrupt).unwrap();

        let (preferences, writable) = load_preferences_with_backup(&path);
        assert!(preferences.is_empty(), "corrupt values must not be used");
        assert!(!writable, "a corrupt file must not be writable");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            corrupt,
            "the original file must stay in place so the user can fix it"
        );

        let backups: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("preferences.toml.bad-"))
            .collect();
        assert_eq!(backups.len(), 1, "expected one backup, got {backups:?}");
        assert_eq!(fs::read_to_string(dir.join(&backups[0])).unwrap(), corrupt);

        // Reading again must neither add backups nor start writing again.
        let (_, writable_again) = load_preferences_with_backup(&path);
        assert!(!writable_again);
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "original + one backup"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_preferences_file_is_not_an_error() {
        let dir = test_dir("preferences-missing");
        let path = dir.join("preferences.toml");

        let (preferences, writable) = load_preferences_with_backup(&path);
        assert!(preferences.is_empty());
        assert!(writable, "a first run may write preferences");
        assert!(!path.exists(), "nothing to back up yet");
    }

    #[test]
    fn write_lines_atomic_replaces_existing_content() {
        let dir = test_dir("atomic-lines");
        let path = dir.join("pins.txt");
        write_lines(&path, &["old".to_string()]).unwrap();
        write_lines(&path, &["new".to_string(), "other".to_string()]).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new\nother\n");
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_does_not_follow_symlink_target() {
        use std::os::unix::fs::symlink;

        let dir = test_dir("atomic-symlink");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        let link = dir.join("preferences.toml");
        fs::write(&target, b"old-secret").unwrap();
        symlink(&target, &link).unwrap();

        write_file_atomic(&link, b"new-value", 0o600).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"old-secret");
        assert_eq!(fs::read(&link).unwrap(), b"new-value");
        assert!(
            !fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let _ = fs::remove_dir_all(dir);
    }

    /// Build a `.tar.gz` from `(member path, content)` pairs.
    fn write_archive(path: &Path, members: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(enc);
        for (name, content) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *content).unwrap();
        }
        builder.finish().unwrap();
    }

    fn archive_members(path: &Path) -> Vec<(String, Vec<u8>)> {
        let file = fs::File::open(path).unwrap();
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        let mut members = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            members.push((name, content));
        }
        members
    }

    fn archive_names(path: &Path) -> Vec<String> {
        archive_members(path)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    #[test]
    fn import_rejects_oversized_archive() {
        let dir = test_dir("import-size");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("big.tar.gz");
        write_archive(
            &src,
            &[("zeshicast/preferences.toml", b"ai_model = \"llama\"\n")],
        );

        let limits = ImportLimits {
            archive_bytes: 10,
            members: 100,
            unpacked_bytes: 1 << 20,
        };
        let target = dir.join("config");
        let error = import_config_with_limits(&src, &target, limits).unwrap_err();
        assert!(
            error.to_string().contains("archive is too large"),
            "{error}"
        );
        assert!(
            !target.exists(),
            "nothing may be swapped in after a failure"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_rejects_too_many_members() {
        let dir = test_dir("import-members");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("many.tar.gz");
        write_archive(
            &src,
            &[("zeshicast/one.txt", b"1"), ("zeshicast/two.txt", b"2")],
        );

        let limits = ImportLimits {
            archive_bytes: 1 << 20,
            members: 1,
            unpacked_bytes: 1 << 20,
        };
        let error = import_config_with_limits(&src, &dir.join("config"), limits).unwrap_err();
        assert!(error.to_string().contains("members"), "{error}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_rejects_bomb() {
        let dir = test_dir("import-bomb");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("bomb.tar.gz");
        // Compresses to a few bytes but unpacks to far more than the budget the
        // caller allows (M-11).
        let payload = vec![b'a'; 8192];
        write_archive(&src, &[("zeshicast/huge.txt", &payload)]);

        let limits = ImportLimits {
            archive_bytes: 1 << 20,
            members: 16,
            unpacked_bytes: 1024,
        };
        let error = import_config_with_limits(&src, &dir.join("config"), limits).unwrap_err();
        assert!(
            error.to_string().contains("unpacks to more than"),
            "{error}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_rejects_traversal_and_unsafe_members() {
        let dir = test_dir("import-unsafe");
        fs::create_dir_all(&dir).unwrap();
        let limits = ImportLimits::default();

        // A member outside the single `zeshicast/` root.
        let outside = dir.join("outside.tar.gz");
        write_archive(&outside, &[("other/preferences.toml", b"x = 1")]);
        let error = import_config_with_limits(&outside, &dir.join("config-b"), limits).unwrap_err();
        assert!(error.to_string().contains("outside zeshicast/"), "{error}");
        assert!(!dir.join("config-b").exists());

        // Path traversal, absolute paths and links never get that far: the tar
        // crate refuses to *write* such a member, so validate the check itself.
        use tar::EntryType;
        assert!(member_is_extractable(Path::new("zeshicast/p.toml"), EntryType::Regular).is_ok());
        assert!(member_is_extractable(Path::new("zeshicast/"), EntryType::Directory).is_ok());
        for bad in [
            "/etc/passwd",
            "zeshicast/../../escape.txt",
            "../zeshicast/preferences.toml",
            "other/preferences.toml",
        ] {
            let error = member_is_extractable(Path::new(bad), EntryType::Regular)
                .expect_err("unsafe member must be rejected");
            assert!(
                error.to_string().contains("unsafe") || error.to_string().contains("outside"),
                "{bad}: {error}"
            );
        }
        assert!(member_is_extractable(Path::new("zeshicast/link"), EntryType::Symlink).is_err());
        assert!(member_is_extractable(Path::new("zeshicast/link"), EntryType::Link).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn safe_export_excludes_clipboard_db() {
        let dir = test_dir("export-safe");
        let config = dir.join("zeshicast");
        fs::create_dir_all(config.join("commands")).unwrap();
        fs::write(config.join("zeshicast.db"), b"clipboard history").unwrap();
        fs::write(config.join("zeshicast.db-wal"), b"wal").unwrap();
        fs::write(config.join("calc_history.json"), b"[]").unwrap();
        fs::write(
            config.join("preferences.toml"),
            "ai_model = \"llama\"\nai_api_key = \"sk-secret\"\n",
        )
        .unwrap();
        fs::write(
            config.join("commands/deploy.toml"),
            "name = \"Deploy\"\ncommand = \"deploy\"\n\n[env]\nDEPLOY_TOKEN = \"tok\"\n",
        )
        .unwrap();

        let dest = dir.join("safe.tar.gz");
        export_config_with_options(&config, &dest, false, false).unwrap();

        let names = archive_names(&dest);
        assert!(
            names.iter().any(|name| name.ends_with("preferences.toml")),
            "{names:?}"
        );
        for excluded in ["zeshicast.db", "zeshicast.db-wal", "calc_history.json"] {
            assert!(
                !names.iter().any(|name| name.ends_with(excluded)),
                "{excluded} must not be exported: {names:?}"
            );
        }
        let deploy = archive_members(&dest)
            .into_iter()
            .find(|(name, _)| name.ends_with("deploy.toml"))
            .expect("command TOML is exported")
            .1;
        let deploy = String::from_utf8(deploy).unwrap();
        assert!(deploy.contains("name = \"Deploy\""), "{deploy}");
        assert!(!deploy.contains("DEPLOY_TOKEN"), "env leaked: {deploy}");
        assert!(!deploy.contains("[env]"), "env table leaked: {deploy}");

        // `--include-history` keeps the history but still strips secrets.
        let dest_all = dir.join("all.tar.gz");
        export_config_with_options(&config, &dest_all, false, true).unwrap();
        let names = archive_names(&dest_all);
        assert!(
            names.iter().any(|name| name.ends_with("zeshicast.db")),
            "{names:?}"
        );
        let prefs = archive_members(&dest_all)
            .into_iter()
            .find(|(name, _)| name.ends_with("preferences.toml"))
            .expect("preferences are exported")
            .1;
        assert!(!String::from_utf8_lossy(&prefs).contains("sk-secret"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn export_does_not_follow_symlinks() {
        let dir = test_dir("export-symlink");
        let config = dir.join("zeshicast");
        fs::create_dir_all(&config).unwrap();
        let secret = dir.join("outside-secret.txt");
        fs::write(&secret, b"TOP SECRET").unwrap();
        std::os::unix::fs::symlink(&secret, config.join("link-to-secret")).unwrap();
        fs::write(
            config.join("quicklinks.txt"),
            "Docs = https://example.com\n",
        )
        .unwrap();

        // The `--include-secrets` path archives the tree directly (M-12).
        let dest = dir.join("with-secrets.tar.gz");
        export_config_with_options(&config, &dest, true, true).unwrap();

        let members = archive_members(&dest);
        let names: Vec<String> = members.iter().map(|(name, _)| name.clone()).collect();
        assert!(
            !names.iter().any(|name| name.ends_with("link-to-secret")),
            "symlink entry archived: {names:?}"
        );
        assert!(
            !members.iter().any(|(_, content)| content == b"TOP SECRET"),
            "symlink target content leaked into the archive"
        );
        assert!(
            names.iter().any(|name| name.ends_with("quicklinks.txt")),
            "regular files are still exported: {names:?}"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
