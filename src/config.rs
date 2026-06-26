use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn export_config(config_dir: &Path, dest: &Path) -> io::Result<()> {
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
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{alias} = {target}")
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    Ok(())
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
