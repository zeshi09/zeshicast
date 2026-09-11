use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const EXPORT_SECRETS_PREFERENCE_KEY: &str = "export_include_secrets";

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
        copy_config_sanitized(config_dir, &staged_config)?;
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
    let root_name = config_dir.file_name().unwrap_or_default();
    builder.append_dir_all(root_name, config_dir)?;
    builder.finish()?;
    Ok(())
}

fn copy_config_sanitized(src: &Path, dest: &Path) -> io::Result<()> {
    if !src.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_config_sanitized(&path, &dest_path)?;
        } else if file_type.is_file() {
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
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
    // Untrusted archive: validate the member list *before* extracting, extract
    // into an isolated staging dir, reject symlinks, then atomically swap. This
    // prevents path traversal (`../`, absolute paths) and symlink write-through
    // from clobbering files outside `config_dir`.
    validate_archive_members(src)?;

    let parent = config_dir.parent().unwrap_or(config_dir);
    fs::create_dir_all(parent)?;
    let temp_staging = tempfile::Builder::new()
        .prefix(".zeshicast-import-")
        .tempdir_in(parent)?;
    let staging = temp_staging.path();

    let file = fs::File::open(src)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(staging)?;

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

/// Reject archives whose members are absolute, contain a `..` component, or sit
/// outside a single top-level `zeshicast/` directory.
fn validate_archive_members(src: &Path) -> io::Result<()> {
    let file = fs::File::open(src)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut saw_member = false;

    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?;
        let raw = path.to_string_lossy();
        let member = raw.trim_end_matches('/').trim();
        if member.is_empty() {
            continue;
        }
        saw_member = true;
        if path.is_absolute() {
            return Err(io::Error::other(format!("unsafe absolute path: {member}")));
        }
        use std::path::Component;
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
        if entry.header().entry_type().is_symlink() || entry.header().entry_type().is_hard_link() {
            return Err(io::Error::other(format!(
                "archive contains a symlink/hardlink: {member}"
            )));
        }
    }
    if !saw_member {
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
            eprintln!("failed to read preferences: {error}");
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
                Ok(Some(backup)) => eprintln!(
                    "failed to read preferences: {error}; using defaults and refusing to write (backup: {})",
                    backup.display()
                ),
                Ok(None) => eprintln!(
                    "failed to read preferences: {error}; using defaults and refusing to write (backup not needed)"
                ),
                Err(backup_error) => eprintln!(
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
}
