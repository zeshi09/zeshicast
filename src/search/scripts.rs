use std::fs;
use std::path::{Path, PathBuf};

use super::commands::parse_capabilities;
use crate::{
    Action, ActionKind, ActionRisk, Capability, ExtensionManifest, ExtensionOrigin, ShellCommand,
    fuzzy_score,
};

#[derive(Debug, Clone)]
pub(crate) struct ScriptEntry {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) icon: String,
    pub(crate) path: PathBuf,
    pub(crate) origin: Option<ExtensionOrigin>,
    pub(crate) capabilities: Vec<Capability>,
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

    fn with_extension_origin(mut self, origin: ExtensionOrigin) -> Self {
        self.capabilities = parse_capabilities(&origin.capabilities);
        self.origin = Some(origin);
        self
    }

    fn has_shell_capability(&self) -> bool {
        self.capabilities.contains(&Capability::Shell)
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

pub(crate) fn load_extension_script_entries(manifests: &[ExtensionManifest]) -> Vec<ScriptEntry> {
    let mut entries = Vec::new();
    for manifest in manifests {
        for path in &manifest.scripts {
            if !is_script_file(path) {
                continue;
            }
            if let Some(script) = parse_script_entry(path) {
                entries.push(script.with_extension_origin(manifest.origin.clone()));
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
        origin: None,
        capabilities: Vec::new(),
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
        query
            .split_once(' ')
            .map(|(_, rest)| rest)
            .unwrap_or("")
            .trim()
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
            let mut subtitle = if !entry.description.is_empty() {
                entry.description.clone()
            } else if !entry.package.is_empty() {
                entry.package.clone()
            } else {
                entry.path.display().to_string()
            };
            // Extension scripts run only when the manifest grants the shell
            // capability; user scripts from plain script_dirs have no manifest
            // and stay available.
            let blocked = entry.origin.is_some() && !entry.has_shell_capability();
            let (kind, icon_name) = if blocked {
                subtitle =
                    "Blocked: script extension manifest lacks capabilities = [\"shell\"]"
                        .to_string();
                (ActionKind::None, "dialog-warning-symbolic")
            } else {
                let cmd = entry.path.to_string_lossy().to_string();
                (ActionKind::Shell(ShellCommand::new(&cmd)), entry.icon.as_str())
            };
            let mut action = Action::new(
                category,
                &entry.title,
                kind,
                score + if explicit { 120 } else { 0 },
            )
            .with_subtitle(subtitle)
            .with_icon(icon_name);
            if !blocked {
                action = action.with_risk(ActionRisk::Shell);
            }
            Some(action)
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    matches.truncate(20);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExtensionManifest, ActionRisk, ExecutionRequest};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SCRIPT_BODY: &str = "#!/bin/sh\n\
        # @raycast.schemaVersion 1\n\
        # @raycast.title Echo Test\n\
        # @raycast.description echoes hello\n\
        # @raycast.packageName test.scripts\n";

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zeshicast-scripts-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_script(dir: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, SCRIPT_BODY).unwrap();
        path
    }

    fn manifest_with_capabilities(root: &Path, capabilities: &[&str]) -> ExtensionManifest {
        ExtensionManifest {
            origin: ExtensionOrigin {
                id: "test.extension".to_string(),
                name: "Test Extension".to_string(),
                version: "0.1.0".to_string(),
                capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
            },
            commands: Vec::new(),
            scripts: vec![root.join("echo.sh")],
        }
    }

    #[test]
    fn extension_script_with_shell_capability_runs_with_confirmation() {
        let root = test_dir("with-shell");
        write_script(&root, "echo.sh");
        let manifests = vec![manifest_with_capabilities(&root, &["shell"])];

        let entries = load_extension_script_entries(&manifests);
        assert_eq!(entries.len(), 1);

        let actions = search_scripts(&entries, "script echo");
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.risk, ActionRisk::Shell);
        assert!(action.risk.requires_confirmation());
        match action.execution_request() {
            Some(ExecutionRequest::Shell { .. }) => {}
            other => panic!("expected shell execution request, got {other:?}"),
        }
        assert!(!action.subtitle.starts_with("Blocked"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn extension_script_without_shell_capability_is_blocked() {
        let root = test_dir("no-shell");
        write_script(&root, "echo.sh");
        let manifests = vec![manifest_with_capabilities(&root, &[])];

        let entries = load_extension_script_entries(&manifests);
        assert_eq!(entries.len(), 1);

        let actions = search_scripts(&entries, "script echo");
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert!(
            action.subtitle.contains("Blocked"),
            "expected blocked subtitle, got {:?}",
            action.subtitle
        );
        assert!(action.execution_request().is_none());
        assert!(!action.risk.requires_confirmation());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn user_script_stays_available_and_requires_confirmation() {
        let dir = test_dir("user");
        write_script(&dir, "echo.sh");

        let entries = load_script_entries(std::slice::from_ref(&dir));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].origin.is_none());

        let actions = search_scripts(&entries, "script echo");
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.risk, ActionRisk::Shell);
        assert!(action.risk.requires_confirmation());
        assert!(action.execution_request().is_some());
        assert!(!action.subtitle.starts_with("Blocked"));

        fs::remove_dir_all(&dir).ok();
    }
}
