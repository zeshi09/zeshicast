use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    Action, ActionForm, ActionFormField, ActionKind, CommandArgumentKind, MAX_RESULTS,
    PlaceholderContext, ShellCommand, expand_placeholders, fuzzy_score, normalize_alias,
    tagged_subtitle, toml_value_string,
};

#[derive(Debug, Clone)]
pub(crate) struct CommandEntry {
    pub(crate) name: String,
    pub(crate) command: String,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandMode {
    Shell,
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
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut commands = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let Some(command) = fs::read_to_string(&path)
            .ok()
            .and_then(|content| parse_command_entry(&content))
        else {
            eprintln!("failed to parse command: {}", path.display());
            continue;
        };
        commands.push(command);
    }

    commands
}

pub(crate) fn parse_command_entry(input: &str) -> Option<CommandEntry> {
    let table = input.parse::<toml::Table>().ok()?;
    let name = toml_required_string(&table, "name")?;
    let command = toml_required_string(&table, "command")?;
    let env = parse_env_table(table.get("env"));
    let mode = toml_optional_string(&table, "mode")
        .as_deref()
        .and_then(parse_command_mode)
        .unwrap_or(CommandMode::Shell);
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

    Some(CommandEntry {
        name,
        command,
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
    })
}

fn parse_command_mode(mode: &str) -> Option<CommandMode> {
    match mode.trim().to_lowercase().as_str() {
        "shell" | "run" => Some(CommandMode::Shell),
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
    context: &PlaceholderContext,
) -> Vec<Action> {
    entries
        .iter()
        .flat_map(|entry| {
            let command_match = match_command_entry(entry, query)?;
            let command_context = PlaceholderContext {
                query: command_match.argument.clone(),
                clipboard: context.clipboard.clone(),
                args: command_match.args.clone(),
                preferences: command_preferences(entry, &context.preferences),
                now: context.now,
            };
            let command = expand_placeholders(&entry.command, &command_context);
            let env = command_env(entry, &command_context);
            let shell_command = ShellCommand::with_env(command, env);

            if entry.mode == CommandMode::Json
                && command_match.direct
                && command_match.missing.is_empty()
            {
                return Some(search_json_command(
                    entry,
                    shell_command,
                    command_match.score,
                ));
            }

            let command = shell_command.command.clone();
            let subtitle = command_subtitle(entry, &command, &command_match.missing);
            let (kind, icon_name) = if command_match.missing.is_empty() {
                (ActionKind::Shell(shell_command), entry.icon_name.as_str())
            } else {
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
                    })
                    .collect();
                let form = ActionForm {
                    name: entry.name.clone(),
                    fields,
                    command: entry.command.clone(),
                    env: entry.env.clone(),
                    preferences: command_preferences(entry, &context.preferences),
                    current_args: command_match.args.clone(),
                    partial_query: command_match.argument.clone(),
                };
                (ActionKind::Form(form), "dialog-question-symbolic")
            };

            Some(vec![
                Action::new(&entry.category, &entry.name, kind, command_match.score + 85)
                    .with_subtitle(subtitle)
                    .with_icon(icon_name),
            ])
        })
        .flatten()
        .collect()
}

fn search_json_command(entry: &CommandEntry, command: ShellCommand, score: i32) -> Vec<Action> {
    match run_json_command(&command) {
        Ok(stdout) => parse_json_actions(&stdout, &entry.category, score + 100),
        Err(error) => vec![
            Action::new(&entry.category, &entry.name, ActionKind::None, score + 1)
                .with_subtitle(format!("JSON command failed: {error}"))
                .with_icon("dialog-warning-symbolic"),
        ],
    }
}

fn run_json_command(command: &ShellCommand) -> io::Result<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command.command)
        .envs(&command.env)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(
            stderr.trim().chars().take(160).collect::<String>(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn parse_json_actions(input: &str, category: &str, base_score: i32) -> Vec<Action> {
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
        .take(MAX_RESULTS)
        .enumerate()
        .filter_map(|(index, value)| parse_json_action(value, category, base_score - index as i32))
        .collect()
}

fn parse_json_action(value: &serde_json::Value, category: &str, score: i32) -> Option<Action> {
    let title = json_string(value, "title")?;
    let subtitle = json_string(value, "subtitle").unwrap_or_default();
    let icon_name = json_string(value, "icon").unwrap_or_else(|| "system-run-symbolic".to_string());
    let kind = parse_json_action_kind(value);

    Some(
        Action::new(
            json_string(value, "category").unwrap_or_else(|| category.to_string()),
            title,
            kind,
            score,
        )
        .with_subtitle(subtitle)
        .with_icon(icon_name),
    )
}

fn parse_json_action_kind(value: &serde_json::Value) -> ActionKind {
    let action = value.get("action").unwrap_or(value);
    let action_type = json_string(action, "type")
        .or_else(|| json_string(value, "action_type"))
        .unwrap_or_else(|| "none".to_string());
    let action_value = json_string(action, "value")
        .or_else(|| json_string(value, "value"))
        .unwrap_or_default();

    match action_type.as_str() {
        "open_url" | "url" if !action_value.is_empty() => ActionKind::OpenUrl(action_value),
        "open_path" | "path" if !action_value.is_empty() => {
            ActionKind::OpenPath(PathBuf::from(action_value))
        }
        "copy" | "copy_text" if !action_value.is_empty() => ActionKind::Copy(action_value),
        "shell" | "run" if !action_value.is_empty() => {
            ActionKind::Shell(ShellCommand::new(action_value))
        }
        _ => ActionKind::None,
    }
}

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
    context: &PlaceholderContext,
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

        if argument.required && value.trim().is_empty() {
            missing.push(argument.name.clone());
        } else if !value.trim().is_empty() && !argument_accepts_value(argument, &value) {
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
