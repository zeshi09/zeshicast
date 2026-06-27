use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn export_config(config_dir: &Path, dest: &Path) -> io::Result<()> {
    let preferences = load_preferences(&config_dir.join("preferences.toml"));
    let include_secrets = preference_bool(&preferences, "export_include_secrets", false);
    export_config_with_options(config_dir, dest, include_secrets)
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
    let status = Command::new("tar")
        .args(["-czf"])
        .arg(dest)
        .arg("-C")
        .arg(config_dir.parent().unwrap_or(config_dir))
        .arg(config_dir.file_name().unwrap_or_default())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("tar export failed"))
    }
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
    let mut preferences = load_preferences(path);
    preferences.retain(|key, _| !is_secret_preference_key(key));
    preferences.remove("export_include_secrets");
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
    let staging = parent.join(format!(
        ".zeshicast-import-{}-{}",
        std::process::id(),
        unix_now()
    ));
    // Clean any stale staging dir, then extract into it.
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let extract = || -> io::Result<()> {
        let status = Command::new("tar")
            .args(["-xzf"])
            .arg(src)
            .args(["--no-same-owner", "-C"])
            .arg(&staging)
            .status()?;
        if !status.success() {
            return Err(io::Error::other("tar import failed"));
        }
        // The validated archive has a single `zeshicast/` root.
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
    };

    let result = extract();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Reject archives whose members are absolute, contain a `..` component, or sit
/// outside a single top-level `zeshicast/` directory.
fn validate_archive_members(src: &Path) -> io::Result<()> {
    let output = Command::new("tar").args(["-tzf"]).arg(src).output()?;
    if !output.status.success() {
        return Err(io::Error::other("could not read archive"));
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let mut saw_member = false;
    for raw in listing.lines() {
        let member = raw.trim_end_matches('/').trim();
        if member.is_empty() {
            continue;
        }
        saw_member = true;
        let path = Path::new(member);
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
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        content.push_str(&format!("{key} = \"{escaped}\"\n"));
    }
    write_file_atomic(path, content.as_bytes(), 0o600)
}

pub(crate) fn load_preferences(path: &Path) -> HashMap<String, String> {
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(table) = content.parse::<toml::Table>() else {
        eprintln!("failed to parse preferences: {}", path.display());
        return HashMap::new();
    };

    table
        .iter()
        .filter_map(|(key, value)| toml_value_string(value).map(|value| (key.clone(), value)))
        .collect()
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
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_now_nanos()
    ));

    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        set_file_mode(&file, mode)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_path, path)?;
        sync_parent_dir(parent)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(unix)]
fn set_file_mode(file: &fs::File, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_file_mode(_file: &fs::File, _mode: u32) -> io::Result<()> {
    Ok(())
}

fn sync_parent_dir(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

fn unix_now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
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
            unix_now_nanos()
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
        let preferences = load_preferences(&path);

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
