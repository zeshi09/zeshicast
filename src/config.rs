use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

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
        Err(io::Error::new(io::ErrorKind::Other, "tar export failed"))
    }
}

pub fn import_config(src: &Path, config_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(config_dir)?;
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(src)
        .arg("-C")
        .arg(config_dir.parent().unwrap_or(config_dir))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "tar import failed"))
    }
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

pub(crate) fn write_frequencies(path: &Path, frequencies: &HashMap<String, u32>) -> io::Result<()> {
    let mut entries: Vec<_> = frequencies.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let lines: Vec<String> = entries.iter().map(|(k, v)| format!("{k}:{v}")).collect();
    write_lines(path, &lines)
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
