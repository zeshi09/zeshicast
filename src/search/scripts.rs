use std::fs;
use std::path::{Path, PathBuf};

use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone)]
pub(crate) struct ScriptEntry {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) icon: String,
    pub(crate) path: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptMode {
    Compact,
    FullOutput,
    Silent,
}

impl ScriptEntry {
    fn search_text(&self) -> String {
        format!("{} {} {}", self.title, self.description, self.package)
    }
}

pub(crate) fn load_script_entries(script_dirs: &[PathBuf]) -> Vec<ScriptEntry> {
    let mut entries = Vec::new();
    for dir in script_dirs {
        let Ok(read_dir) = fs::read_dir(dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !is_script_file(&path) {
                continue;
            }
            if let Some(script) = parse_script_entry(&path) {
                entries.push(script);
            }
        }
    }
    entries.sort_by(|a, b| a.title.cmp(&b.title));
    entries
}

fn is_script_file(path: &Path) -> bool {
    let Some(ext) = path.extension() else {
        return path
            .metadata()
            .map(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.permissions().mode() & 0o111 != 0
            })
            .unwrap_or(false);
    };
    matches!(
        ext.to_str().unwrap_or(""),
        "sh" | "bash" | "zsh" | "py" | "rb" | "js" | "ts" | "swift" | "applescript"
    )
}

pub(crate) fn parse_script_entry(path: &Path) -> Option<ScriptEntry> {
    let content = fs::read_to_string(path).ok()?;

    let mut schema_version: Option<u32> = None;
    let mut title: Option<String> = None;
    let mut description = String::new();
    let mut package = String::new();
    let mut icon = "text-x-script-symbolic".to_string();
    let mut _mode = ScriptMode::Compact;

    for line in content.lines().take(50) {
        let line = line.trim();
        if !line.starts_with('#') && !line.starts_with("//") {
            if schema_version.is_none() {
                continue;
            }
            break;
        }
        let comment = line.trim_start_matches('#').trim_start_matches("//").trim();

        if let Some(value) = raycast_meta(comment, "schemaVersion") {
            schema_version = value.parse().ok();
        } else if let Some(value) = raycast_meta(comment, "title") {
            title = Some(value.to_string());
        } else if let Some(value) = raycast_meta(comment, "description") {
            description = value.to_string();
        } else if let Some(value) = raycast_meta(comment, "packageName") {
            package = value.to_string();
        } else if let Some(value) = raycast_meta(comment, "icon") {
            icon = value.to_string();
        } else if let Some(value) = raycast_meta(comment, "mode") {
            _mode = match value {
                "fullOutput" => ScriptMode::FullOutput,
                "silent" => ScriptMode::Silent,
                _ => ScriptMode::Compact,
            };
        }
    }

    if schema_version.is_none() || title.is_none() {
        return None;
    }

    Some(ScriptEntry {
        title: title.unwrap(),
        description,
        package,
        icon,
        path: path.to_path_buf(),
    })
}

fn raycast_meta<'a>(comment: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("@raycast.{key}");
    if comment.starts_with(&prefix) {
        let rest = comment[prefix.len()..].trim();
        return Some(rest);
    }
    let prefix2 = format!("@vicinae.{key}");
    if comment.starts_with(&prefix2) {
        let rest = comment[prefix2.len()..].trim();
        return Some(rest);
    }
    None
}

/// Run a script and return its stdout. Used for mode=fullOutput / compact result display.
#[cfg(feature = "gui")]
pub(crate) fn run_script_stdout(path: &std::path::Path) -> std::io::Result<String> {
    let output = std::process::Command::new(path)
        .output()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn search_scripts(entries: &[ScriptEntry], query: &str) -> Vec<Action> {
    if entries.is_empty() {
        return Vec::new();
    }

    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("script ") || lower.starts_with("scripts ");
    let search_query = if explicit {
        query.splitn(2, ' ').nth(1).unwrap_or("").trim()
    } else {
        query.trim()
    };

    if !explicit && search_query.len() < 2 {
        return Vec::new();
    }

    let mut matches: Vec<Action> = entries
        .iter()
        .filter_map(|entry| {
            let text = entry.search_text();
            let score = if search_query.is_empty() {
                20
            } else {
                fuzzy_score(&text, search_query)?
            };
            let category = if entry.package.is_empty() {
                "Script"
            } else {
                "Script"
            };
            let subtitle = if !entry.description.is_empty() {
                entry.description.clone()
            } else if !entry.package.is_empty() {
                entry.package.clone()
            } else {
                entry.path.display().to_string()
            };
            let cmd = entry.path.to_string_lossy().to_string();
            Some(
                Action::new(
                    category,
                    &entry.title,
                    ActionKind::Shell(ShellCommand::new(&cmd)),
                    score + if explicit { 120 } else { 0 },
                )
                .with_subtitle(subtitle)
                .with_icon(&entry.icon),
            )
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    matches.truncate(20);
    matches
}
