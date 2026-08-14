use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    Action, ActionForm, ActionFormField, ActionKind, CommandArgumentKind, ShellCommand, fuzzy_score,
};

#[derive(Debug, Clone)]
pub(crate) struct ScriptArgument {
    #[allow(dead_code)]
    pub(crate) name: String,
    pub(crate) placeholder: String,
    pub(crate) optional: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ScriptEntry {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) icon: String,
    pub(crate) path: PathBuf,
    #[allow(dead_code)]
    pub(crate) mode: ScriptMode,
    pub(crate) arguments: Vec<ScriptArgument>,
    #[allow(dead_code)]
    pub(crate) needs_confirmation: bool,
    #[allow(dead_code)]
    pub(crate) current_directory: Option<PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptMode {
    Compact,
    FullOutput,
    Silent,
    Inline,
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
    let mut mode = ScriptMode::Compact;
    let mut arguments = Vec::new();
    let mut needs_confirmation = false;
    let mut current_directory = None;

    for line in content.lines().take(50) {
        let line = line.trim();
        if !line.starts_with('#') && !line.starts_with("//") {
            if schema_version.is_none() {
                continue;
            }
            break;
        }
        let comment = line
            .trim_start_matches('#')
            .trim_start_matches("//")
            .trim();

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
            mode = match value {
                "fullOutput" => ScriptMode::FullOutput,
                "silent" => ScriptMode::Silent,
                "inline" => ScriptMode::Inline,
                _ => ScriptMode::Compact,
            };
        } else if let Some(value) = raycast_meta(comment, "needsConfirmation") {
            needs_confirmation = value.parse().unwrap_or(false);
        } else if let Some(value) = raycast_meta(comment, "currentDirectoryPath") {
            current_directory = Some(PathBuf::from(value));
        } else if let Some(arg) = parse_raycast_argument(comment) {
            arguments.push(arg);
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
        mode,
        arguments,
        needs_confirmation,
        current_directory,
    })
}

fn parse_raycast_argument(comment: &str) -> Option<ScriptArgument> {
    for idx in 1..=4 {
        let key = format!("argument{idx}");
        if let Some(json_str) = raycast_meta(comment, &key) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                let placeholder = value
                    .get("placeholder")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&key)
                    .to_string();
                let optional = value
                    .get("optional")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                return Some(ScriptArgument {
                    name: format!("arg{idx}"),
                    placeholder,
                    optional,
                });
            } else {
                return Some(ScriptArgument {
                    name: format!("arg{idx}"),
                    placeholder: json_str.to_string(),
                    optional: false,
                });
            }
        }
    }
    None
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
#[allow(dead_code)]
pub(crate) fn run_script_stdout(path: &std::path::Path) -> std::io::Result<String> {
    let output = std::process::Command::new(path)
        .output()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse compact-mode JSON output into actions.
#[allow(dead_code)]
pub(crate) fn parse_script_json_output(stdout: &str, category: &str) -> Vec<Action> {
    if serde_json::from_str::<serde_json::Value>(stdout).is_err() {
        return Vec::new();
    }
    crate::search::commands::parse_json_actions(stdout, category, 500)
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
            let category = "Script";
            let subtitle = if !entry.description.is_empty() {
                entry.description.clone()
            } else if !entry.package.is_empty() {
                entry.package.clone()
            } else {
                entry.path.display().to_string()
            };
            let cmd = entry.path.to_string_lossy().to_string();

            let kind = if !entry.arguments.is_empty() {
                let fields = entry
                    .arguments
                    .iter()
                    .map(|arg| ActionFormField {
                        name: arg.placeholder.clone(),
                        kind: CommandArgumentKind::Text,
                        required: !arg.optional,
                        default: String::new(),
                        options: Vec::new(),
                        current_value: String::new(),
                    })
                    .collect();

                ActionKind::Form(ActionForm {
                    name: entry.title.clone(),
                    fields,
                    command: cmd,
                    env: HashMap::new(),
                    preferences: HashMap::new(),
                    current_args: HashMap::new(),
                    partial_query: String::new(),
                })
            } else {
                ActionKind::Shell(ShellCommand::new(&cmd))
            };

            Some(
                Action::new(
                    category,
                    &entry.title,
                    kind,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_script_json_output_handles_valid_and_invalid_json() {
        let empty = parse_script_json_output("not json", "Script");
        assert!(empty.is_empty());

        let valid = r#"[{"title": "Test Action", "cmd": "echo test"}]"#;
        let actions = parse_script_json_output(valid, "Script");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Test Action");
    }

    #[test]
    fn test_parse_raycast_argument_json() {
        let comment = r#"@raycast.argument1 {"type": "text", "placeholder": "Search Query", "optional": false}"#;
        let arg = parse_raycast_argument(comment).unwrap();
        assert_eq!(arg.name, "arg1");
        assert_eq!(arg.placeholder, "Search Query");
        assert!(!arg.optional);
    }
}

