use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::commands::parse_capabilities;
use crate::{
    Action, ActionForm, ActionFormCommand, ActionFormField, ActionKind, ActionRisk, Capability,
    CommandArgumentKind, ExtensionManifest, ExtensionOrigin, ScriptMode, ShellCommand, fuzzy_score,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptArgument {
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) placeholder: String,
    pub(crate) kind: CommandArgumentKind,
    pub(crate) optional: bool,
    pub(crate) percent_encoded: bool,
    pub(crate) options: Vec<String>,
    pub(crate) default: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ScriptEntry {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) icon: String,
    pub(crate) path: PathBuf,
    pub(crate) origin: Option<ExtensionOrigin>,
    pub(crate) capabilities: Vec<Capability>,
    pub(crate) mode: ScriptMode,
    pub(crate) arguments: Vec<ScriptArgument>,
    pub(crate) needs_confirmation: Option<bool>,
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

fn parse_argument_meta(comment: &str) -> Option<(usize, ScriptArgument)> {
    let rest = comment
        .strip_prefix("@raycast.argument")
        .or_else(|| comment.strip_prefix("@vicinae.argument"))?;

    let split_pos = rest.find(|c: char| c.is_whitespace() || c == '{')?;
    let (idx_str, remainder) = rest.split_at(split_pos);
    let index: usize = idx_str.parse().ok()?;
    let json_start = remainder.find('{')?;
    let json_str = &remainder[json_start..];

    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let obj = val.as_object()?;

    let arg_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("text");
    let kind = match arg_type {
        "dropdown" | "enum" => CommandArgumentKind::Enum,
        "number" => CommandArgumentKind::Number,
        "path" => CommandArgumentKind::Path,
        "bool" | "boolean" => CommandArgumentKind::Bool,
        _ => CommandArgumentKind::Text,
    };

    let placeholder = obj
        .get("placeholder")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let name = if !placeholder.is_empty() {
        placeholder.clone()
    } else {
        format!("argument{index}")
    };

    let optional = obj
        .get("optional")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let percent_encoded = obj
        .get("percentEncoded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut options = Vec::new();
    if let Some(data) = obj.get("data").and_then(|v| v.as_array()) {
        for item in data {
            if let Some(s) = item.as_str() {
                options.push(s.to_string());
            } else if let Some(opt_obj) = item.as_object() {
                if let Some(val) = opt_obj.get("value").and_then(|v| v.as_str()) {
                    options.push(val.to_string());
                } else if let Some(title) = opt_obj.get("title").and_then(|v| v.as_str()) {
                    options.push(title.to_string());
                }
            }
        }
    }

    let default = obj
        .get("default")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some((
        index,
        ScriptArgument {
            index,
            name,
            placeholder,
            kind,
            optional,
            percent_encoded,
            options,
            default,
        },
    ))
}

pub(crate) fn parse_script_entry(path: &Path) -> Option<ScriptEntry> {
    let content = fs::read_to_string(path).ok()?;

    let mut schema_version: Option<u32> = None;
    let mut title: Option<String> = None;
    let mut description = String::new();
    let mut package = String::new();
    let mut icon = "text-x-script-symbolic".to_string();
    let mut mode = ScriptMode::FullOutput;
    let mut needs_confirmation: Option<bool> = None;
    let mut arguments_map: BTreeMap<usize, ScriptArgument> = BTreeMap::new();

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
            mode = match value {
                "fullOutput" => ScriptMode::FullOutput,
                "silent" => ScriptMode::Silent,
                "compact" => ScriptMode::Compact,
                "inline" => ScriptMode::Inline,
                _ => ScriptMode::FullOutput,
            };
        } else if let Some(value) = raycast_meta(comment, "needsConfirmation") {
            needs_confirmation = value.parse().ok();
        } else if let Some((idx, arg)) = parse_argument_meta(comment) {
            arguments_map.insert(idx, arg);
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
        mode,
        arguments: arguments_map.into_values().collect(),
        needs_confirmation,
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

/// Run a script with arguments and return its stdout.
#[cfg(feature = "gui")]
pub(crate) fn run_script_stdout_with_args(
    path: &std::path::Path,
    args: &[String],
) -> std::io::Result<String> {
    let res = std::process::Command::new(path).args(args).output();
    let output = match res {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let mut cmd = match ext {
                "py" => std::process::Command::new("python3"),
                "js" | "ts" => std::process::Command::new("node"),
                "rb" => std::process::Command::new("ruby"),
                _ => std::process::Command::new("sh"),
            };
            cmd.arg(path)
                .args(args)
                .output()
                .map_err(|err| std::io::Error::other(err.to_string()))?
        }
        Err(e) => return Err(std::io::Error::other(e.to_string())),
    };
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run a script and return its stdout. Used for mode=fullOutput / compact result display.
#[cfg(feature = "gui")]
#[allow(dead_code)]
pub(crate) fn run_script_stdout(path: &std::path::Path) -> std::io::Result<String> {
    run_script_stdout_with_args(path, &[])
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
            } else if entry.arguments.is_empty() {
                let cmd = entry.path.to_string_lossy().to_string();
                (ActionKind::Shell(ShellCommand::new(&cmd)), entry.icon.as_str())
            } else {
                let fields = entry
                    .arguments
                    .iter()
                    .map(|arg| ActionFormField {
                        name: arg.name.clone(),
                        kind: arg.kind,
                        required: !arg.optional,
                        default: arg.default.clone(),
                        options: arg.options.clone(),
                        current_value: arg.default.clone(),
                        percent_encoded: arg.percent_encoded,
                    })
                    .collect();
                let form = ActionForm {
                    name: entry.title.clone(),
                    fields,
                    command: ActionFormCommand::Argv {
                        program: entry.path.to_string_lossy().to_string(),
                        args: entry
                            .arguments
                            .iter()
                            .map(|arg| format!("{{{{arg:{}}}}}", arg.name))
                            .collect(),
                    },
                    env: std::collections::HashMap::new(),
                    preferences: std::collections::HashMap::new(),
                    current_args: std::collections::HashMap::new(),
                    partial_query: String::new(),
                };
                (ActionKind::Form(form), entry.icon.as_str())
            };
            let mut action = Action::new(
                category,
                &entry.title,
                kind,
                score + if explicit { 120 } else { 0 },
            )
            .with_subtitle(subtitle)
            .with_icon(icon_name)
            .with_script_mode(entry.mode);
            if !blocked && entry.needs_confirmation != Some(false) {
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

    #[test]
    fn parse_script_metadata_modes_and_arguments() {
        let dir = test_dir("meta-args");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("search.sh");
        let content = "#!/bin/sh\n\
            # @raycast.schemaVersion 1\n\
            # @raycast.title Web Search\n\
            # @raycast.mode compact\n\
            # @raycast.packageName web.search\n\
            # @raycast.needsConfirmation false\n\
            # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"Search Query\", \"percentEncoded\": true }\n\
            # @raycast.argument2 { \"type\": \"dropdown\", \"placeholder\": \"Engine\", \"optional\": true, \"data\": [{\"title\": \"Google\", \"value\": \"g\"}, {\"title\": \"Bing\", \"value\": \"b\"}] }\n\
            echo \"searching\"\n";
        fs::write(&path, content).unwrap();

        let entry = parse_script_entry(&path).expect("should parse script entry");
        assert_eq!(entry.title, "Web Search");
        assert_eq!(entry.mode, ScriptMode::Compact);
        assert_eq!(entry.needs_confirmation, Some(false));
        assert_eq!(entry.arguments.len(), 2);

        let arg1 = &entry.arguments[0];
        assert_eq!(arg1.index, 1);
        assert_eq!(arg1.name, "Search Query");
        assert_eq!(arg1.kind, CommandArgumentKind::Text);
        assert!(arg1.percent_encoded);
        assert!(!arg1.optional);

        let arg2 = &entry.arguments[1];
        assert_eq!(arg2.index, 2);
        assert_eq!(arg2.name, "Engine");
        assert_eq!(arg2.kind, CommandArgumentKind::Enum);
        assert!(arg2.optional);
        assert_eq!(arg2.options, vec!["g", "b"]);

        // Search creates ActionKind::Form because arguments are required
        let actions = search_scripts(&[entry], "Web Search");
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.script_mode(), Some(ScriptMode::Compact));
        // needsConfirmation was false, so risk should remain Normal
        assert_eq!(action.risk, ActionRisk::Normal);
        assert!(action.form_data().is_some());
        let form = action.form_data().unwrap();
        assert_eq!(form.fields.len(), 2);
        assert_eq!(form.fields[0].name, "Search Query");
        assert!(form.fields[0].percent_encoded);
        assert_eq!(form.fields[1].name, "Engine");
        assert_eq!(form.fields[1].options, vec!["g", "b"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_vicinae_script_metadata() {
        let dir = test_dir("vicinae-meta");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vicinae_test.sh");
        let content = "#!/bin/sh\n\
            // @vicinae.schemaVersion 1\n\
            // @vicinae.title Vicinae Test\n\
            // @vicinae.mode silent\n\
            // @vicinae.argument1 { \"type\": \"text\", \"placeholder\": \"Vicinae Arg\" }\n";
        fs::write(&path, content).unwrap();

        let entry = parse_script_entry(&path).expect("should parse vicinae entry");
        assert_eq!(entry.title, "Vicinae Test");
        assert_eq!(entry.mode, ScriptMode::Silent);
        assert_eq!(entry.arguments.len(), 1);
        assert_eq!(entry.arguments[0].name, "Vicinae Arg");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn percent_encoding_works() {
        assert_eq!(crate::percent_encode("hello world"), "hello%20world");
        assert_eq!(
            crate::percent_encode("foo+bar/baz?a=1&b=2"),
            "foo%2Bbar%2Fbaz%3Fa%3D1%26b%3D2"
        );
        assert_eq!(
            crate::percent_encode("unreserved-_.~123"),
            "unreserved-_.~123"
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn run_script_with_args_captures_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = test_dir("run-args");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("greet.sh");
        let content = "#!/bin/sh\necho \"Hello $1, welcome to $2!\"\n";
        fs::write(&path, content).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();

        let output = run_script_stdout_with_args(
            &path,
            &["Zeshi".to_string(), "Zeshicast".to_string()],
        )
        .expect("script execution succeeded");

        assert_eq!(output.trim(), "Hello Zeshi, welcome to Zeshicast!");

        fs::remove_dir_all(&dir).ok();
    }
}
