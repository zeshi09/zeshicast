use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
// The palette's own machinery: a JSON command's stdout becomes rows, so only a
// build with a results list can consume it (P5.1).
#[cfg(feature = "gui")]
use std::io;
use std::path::{Path, PathBuf};
#[cfg(feature = "gui")]
use std::process::Command;

use crate::action::{
    Action, ActionForm, ActionFormCommand, ActionFormField, ActionKind, ActionRisk, Capability,
    CapabilitySet, CommandArgumentKind, JsonCommandAction, ProcessCommand, ShellCommand,
};
use crate::config::{normalize_alias, toml_value_string};
use crate::extensions::{ExtensionManifest, ExtensionOrigin};
use crate::placeholders::{PlaceholderContext, expand_placeholders, expand_placeholders_shell};
use crate::search::fuzzy_score;
use crate::search::named_values::tagged_subtitle;

#[derive(Debug, Clone)]
pub(crate) struct CommandEntry {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) program: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: HashMap<String, String>,
    pub(crate) mode: CommandMode,
    pub(crate) category: String,
    pub(crate) keyword: Option<String>,
    pub(crate) argument_hint: String,
    pub(crate) arguments: Vec<CommandArgument>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) tags: Vec<String>,
    pub(crate) icon_name: String,
    pub(crate) description: String,
    pub(crate) permissions: Vec<String>,
    pub(crate) capabilities: Vec<Capability>,
    pub(crate) origin: Option<ExtensionOrigin>,
}

impl CommandEntry {
    fn search_text(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.name,
            self.description,
            self.category,
            self.tags.join(" "),
            self.keyword.as_deref().unwrap_or_default()
        )
    }

    /// The extension manifest is a ceiling, not a grant: `effective =
    /// declared ∩ manifest`. A command may narrow its own permissions further
    /// but can never widen what the manifest granted (see P1.8).
    pub(crate) fn with_extension_origin(mut self, origin: ExtensionOrigin) -> Self {
        let manifest = CapabilitySet::new(parse_capabilities(&origin.capabilities));
        let effective = parse_capabilities(&self.permissions)
            .into_iter()
            .filter(|capability| manifest.allows(*capability))
            .collect::<Vec<_>>();
        self.permissions = effective
            .iter()
            .map(|capability| capability.label().to_string())
            .collect();
        self.capabilities = effective;
        self.origin = Some(origin);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandMode {
    Shell,
    Argv,
    Json,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandArgument {
    pub(crate) name: String,
    pub(crate) kind: CommandArgumentKind,
    pub(crate) required: bool,
    pub(crate) default: String,
    pub(crate) options: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandMatch {
    pub(crate) score: i32,
    pub(crate) argument: String,
    pub(crate) args: HashMap<String, String>,
    missing: Vec<String>,
    direct: bool,
}

#[derive(Debug, Clone)]
struct ArgumentBinding {
    values: HashMap<String, String>,
    missing: Vec<String>,
}

pub(crate) fn load_command_entries(dir: &Path) -> Vec<CommandEntry> {
    load_command_entries_from_paths(command_paths_in_dir(dir), None)
}

pub(crate) fn load_extension_command_entries(manifests: &[ExtensionManifest]) -> Vec<CommandEntry> {
    manifests
        .iter()
        .flat_map(|manifest| {
            load_command_entries_from_paths(
                manifest.commands.clone(),
                Some(manifest.origin.clone()),
            )
        })
        .collect()
}

fn command_paths_in_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect()
}

fn load_command_entries_from_paths(
    paths: Vec<PathBuf>,
    origin: Option<ExtensionOrigin>,
) -> Vec<CommandEntry> {
    let mut commands = Vec::new();
    for path in paths {
        let Some(mut command) = fs::read_to_string(&path)
            .ok()
            .and_then(|content| parse_command_entry(&content))
        else {
            eprintln!("failed to parse command: {}", path.display());
            continue;
        };
        if let Some(origin) = &origin {
            command = command.with_extension_origin(origin.clone());
        }
        commands.push(command);
    }

    commands
}

pub(crate) fn parse_command_entry(input: &str) -> Option<CommandEntry> {
    let table = input.parse::<toml::Table>().ok()?;
    let name = toml_required_string(&table, "name")?;
    let env = parse_env_table(table.get("env"));
    let mode = toml_optional_string(&table, "mode")
        .as_deref()
        .and_then(parse_command_mode)
        .unwrap_or(CommandMode::Shell);
    let command = toml_optional_string(&table, "command").unwrap_or_default();
    let program = toml_optional_string(&table, "program");
    let args = toml_string_array(&table, "args");
    match mode {
        CommandMode::Shell | CommandMode::Json if command.is_empty() => return None,
        CommandMode::Argv if program.as_deref().unwrap_or_default().is_empty() => return None,
        _ => {}
    }
    let category = toml_optional_string(&table, "category")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Command".to_string());
    let icon_name = toml_optional_string(&table, "icon")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "utilities-terminal-symbolic".to_string());
    let keyword = toml_optional_string(&table, "keyword")
        .map(|value| normalize_alias(&value))
        .filter(|value| !value.is_empty());
    let argument_hint = toml_optional_string(&table, "argument_hint").unwrap_or_default();
    let arguments = parse_command_arguments(&table);
    let preferences = parse_preferences_table(table.get("preferences"));
    let description = toml_optional_string(&table, "description").unwrap_or_default();
    let tags = toml_string_array(&table, "tags");
    let permissions = toml_string_array(&table, "permissions");
    let capabilities = parse_capabilities(&permissions);

    Some(CommandEntry {
        name,
        command,
        program,
        args,
        env,
        mode,
        category,
        keyword,
        argument_hint,
        arguments,
        preferences,
        tags,
        icon_name,
        description,
        permissions,
        capabilities,
        origin: None,
    })
}

pub(crate) fn parse_capabilities(permissions: &[String]) -> Vec<Capability> {
    let mut capabilities = Vec::new();
    for permission in permissions {
        let Some(capability) = parse_capability(permission) else {
            continue;
        };
        if !capabilities.contains(&capability) {
            capabilities.push(capability);
        }
    }
    capabilities
}

fn parse_capability(permission: &str) -> Option<Capability> {
    match permission.trim().to_lowercase().replace('-', "_").as_str() {
        "shell" | "run" | "exec" => Some(Capability::Shell),
        "network" | "net" => Some(Capability::Network),
        "filesystem" | "fs" => Some(Capability::Filesystem),
        "clipboard_read" | "clipboardread" => Some(Capability::ClipboardRead),
        "clipboard_write" | "clipboardwrite" | "clipboard" => Some(Capability::ClipboardWrite),
        "open_url" | "openurl" | "url" => Some(Capability::OpenUrl),
        "open_path" | "openpath" | "path" => Some(Capability::OpenPath),
        _ => None,
    }
}

fn has_capability(capabilities: &[Capability], required: Capability) -> bool {
    capabilities.iter().any(|capability| {
        *capability == required
            || matches!(
                (*capability, required),
                (Capability::Network, Capability::OpenUrl)
                    | (Capability::Filesystem, Capability::OpenPath)
            )
    })
}

fn parse_command_mode(mode: &str) -> Option<CommandMode> {
    match mode.trim().to_lowercase().as_str() {
        "shell" | "run" => Some(CommandMode::Shell),
        "argv" | "exec" | "process" => Some(CommandMode::Argv),
        "json" | "list" => Some(CommandMode::Json),
        _ => None,
    }
}

fn parse_command_arguments(table: &toml::Table) -> Vec<CommandArgument> {
    table
        .get("arguments")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_table())
        .filter_map(parse_command_argument)
        .collect()
}

fn parse_command_argument(table: &toml::Table) -> Option<CommandArgument> {
    let name = toml_required_string(table, "name")?;
    let kind = toml_optional_string(table, "type")
        .as_deref()
        .and_then(parse_command_argument_kind)
        .unwrap_or(CommandArgumentKind::Text);
    let required = table
        .get("required")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    let default = toml_optional_string(table, "default").unwrap_or_default();
    let options = toml_string_array(table, "options");

    Some(CommandArgument {
        name,
        kind,
        required,
        default,
        options,
    })
}

fn parse_command_argument_kind(kind: &str) -> Option<CommandArgumentKind> {
    match kind.trim().to_lowercase().as_str() {
        "text" | "string" => Some(CommandArgumentKind::Text),
        "number" | "float" | "integer" | "int" => Some(CommandArgumentKind::Number),
        "path" | "file" | "folder" => Some(CommandArgumentKind::Path),
        "bool" | "boolean" => Some(CommandArgumentKind::Bool),
        "enum" | "select" | "choice" => Some(CommandArgumentKind::Enum),
        _ => None,
    }
}

fn toml_required_string(table: &toml::Table, key: &str) -> Option<String> {
    toml_optional_string(table, key).filter(|value| !value.is_empty())
}

fn toml_optional_string(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)?
        .as_str()
        .map(|value| value.trim().to_string())
}

fn toml_string_array(table: &toml::Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_preferences_table(value: Option<&toml::Value>) -> HashMap<String, String> {
    value
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|table| table.iter())
        .filter_map(|(key, value)| toml_value_string(value).map(|value| (key.clone(), value)))
        .collect()
}

fn parse_env_table(value: Option<&toml::Value>) -> HashMap<String, String> {
    value
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|table| table.iter())
        .filter_map(|(key, value)| {
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            toml_value_string(value).map(|value| (key.to_string(), value))
        })
        .collect()
}

pub(crate) fn search_commands(
    entries: &[CommandEntry],
    query: &str,
    context: &PlaceholderContext<'_>,
) -> Vec<Action> {
    entries
        .iter()
        .flat_map(|entry| {
            let command_match = match_command_entry(entry, query)?;
            let merged_preferences = command_preferences(entry, &context.preferences);
            let command_context = PlaceholderContext {
                query: command_match.argument.clone(),
                clipboard: context.clipboard.clone(),
                args: command_match.args.clone(),
                preferences: Cow::Owned(merged_preferences),
                now: context.now,
            };
            let env = command_env(entry, &command_context);

            if entry.mode == CommandMode::Json
                && command_match.direct
                && command_match.missing.is_empty()
            {
                let command = expand_placeholders_shell(&entry.command, &command_context);
                let shell_command = ShellCommand::with_env(command, env);
                return Some(vec![json_command_action(
                    entry,
                    shell_command,
                    command_match.score,
                )]);
            }

            let command_display = command_display(entry, &command_context);
            let mut subtitle = command_subtitle(entry, &command_display, &command_match.missing);
            let capabilities = effective_capabilities(entry);
            let (kind, icon_name) =
                match (capability_denial(entry), command_match.missing.is_empty()) {
                    (Some(reason), _) => {
                        subtitle = reason.to_string();
                        (ActionKind::None, "dialog-warning-symbolic")
                    }
                    (None, true) => match entry.mode {
                        CommandMode::Argv => (
                            ActionKind::Command(argv_process_command(entry, &command_context, env)),
                            entry.icon_name.as_str(),
                        ),
                        CommandMode::Shell | CommandMode::Json => {
                            let command =
                                expand_placeholders_shell(&entry.command, &command_context);
                            let shell_command = ShellCommand::with_env(command, env);
                            (ActionKind::Shell(shell_command), entry.icon_name.as_str())
                        }
                    },
                    (None, false) => {
                        let fields = entry
                            .arguments
                            .iter()
                            .filter(|arg| command_match.missing.contains(&arg.name))
                            .map(|arg| ActionFormField {
                                name: arg.name.clone(),
                                kind: arg.kind,
                                required: arg.required,
                                default: arg.default.clone(),
                                options: arg.options.clone(),
                                current_value: command_match
                                    .args
                                    .get(&arg.name)
                                    .cloned()
                                    .unwrap_or_default(),
                                percent_encoded: false,
                            })
                            .collect();
                        let form = ActionForm {
                            name: entry.name.clone(),
                            fields,
                            command: form_command(entry),
                            env: entry.env.clone(),
                            preferences: command_preferences(entry, &context.preferences),
                            current_args: command_match.args.clone(),
                            partial_query: command_match.argument.clone(),
                            capabilities: capabilities.clone(),
                            // A form is never more trusted than the command it
                            // came from; submission asks for confirmation.
                            risk: ActionRisk::Shell,
                        };
                        (ActionKind::Form(form), "dialog-question-symbolic")
                    }
                };

            let mut action =
                Action::new(&entry.category, &entry.name, kind, command_match.score + 85)
                    .with_subtitle(subtitle)
                    .with_icon(icon_name)
                    .with_capabilities(capabilities.clone());
            // Anything that can execute code (directly or via a form) asks for
            // confirmation; `None` (blocked) and JSON producers keep their own
            // risk, which is set where the action is built.
            if matches!(
                action.kind,
                ActionKind::Shell(_) | ActionKind::Command(_) | ActionKind::Form(_)
            ) {
                action = action.with_risk(ActionRisk::Shell);
            }

            Some(vec![action])
        })
        .flatten()
        .collect()
}

const BLOCKED_LACKS_SHELL: &str = "Blocked: command manifest lacks permissions = [\"shell\"]";

/// `Some(reason)` when the command must not run at all (not even as a form).
///
/// Shell and JSON producer commands always need `sh -c`; an argv command from
/// an extension is not a loophole and needs the manifest to grant
/// `shell`/`exec` too. A hand-written argv command in the user's own config dir
/// has no manifest behind it and is gated by the confirmation prompt instead.
fn capability_denial(entry: &CommandEntry) -> Option<&'static str> {
    if has_capability(&entry.capabilities, Capability::Shell) {
        return None;
    }
    match entry.mode {
        CommandMode::Shell | CommandMode::Json => Some(BLOCKED_LACKS_SHELL),
        CommandMode::Argv if entry.origin.is_some() => Some(BLOCKED_LACKS_SHELL),
        CommandMode::Argv => None,
    }
}

/// Capabilities handed to the action/form built from `entry`.
fn effective_capabilities(entry: &CommandEntry) -> CapabilitySet {
    let mut capabilities = CapabilitySet::new(entry.capabilities.clone());
    if entry.mode == CommandMode::Argv && entry.origin.is_none() {
        // Local argv command: it executes a program directly (no `sh -c`) and
        // is confirmed before running, so it may carry the shell capability.
        capabilities.grant(Capability::Shell);
    }
    capabilities
}

fn command_display(entry: &CommandEntry, context: &PlaceholderContext<'_>) -> String {
    match entry.mode {
        CommandMode::Argv => argv_process_command(entry, context, HashMap::new()).display(),
        CommandMode::Shell | CommandMode::Json => {
            expand_placeholders_shell(&entry.command, context)
        }
    }
}

fn argv_process_command(
    entry: &CommandEntry,
    context: &PlaceholderContext<'_>,
    env: HashMap<String, String>,
) -> ProcessCommand {
    let program = entry
        .program
        .as_deref()
        .map(|program| expand_placeholders(program, context))
        .unwrap_or_default();
    let args = entry
        .args
        .iter()
        .map(|arg| expand_placeholders(arg, context))
        .collect();
    ProcessCommand::with_env(program, args, env)
}

fn form_command(entry: &CommandEntry) -> ActionFormCommand {
    match entry.mode {
        CommandMode::Argv => ActionFormCommand::Argv {
            program: entry.program.clone().unwrap_or_default(),
            args: entry.args.clone(),
        },
        CommandMode::Shell | CommandMode::Json => ActionFormCommand::Shell(entry.command.clone()),
    }
}

fn json_command_action(entry: &CommandEntry, command: ShellCommand, score: i32) -> Action {
    let capabilities = CapabilitySet::new(entry.capabilities.clone());
    if !has_capability(&entry.capabilities, Capability::Shell) {
        return Action::new(&entry.category, &entry.name, ActionKind::None, score + 1)
            .with_subtitle(BLOCKED_LACKS_SHELL)
            .with_icon("dialog-warning-symbolic")
            .with_capabilities(capabilities);
    }

    let command_value = command.command.clone();
    Action::new(
        &entry.category,
        &entry.name,
        ActionKind::JsonCommand(JsonCommandAction {
            category: entry.category.clone(),
            command,
            capabilities: entry.capabilities.clone(),
            score: score + 100,
        }),
        score + 85,
    )
    .with_subtitle(format!("Run JSON command - {command_value}"))
    .with_icon(&entry.icon_name)
    .with_risk(ActionRisk::Shell)
    .with_capabilities(capabilities)
}

#[cfg(feature = "gui")]
pub(crate) fn run_json_command_actions(action: &JsonCommandAction) -> Vec<Action> {
    match run_json_command(&action.command) {
        Ok(stdout) => parse_json_actions(
            &stdout,
            &action.category,
            action.score,
            &action.capabilities,
        ),
        Err(error) => vec![
            Action::new(
                &action.category,
                "JSON command failed",
                ActionKind::None,
                action.score,
            )
            .with_subtitle(format!("JSON command failed: {error}"))
            .with_icon("dialog-warning-symbolic"),
        ],
    }
}

/// Bound JSON commands so slow scripts do not occupy the worker indefinitely.
#[cfg(feature = "gui")]
const JSON_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(feature = "gui")]
const JSON_STDOUT_LIMIT_BYTES: u64 = 512 * 1024;
#[cfg(feature = "gui")]
const JSON_STDERR_LIMIT_BYTES: u64 = 8 * 1024;

#[cfg(feature = "gui")]
fn run_json_command(command: &ShellCommand) -> io::Result<String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command.command)
        .envs(&command.env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= JSON_COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("json command timed out"));
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    };

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.by_ref()
            .take(JSON_STDOUT_LIMIT_BYTES + 1)
            .read_to_string(&mut stdout)
            .ok();
    }
    if stdout.len() as u64 > JSON_STDOUT_LIMIT_BYTES {
        return Err(io::Error::other("json command stdout exceeded 512 KiB"));
    }

    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            err.by_ref()
                .take(JSON_STDERR_LIMIT_BYTES)
                .read_to_string(&mut stderr)
                .ok();
        }
        return Err(io::Error::other(
            stderr.trim().chars().take(160).collect::<String>(),
        ));
    }

    Ok(stdout)
}

#[cfg(any(feature = "gui", test))]
pub(crate) fn parse_json_actions(
    input: &str,
    category: &str,
    base_score: i32,
    capabilities: &[Capability],
) -> Vec<Action> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return vec![
            Action::new(
                category,
                "Invalid JSON output",
                ActionKind::None,
                base_score,
            )
            .with_subtitle("Expected an array or {\"results\": [...]}")
            .with_icon("dialog-warning-symbolic"),
        ];
    };

    let values = value
        .as_array()
        .or_else(|| value.get("results").and_then(serde_json::Value::as_array));

    let Some(values) = values else {
        return Vec::new();
    };

    values
        .iter()
        .take(crate::search::MAX_RESULTS)
        .enumerate()
        .filter_map(|(index, value)| {
            parse_json_action(value, category, base_score - index as i32, capabilities)
        })
        .collect()
}

#[cfg(any(feature = "gui", test))]
fn parse_json_action(
    value: &serde_json::Value,
    category: &str,
    score: i32,
    capabilities: &[Capability],
) -> Option<Action> {
    let title = json_string(value, "title")?;
    let mut subtitle = json_string(value, "subtitle").unwrap_or_default();
    let icon_name = json_string(value, "icon").unwrap_or_else(|| "system-run-symbolic".to_string());
    let parsed = parse_json_action_kind(value, capabilities);
    if let Some(denial) = parsed.denial {
        subtitle = if subtitle.is_empty() {
            denial
        } else {
            format!("{denial} - {subtitle}")
        };
    }
    let risk = parsed.risk;

    Some(
        Action::new(
            json_string(value, "category").unwrap_or_else(|| category.to_string()),
            title,
            parsed.kind,
            score,
        )
        .with_subtitle(subtitle)
        .with_icon(icon_name)
        .with_risk(risk)
        // The producer's own manifest is the ceiling for whatever it returns;
        // the gateway re-checks it before anything runs.
        .with_capabilities(CapabilitySet::new(capabilities.to_vec())),
    )
}

#[cfg(any(feature = "gui", test))]
struct ParsedJsonActionKind {
    kind: ActionKind,
    denial: Option<String>,
    risk: ActionRisk,
}

#[cfg(any(feature = "gui", test))]
impl ParsedJsonActionKind {
    fn blocked(reason: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::None,
            denial: Some(reason.into()),
            risk: ActionRisk::Normal,
        }
    }
}

/// The effect an extension-produced result wants to perform, before any
/// capability is consulted. Shared by JSON-command output and extension search
/// items so both pass through exactly one gate (P1.5).
// `ActionIntent` variants produced only by JSON-command output are compiled
// only where that producer exists (`gui` or tests); extension search results
// use the always-present subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActionIntent {
    OpenUrl(String),
    Copy(String),
    /// Run an item of the producing extension through that extension's own
    /// JSON-RPC `execute` method. The id names an item in that extension, so it
    /// is never a command line (P1.5c).
    ExtensionItem {
        binary: PathBuf,
        id: String,
    },
    #[cfg(any(feature = "gui", test))]
    OpenPath(String),
    #[cfg(any(feature = "gui", test))]
    Shell(String),
    #[cfg(any(feature = "gui", test))]
    None,
}

/// Result of the shared capability gate: a kind to build an action from, the
/// risk it must carry, and a denial reason when it is not allowed.
pub(crate) struct GatedAction {
    pub(crate) kind: ActionKind,
    pub(crate) risk: ActionRisk,
    pub(crate) denial: Option<String>,
}

impl GatedAction {
    fn allowed(kind: ActionKind, risk: ActionRisk) -> Self {
        Self {
            kind,
            risk,
            denial: None,
        }
    }

    fn blocked(reason: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::None,
            risk: ActionRisk::Normal,
            denial: Some(reason.into()),
        }
    }
}

/// The single gate for results produced by commands and extensions. Anything
/// that can execute code (`Shell`, `ExtensionItem`) is downgraded to a confirmed
/// action; anything else is checked against the granted capabilities.
pub(crate) fn gate_action_intent(intent: ActionIntent, capabilities: &[Capability]) -> GatedAction {
    let allowed = |capability: Capability| has_capability(capabilities, capability);
    match intent {
        ActionIntent::OpenUrl(url) => {
            // A `file://` URL opens a local path, so it is gated like
            // `open_path`: a plain `open_url`/`network` grant must not be able
            // to read the filesystem through the browser/`xdg-open`.
            let required = if json_url_requires_capability(&url) {
                Capability::OpenUrl
            } else {
                Capability::OpenPath
            };
            if allowed(required) {
                GatedAction::allowed(ActionKind::OpenUrl(url), ActionRisk::Normal)
            } else if required == Capability::OpenPath {
                GatedAction::blocked(
                    "Blocked: file:// URL requires permissions = [\"filesystem\"] or [\"open_path\"]",
                )
            } else {
                GatedAction::blocked(
                    "Blocked: JSON action requires permissions = [\"network\"] or [\"open_url\"]",
                )
            }
        }
        #[cfg(any(feature = "gui", test))]
        ActionIntent::OpenPath(path) => {
            if allowed(Capability::OpenPath) {
                GatedAction::allowed(
                    ActionKind::OpenPath(PathBuf::from(path)),
                    ActionRisk::Normal,
                )
            } else {
                GatedAction::blocked(
                    "Blocked: JSON action requires permissions = [\"filesystem\"] or [\"open_path\"]",
                )
            }
        }
        ActionIntent::Copy(text) => {
            if allowed(Capability::ClipboardWrite) {
                GatedAction::allowed(ActionKind::Copy(text), ActionRisk::Normal)
            } else {
                GatedAction::blocked(
                    "Blocked: JSON action requires permissions = [\"clipboard_write\"]",
                )
            }
        }
        #[cfg(any(feature = "gui", test))]
        ActionIntent::Shell(command) => {
            if allowed(Capability::Shell) {
                GatedAction::allowed(
                    ActionKind::Shell(ShellCommand::new(command)),
                    ActionRisk::Shell,
                )
            } else {
                GatedAction::blocked("Blocked: JSON action requires permissions = [\"shell\"]")
            }
        }
        ActionIntent::ExtensionItem { binary, id } => {
            if allowed(Capability::Shell) {
                GatedAction::allowed(ActionKind::ExtensionItem { binary, id }, ActionRisk::Shell)
            } else {
                GatedAction::blocked(
                    "Blocked: extension result requires capabilities = [\"shell\"]",
                )
            }
        }
        #[cfg(any(feature = "gui", test))]
        ActionIntent::None => GatedAction::allowed(ActionKind::None, ActionRisk::Normal),
    }
}

#[cfg(any(feature = "gui", test))]
fn parse_json_action_kind(
    value: &serde_json::Value,
    capabilities: &[Capability],
) -> ParsedJsonActionKind {
    let action = value.get("action").unwrap_or(value);
    let action_type = json_string(action, "type")
        .or_else(|| json_string(value, "action_type"))
        .unwrap_or_else(|| "none".to_string());
    let action_value = json_string(action, "value")
        .or_else(|| json_string(value, "value"))
        .unwrap_or_default();

    let intent = match action_type.as_str() {
        "open_url" | "url" if !action_value.is_empty() => ActionIntent::OpenUrl(action_value),
        "open_path" | "path" if !action_value.is_empty() => ActionIntent::OpenPath(action_value),
        "copy" | "copy_text" if !action_value.is_empty() => ActionIntent::Copy(action_value),
        "shell" | "run" if !action_value.is_empty() => ActionIntent::Shell(action_value),
        "none" => ActionIntent::None,
        _ => {
            return ParsedJsonActionKind::blocked(format!(
                "Unsupported JSON action type: {action_type}"
            ));
        }
    };

    let gated = gate_action_intent(intent, capabilities);
    ParsedJsonActionKind {
        kind: gated.kind,
        denial: gated.denial,
        risk: gated.risk,
    }
}

fn json_url_requires_capability(value: &str) -> bool {
    !value.trim().to_lowercase().starts_with("file://")
}

#[cfg(any(feature = "gui", test))]
fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn match_command_entry(entry: &CommandEntry, query: &str) -> Option<CommandMatch> {
    let trimmed = query.trim();
    if let Some(keyword) = &entry.keyword {
        let normalized_query = normalize_alias(trimmed);
        if normalized_query == *keyword {
            let binding = bind_command_arguments(&entry.arguments, "");
            return Some(CommandMatch {
                score: 980,
                argument: String::new(),
                args: binding.values,
                missing: binding.missing,
                direct: true,
            });
        }

        if let Some(argument) = keyword_argument(trimmed, keyword) {
            let binding = bind_command_arguments(&entry.arguments, &argument);
            return Some(CommandMatch {
                score: 980 + (!argument.is_empty()) as i32 * 20,
                argument,
                args: binding.values,
                missing: binding.missing,
                direct: true,
            });
        }
    }

    if trimmed.is_empty() {
        let binding = bind_command_arguments(&entry.arguments, "");
        return Some(CommandMatch {
            score: 0,
            argument: String::new(),
            args: binding.values,
            missing: binding.missing,
            direct: false,
        });
    }

    let binding = bind_command_arguments(&entry.arguments, trimmed);
    Some(CommandMatch {
        score: fuzzy_score(&entry.search_text(), trimmed)?,
        argument: trimmed.to_string(),
        args: binding.values,
        missing: binding.missing,
        direct: false,
    })
}

fn keyword_argument(query: &str, keyword: &str) -> Option<String> {
    let trimmed = query.trim();
    let (candidate, argument) = trimmed.split_once(char::is_whitespace)?;
    (normalize_alias(candidate) == keyword).then(|| argument.trim().to_string())
}

pub(crate) fn command_preferences(
    entry: &CommandEntry,
    global_preferences: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut preferences = entry.preferences.clone();
    preferences.extend(global_preferences.clone());
    preferences
}

pub(crate) fn command_env(
    entry: &CommandEntry,
    context: &PlaceholderContext<'_>,
) -> HashMap<String, String> {
    entry
        .env
        .iter()
        .map(|(key, value)| (key.clone(), expand_placeholders(value, context)))
        .collect()
}

fn bind_command_arguments(arguments: &[CommandArgument], input: &str) -> ArgumentBinding {
    let tokens = split_argument_tokens(input);
    let mut values = HashMap::new();
    let mut missing = Vec::new();
    let mut token_index = 0;

    for (argument_index, argument) in arguments.iter().enumerate() {
        let is_last = argument_index + 1 == arguments.len();
        let can_consume_rest = argument.kind == CommandArgumentKind::Text
            && token_index < tokens.len()
            && (is_last
                || arguments[argument_index + 1..]
                    .iter()
                    .all(|argument| !argument.required));
        let value = if can_consume_rest {
            let value = tokens[token_index..].join(" ");
            token_index = tokens.len();
            value
        } else if token_index < tokens.len() {
            let value = tokens[token_index].clone();
            token_index += 1;
            value
        } else {
            argument.default.clone()
        };

        let value_is_invalid = (argument.required && value.trim().is_empty())
            || (!value.trim().is_empty() && !argument_accepts_value(argument, &value));
        if value_is_invalid {
            missing.push(argument.name.clone());
        }

        values.insert(argument.name.clone(), value);
    }

    ArgumentBinding { values, missing }
}

fn split_argument_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ch if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn argument_accepts_value(argument: &CommandArgument, value: &str) -> bool {
    match argument.kind {
        CommandArgumentKind::Text | CommandArgumentKind::Path => true,
        CommandArgumentKind::Number => value.parse::<f64>().is_ok(),
        CommandArgumentKind::Bool => matches!(
            value.trim().to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "1" | "0" | "on" | "off"
        ),
        CommandArgumentKind::Enum => {
            argument.options.is_empty() || argument.options.iter().any(|option| option == value)
        }
    }
}

fn command_subtitle(entry: &CommandEntry, command: &str, missing: &[String]) -> String {
    let mut parts = Vec::new();

    if !missing.is_empty() {
        parts.push(format!("Missing argument: {}", missing.join(", ")));
    }
    if !entry.description.trim().is_empty() {
        parts.push(entry.description.clone());
    }
    if let Some(keyword) = &entry.keyword {
        let usage = if entry.argument_hint.trim().is_empty() {
            keyword.clone()
        } else {
            format!("{keyword} {}", entry.argument_hint.trim())
        };
        parts.push(usage);
    }
    if parts.is_empty() {
        parts.push(command.to_string());
    }

    tagged_subtitle(&parts.join(" - "), &entry.tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ExecutionRequest;
    use crate::placeholders::PlaceholderContext;

    fn no_context() -> PlaceholderContext<'static> {
        PlaceholderContext::new("", None)
    }

    fn entry(toml: &str) -> CommandEntry {
        parse_command_entry(toml).expect("command TOML parses")
    }

    fn empty_manifest() -> ExtensionOrigin {
        ExtensionOrigin {
            id: "example.empty".to_string(),
            name: "Empty".to_string(),
            version: "0.1.0".to_string(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn shell_command_without_shell_permission_has_no_form() {
        let command = entry(
            r#"
name = "Deploy"
command = "deploy {{arg:env}}"
keyword = "deploy"
arguments = [ { name = "env", type = "text", required = true } ]
"#,
        );

        let actions = search_commands(&[command], "deploy", &no_context());
        assert_eq!(actions.len(), 1);
        // B-1: the missing-argument branch must not hand out a form that
        // bypasses the `shell` permission.
        assert!(matches!(actions[0].kind, ActionKind::None));
        assert!(actions[0].subtitle.starts_with("Blocked"));
        assert!(!actions[0].risk.requires_confirmation());
    }

    #[test]
    fn shell_command_with_shell_permission_offers_a_confirming_form() {
        let command = entry(
            r#"
name = "Deploy"
command = "deploy {{arg:env}}"
keyword = "deploy"
permissions = ["shell"]
arguments = [ { name = "env", type = "text", required = true } ]
"#,
        );

        let actions = search_commands(&[command], "deploy", &no_context());
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.risk, ActionRisk::Shell);
        let ActionKind::Form(form) = &action.kind else {
            panic!("expected a form, got {:?}", action.kind);
        };
        assert!(form.capabilities.allows(Capability::Shell));
        assert_eq!(form.risk, ActionRisk::Shell);
        assert!(action.capabilities.allows(Capability::Shell));
    }

    #[test]
    fn argv_command_requires_confirmation() {
        let command = entry(
            r#"
name = "Safe Echo"
mode = "argv"
program = "printf"
args = ["%s", "{{query}}"]
keyword = "safe"
"#,
        );

        let actions = search_commands(&[command], "safe hello", &no_context());
        assert_eq!(actions.len(), 1);
        // P1.4: argv no longer runs without a prompt.
        assert_eq!(actions[0].risk, ActionRisk::Shell);
        assert!(matches!(
            actions[0].execution_request(),
            Some(ExecutionRequest::Command(_))
        ));
    }

    #[test]
    fn argv_sh_c_is_not_silent() {
        let command = entry(
            r#"
name = "Sneaky"
mode = "argv"
program = "/bin/sh"
args = ["-c", "touch /tmp/zeshicast-pwned"]
keyword = "sneaky"
"#,
        )
        .with_extension_origin(empty_manifest());

        let actions = search_commands(&[command], "sneaky", &no_context());
        assert_eq!(actions.len(), 1);
        // B-6: an extension cannot smuggle `sh -c` past the manifest gate.
        assert!(matches!(actions[0].kind, ActionKind::None));
        assert!(actions[0].subtitle.starts_with("Blocked"));
        assert!(actions[0].execution_request().is_none());
    }

    #[test]
    fn command_cannot_self_grant_shell_inside_empty_manifest() {
        let command = entry(
            r#"
name = "Sneaky"
command = "touch /tmp/zeshicast-pwned"
permissions = ["shell"]
"#,
        )
        .with_extension_origin(empty_manifest());

        // P1.8: the manifest is a ceiling, so the self-declared shell
        // permission does not survive the intersection.
        assert!(command.capabilities.is_empty());
        assert!(command.permissions.is_empty());

        let actions = search_commands(&[command], "", &no_context());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].kind, ActionKind::None));
    }

    #[test]
    fn json_file_url_requires_open_path_capability() {
        let file_url = ActionIntent::OpenUrl("file:///etc/passwd".to_string());

        // `open_url` alone must not be enough to read a local path.
        let with_url = gate_action_intent(file_url.clone(), &[Capability::OpenUrl]);
        assert!(matches!(with_url.kind, ActionKind::None));
        assert!(with_url.denial.is_some());

        let with_network = gate_action_intent(file_url.clone(), &[Capability::Network]);
        assert!(matches!(with_network.kind, ActionKind::None));

        let with_filesystem = gate_action_intent(file_url, &[Capability::Filesystem]);
        assert!(matches!(with_filesystem.kind, ActionKind::OpenUrl(_)));
    }

    #[test]
    fn json_result_inherits_producer_capabilities() {
        let output =
            r#"[{"title":"Open","action":{"type":"open_url","value":"https://example.com"}}]"#;

        let allowed = parse_json_actions(output, "Docs", 10, &[Capability::OpenUrl]);
        assert_eq!(allowed.len(), 1);
        assert!(allowed[0].capabilities.allows(Capability::OpenUrl));

        // A producer without the capability still yields a row (with a denial
        // note), and the action must not carry the daemon's trusted ceiling.
        let denied = parse_json_actions(output, "Docs", 10, &[]);
        assert_eq!(denied.len(), 1);
        assert!(matches!(denied[0].kind, ActionKind::None));
        assert!(!denied[0].capabilities.allows(Capability::OpenUrl));
    }

    #[test]
    fn gate_action_intent_blocks_an_extension_item_without_shell() {
        let item = || ActionIntent::ExtensionItem {
            binary: PathBuf::from("/usr/bin/demo-extension"),
            id: "deploy".to_string(),
        };

        let blocked = gate_action_intent(item(), &[]);
        assert!(matches!(blocked.kind, ActionKind::None));
        assert!(blocked.denial.is_some());

        let allowed = gate_action_intent(item(), &[Capability::Shell]);
        assert!(
            matches!(allowed.kind, ActionKind::ExtensionItem { .. }),
            "an item must stay an item: never a shell command line"
        );
        assert_eq!(allowed.risk, ActionRisk::Shell);
        assert!(allowed.risk.requires_confirmation());
    }
}
