use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

const MAX_FILE_DEPTH: usize = 5;
const MAX_INDEXED_FILES: usize = 10_000;
const MAX_CLIPBOARD_ENTRIES: usize = 100;
const MAX_CLIPBOARD_TEXT_BYTES: usize = 20_000;
const MAX_RESULTS: usize = 12;

#[derive(Debug, Clone)]
pub struct Zeshicast {
    apps: Vec<AppEntry>,
    quicklinks: Vec<NamedValue>,
    snippets: Vec<NamedValue>,
    commands: Vec<CommandEntry>,
    clipboard_history: Vec<String>,
    preferences: HashMap<String, String>,
    aliases: HashMap<String, String>,
    pins: HashSet<String>,
    recent: Vec<String>,
    files: Vec<FileEntry>,
    config_dir: PathBuf,
}

impl Zeshicast {
    pub fn load() -> Self {
        let home = home_dir();
        let config_dir = home.join(".config/zeshicast");
        Self {
            apps: load_apps(&home),
            quicklinks: load_named_values(&config_dir.join("quicklinks.txt")),
            snippets: load_named_values(&config_dir.join("snippets.txt")),
            commands: load_command_entries(&config_dir.join("commands")),
            clipboard_history: load_clipboard_history(&config_dir.join("clipboard.txt")),
            preferences: load_preferences(&config_dir.join("preferences.toml")),
            aliases: load_aliases(&config_dir.join("aliases.txt")),
            pins: load_lines(&config_dir.join("pins.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            recent: load_lines(&config_dir.join("recent.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            files: load_file_index(&home),
            config_dir,
        }
    }

    pub fn reload(&mut self) {
        *self = Self::load();
    }

    pub fn search(&self, query: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        let trimmed = query.trim();
        let lower = trimmed.to_lowercase();
        let context = PlaceholderContext::new(trimmed, self.clipboard_history.first());
        let context = context.with_preferences(self.preferences.clone());

        if trimmed.is_empty() {
            actions.extend(self.default_actions(&context));
            for action in &mut actions {
                action.score += self.config_score(action, trimmed);
            }
            actions.sort_by(|left, right| {
                right
                    .score
                    .cmp(&left.score)
                    .then_with(|| left.title.cmp(&right.title))
            });
            actions.truncate(MAX_RESULTS);
            return actions;
        }

        if lower.starts_with("calc ") || looks_like_expression(trimmed) {
            let expr = trimmed.strip_prefix("calc ").unwrap_or(trimmed).trim();
            match Calculator::new(expr).parse() {
                Ok(value) => actions.push(
                    Action::new(
                        "Calculator",
                        format!("{expr} = {}", format_number(value)),
                        ActionKind::Copy(format_number(value)),
                        1000,
                    )
                    .with_subtitle("Copy result to clipboard")
                    .with_icon("accessories-calculator-symbolic"),
                ),
                Err(error) if lower.starts_with("calc ") => actions.push(
                    Action::new(
                        "Calculator",
                        format!("Invalid expression: {error}"),
                        ActionKind::None,
                        1,
                    )
                    .with_subtitle(expr)
                    .with_icon("dialog-warning-symbolic"),
                ),
                Err(_) => {}
            }
        }

        actions.extend(search_apps(&self.apps, trimmed));
        actions.extend(search_named_values(
            "Quicklink",
            &self.quicklinks,
            trimmed,
            ActionTarget::OpenUrl,
            &context,
        ));
        actions.extend(search_named_values(
            "Snippet",
            &self.snippets,
            trimmed,
            ActionTarget::CopyText,
            &context,
        ));
        actions.extend(search_commands(&self.commands, trimmed, &context));
        actions.extend(search_system_actions(trimmed));
        actions.extend(search_audio_actions(trimmed));
        actions.extend(search_network_actions(trimmed));
        actions.extend(search_niri_actions(trimmed));
        actions.extend(search_ai(trimmed, &self.preferences));
        actions.extend(search_translate(trimmed, &self.preferences));

        if lower.starts_with("clip ") || lower.starts_with("clipboard ") {
            let needle = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            actions.extend(search_clipboard(&self.clipboard_history, needle, true));
        } else if trimmed.len() >= 3 {
            actions.extend(search_clipboard(&self.clipboard_history, trimmed, false));
        }

        if lower.starts_with("file ") || lower.starts_with("find ") {
            let needle = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            actions.extend(search_files(&self.files, needle, true));
        } else if trimmed.len() >= 2 {
            actions.extend(search_files(&self.files, trimmed, false));
        }

        if lower.starts_with("proc ") || lower.starts_with("process ") {
            let needle = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            actions.extend(search_processes(needle));
        }

        if lower.starts_with("shell ") {
            let command = trimmed.strip_prefix("shell ").unwrap_or_default().trim();
            if !command.is_empty() {
                actions.push(
                    Action::new(
                        "Shell",
                        command,
                        ActionKind::Shell(ShellCommand::new(command)),
                        500,
                    )
                    .with_subtitle("Run with sh -c")
                    .with_icon("utilities-terminal-symbolic"),
                );
            }
        }

        for action in &mut actions {
            action.score += self.config_score(action, trimmed);
        }

        actions.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        });
        actions.truncate(MAX_RESULTS);
        actions
    }

    pub fn run_action(&mut self, action: &Action) {
        action.run();
        if let Err(error) = self.record_recent(action) {
            eprintln!("failed to record recent action: {error}");
        }
    }

    pub fn available_secondary_actions(&self, action: &Action) -> Vec<SecondaryAction> {
        let mut actions = vec![
            SecondaryAction::new(
                SecondaryActionKind::Run,
                "Run",
                "media-playback-start-symbolic",
            ),
            SecondaryAction::new(
                SecondaryActionKind::CopyValue,
                "Copy Value",
                "edit-copy-symbolic",
            ),
        ];

        if action.parent_dir().is_some() {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::OpenParent,
                "Open Containing Folder",
                "folder-open-symbolic",
            ));
        }

        if action.category == "Clipboard" {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::DeleteClipboardItem,
                "Delete Clipboard Item",
                "edit-delete-symbolic",
            ));
            actions.push(SecondaryAction::new(
                SecondaryActionKind::ClearClipboardHistory,
                "Clear Clipboard History",
                "edit-clear-symbolic",
            ));
        }

        if self.is_pinned(action) {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Unpin,
                "Unpin",
                "view-pin-symbolic",
            ));
        } else {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Pin,
                "Pin",
                "view-pin-symbolic",
            ));
        }

        actions
    }

    pub fn run_secondary_action(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
    ) -> io::Result<()> {
        match secondary {
            SecondaryActionKind::Run => self.run_action(action),
            SecondaryActionKind::CopyValue => action.copy_value(),
            SecondaryActionKind::OpenParent => action.open_parent_dir(),
            SecondaryActionKind::Pin => self.pin_action(action)?,
            SecondaryActionKind::Unpin => self.unpin_action(action)?,
            SecondaryActionKind::DeleteClipboardItem => self.delete_clipboard_item(action)?,
            SecondaryActionKind::ClearClipboardHistory => self.clear_clipboard_history()?,
        }
        Ok(())
    }

    pub fn add_clipboard_text(&mut self, text: &str) -> io::Result<bool> {
        let text = normalize_clipboard_text(text);
        if text.is_empty() {
            return Ok(false);
        }

        self.clipboard_history.retain(|entry| entry != &text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(MAX_CLIPBOARD_ENTRIES);
        write_clipboard_history(
            &self.config_dir.join("clipboard.txt"),
            &self.clipboard_history,
        )?;
        Ok(true)
    }

    pub fn delete_clipboard_item(&mut self, action: &Action) -> io::Result<()> {
        let value = action.value();
        self.clipboard_history.retain(|entry| entry != &value);
        write_clipboard_history(
            &self.config_dir.join("clipboard.txt"),
            &self.clipboard_history,
        )
    }

    pub fn clear_clipboard_history(&mut self) -> io::Result<()> {
        self.clipboard_history.clear();
        write_clipboard_history(
            &self.config_dir.join("clipboard.txt"),
            &self.clipboard_history,
        )
    }

    pub fn set_alias_for_action(&mut self, alias: &str, action: &Action) -> io::Result<String> {
        let alias = normalize_alias(alias);
        if alias.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "alias is empty",
            ));
        }

        let target = action.title.clone();
        append_alias(&self.config_dir, &alias, &target)?;
        self.aliases.insert(alias.clone(), target);
        Ok(alias)
    }

    pub fn is_pinned(&self, action: &Action) -> bool {
        let title = action.title.to_lowercase();
        let identity = action.identity().to_lowercase();
        self.pins.contains(&title) || self.pins.contains(&identity)
    }

    pub fn pin_action(&mut self, action: &Action) -> io::Result<()> {
        self.pins.insert(action.identity().to_lowercase());
        self.write_pins()
    }

    pub fn unpin_action(&mut self, action: &Action) -> io::Result<()> {
        let title = action.title.to_lowercase();
        let identity = action.identity().to_lowercase();
        self.pins.remove(&title);
        self.pins.remove(&identity);
        self.write_pins()
    }

    fn default_actions(&self, context: &PlaceholderContext) -> Vec<Action> {
        let mut actions = Vec::new();

        actions.extend(self.apps.iter().map(|app| app_action(app, 20)));
        actions.extend(search_named_values(
            "Quicklink",
            &self.quicklinks,
            "",
            ActionTarget::OpenUrl,
            context,
        ));
        actions.extend(search_named_values(
            "Snippet",
            &self.snippets,
            "",
            ActionTarget::CopyText,
            context,
        ));
        actions.extend(search_commands(&self.commands, "", context));

        actions
    }

    pub fn run_form_action(&mut self, action: &Action, values: HashMap<String, String>) {
        let ActionKind::Form(form) = &action.kind else {
            return;
        };
        let mut args = form.current_args.clone();
        args.extend(values);
        let context = PlaceholderContext {
            query: form.partial_query.clone(),
            clipboard: self.clipboard_history.first().cloned().unwrap_or_default(),
            args,
            preferences: form.preferences.clone(),
            now: SystemTime::now(),
        };
        let command = expand_placeholders(&form.command, &context);
        let env = form
            .env
            .iter()
            .map(|(k, v)| (k.clone(), expand_placeholders(v, &context)))
            .collect();
        spawn_shell(&ShellCommand::with_env(command, env));
        if let Err(e) = self.record_recent(action) {
            eprintln!("failed to record recent: {e}");
        }
    }

    pub fn get_preferences(&self) -> &HashMap<String, String> {
        &self.preferences
    }

    pub fn set_preference(&mut self, key: String, value: String) -> io::Result<()> {
        if value.is_empty() {
            self.preferences.remove(&key);
        } else {
            self.preferences.insert(key, value);
        }
        write_preferences(&self.config_dir.join("preferences.toml"), &self.preferences)
    }

    pub fn list_commands(&self) -> Vec<CommandSummary> {
        self.commands
            .iter()
            .map(|e| CommandSummary {
                name: e.name.clone(),
                category: e.category.clone(),
                description: e.description.clone(),
                keyword: e.keyword.clone(),
                icon_name: e.icon_name.clone(),
                tags: e.tags.clone(),
            })
            .collect()
    }

    fn record_recent(&mut self, action: &Action) -> io::Result<()> {
        let identity = action.identity().to_lowercase();
        self.recent.retain(|entry| entry != &identity);
        self.recent.insert(0, identity);
        self.recent.truncate(50);
        write_lines(&self.config_dir.join("recent.txt"), &self.recent)
    }

    fn write_pins(&self) -> io::Result<()> {
        let mut pins = self.pins.iter().cloned().collect::<Vec<_>>();
        pins.sort();
        write_lines(&self.config_dir.join("pins.txt"), &pins)
    }

    fn config_score(&self, action: &Action, query: &str) -> i32 {
        let mut score = 0;
        let title_lower = action.title.to_lowercase();
        let identity_lower = action.identity().to_lowercase();

        if self.pins.contains(&title_lower) || self.pins.contains(&identity_lower) {
            score += 700;
        }

        if let Some(index) = self
            .recent
            .iter()
            .position(|entry| entry == &identity_lower)
        {
            score += if query.is_empty() {
                650 - index as i32
            } else {
                60
            };
        }

        for (alias, target) in &self.aliases {
            let target = target.to_lowercase();
            if target == title_lower || target == identity_lower {
                if alias == &normalize_alias(query) {
                    score += 900;
                } else if fuzzy_score(alias, query).is_some() {
                    score += 250;
                }
            }
        }

        score
    }
}

#[derive(Debug, Clone)]
struct AppEntry {
    name: String,
    exec: String,
    comment: Option<String>,
}

#[derive(Debug, Clone)]
struct NamedValue {
    name: String,
    value: String,
    tags: Vec<String>,
}

impl NamedValue {
    fn search_text(&self) -> String {
        if self.tags.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, self.tags.join(" "))
        }
    }
}

#[derive(Debug, Clone)]
struct CommandEntry {
    name: String,
    command: String,
    env: HashMap<String, String>,
    mode: CommandMode,
    category: String,
    keyword: Option<String>,
    argument_hint: String,
    arguments: Vec<CommandArgument>,
    preferences: HashMap<String, String>,
    tags: Vec<String>,
    icon_name: String,
    description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandMode {
    Shell,
    Json,
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

#[derive(Debug, Clone)]
struct CommandArgument {
    name: String,
    kind: CommandArgumentKind,
    required: bool,
    default: String,
    options: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgumentKind {
    Text,
    Number,
    Path,
    Bool,
    Enum,
}

#[derive(Debug, Clone)]
pub struct ActionFormField {
    pub name: String,
    pub kind: CommandArgumentKind,
    pub required: bool,
    pub default: String,
    pub options: Vec<String>,
    pub current_value: String,
}

#[derive(Debug, Clone)]
pub struct ActionForm {
    pub name: String,
    pub fields: Vec<ActionFormField>,
    pub(crate) command: String,
    pub(crate) env: HashMap<String, String>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) current_args: HashMap<String, String>,
    pub(crate) partial_query: String,
}

#[derive(Debug, Clone)]
pub enum HttpRequest {
    Translate {
        endpoint: String,
        text: String,
        target: String,
        api_key: String,
    },
    AiChat {
        endpoint: String,
        model: String,
        query: String,
        api_key: String,
    },
}

#[derive(Debug, Clone)]
pub struct CommandSummary {
    pub name: String,
    pub category: String,
    pub description: String,
    pub keyword: Option<String>,
    pub icon_name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct PlaceholderContext {
    query: String,
    clipboard: String,
    args: HashMap<String, String>,
    preferences: HashMap<String, String>,
    now: SystemTime,
}

impl PlaceholderContext {
    fn new(query: &str, clipboard: Option<&String>) -> Self {
        Self {
            query: query.to_string(),
            clipboard: clipboard.cloned().unwrap_or_default(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: SystemTime::now(),
        }
    }

    fn with_preferences(mut self, preferences: HashMap<String, String>) -> Self {
        self.preferences = preferences;
        self
    }
}

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

#[derive(Debug, Clone)]
struct SystemActionEntry {
    title: &'static str,
    subtitle: &'static str,
    command: &'static str,
    icon_name: &'static str,
    hazardous: bool,
}

#[derive(Debug, Clone)]
struct ProcessEntry {
    pid: u32,
    name: String,
    command: String,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub category: String,
    pub title: String,
    pub subtitle: String,
    pub icon_name: String,
    kind: ActionKind,
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryActionKind {
    Run,
    CopyValue,
    OpenParent,
    Pin,
    Unpin,
    DeleteClipboardItem,
    ClearClipboardHistory,
}

#[derive(Debug, Clone)]
pub struct SecondaryAction {
    pub kind: SecondaryActionKind,
    pub title: String,
    pub icon_name: String,
}

impl SecondaryAction {
    fn new(
        kind: SecondaryActionKind,
        title: impl Into<String>,
        icon_name: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            icon_name: icon_name.into(),
        }
    }
}

impl Action {
    fn new(
        category: impl Into<String>,
        title: impl Into<String>,
        kind: ActionKind,
        score: i32,
    ) -> Self {
        Self {
            category: category.into(),
            title: title.into(),
            subtitle: String::new(),
            icon_name: "system-run-symbolic".to_string(),
            kind,
            score,
        }
    }

    fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    fn with_icon(mut self, icon_name: impl Into<String>) -> Self {
        self.icon_name = icon_name.into();
        self
    }

    pub fn run(&self) {
        match &self.kind {
            ActionKind::Launch(command) => spawn_shell(&ShellCommand::new(command)),
            ActionKind::OpenPath(path) => {
                spawn_command("xdg-open", &[path.to_string_lossy().as_ref()])
            }
            ActionKind::OpenUrl(url) => spawn_command("xdg-open", &[url]),
            ActionKind::Copy(text) => copy_to_clipboard(text),
            ActionKind::Shell(command) => spawn_shell(command),
            ActionKind::HttpCopy(req) => {
                if let Some(result) = execute_http_request(req) {
                    copy_to_clipboard(&result);
                } else {
                    eprintln!("http request failed");
                }
            }
            ActionKind::Form(_) => {}
            ActionKind::None => {}
        }
    }

    pub fn form_data(&self) -> Option<&ActionForm> {
        match &self.kind {
            ActionKind::Form(f) => Some(f),
            _ => None,
        }
    }

    pub fn copy_value(&self) {
        copy_to_clipboard(&self.value());
    }

    pub fn value(&self) -> String {
        match &self.kind {
            ActionKind::Launch(command) | ActionKind::OpenUrl(command) => command.clone(),
            ActionKind::Shell(command) => command.command.clone(),
            ActionKind::OpenPath(path) => path.display().to_string(),
            ActionKind::Copy(text) => text.clone(),
            ActionKind::HttpCopy(req) => match req {
                HttpRequest::Translate { text, .. } => text.clone(),
                HttpRequest::AiChat { query, .. } => query.clone(),
            },
            ActionKind::Form(form) => form.command.clone(),
            ActionKind::None => self.title.clone(),
        }
    }

    pub fn parent_dir(&self) -> Option<PathBuf> {
        match &self.kind {
            ActionKind::OpenPath(path) => path.parent().map(Path::to_path_buf),
            _ => None,
        }
    }

    pub fn open_parent_dir(&self) {
        if let Some(parent) = self.parent_dir() {
            spawn_command("xdg-open", &[parent.to_string_lossy().as_ref()]);
        }
    }

    pub fn identity(&self) -> String {
        format!("{}:{}", self.category, self.title)
    }
}

#[derive(Debug, Clone)]
enum ActionKind {
    Launch(String),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    Shell(ShellCommand),
    HttpCopy(HttpRequest),
    Form(ActionForm),
    None,
}

#[derive(Debug, Clone)]
struct ShellCommand {
    command: String,
    env: HashMap<String, String>,
}

impl ShellCommand {
    fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            env: HashMap::new(),
        }
    }

    fn with_env(command: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            command: command.into(),
            env,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ActionTarget {
    OpenUrl,
    CopyText,
}

fn load_apps(home: &Path) -> Vec<AppEntry> {
    let xdg_data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));

    let xdg_data_dirs_raw = env::var_os("XDG_DATA_DIRS")
        .unwrap_or_else(|| std::ffi::OsString::from("/usr/local/share:/usr/share"));

    let mut dirs = vec![xdg_data_home.join("applications")];
    for base in env::split_paths(&xdg_data_dirs_raw) {
        dirs.push(base.join("applications"));
    }

    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path) {
                let key = app.name.to_lowercase();
                if seen.insert(key) {
                    apps.push(app);
                }
            }
        }
    }

    apps
}

fn parse_desktop_file(path: &Path) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut comment = None;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Exec=") {
            exec = Some(clean_desktop_exec(value));
        } else if let Some(value) = line.strip_prefix("Comment=") {
            comment = Some(value.to_string());
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Hidden=true" {
            hidden = true;
        }
    }

    if no_display || hidden {
        return None;
    }

    Some(AppEntry {
        name: name?,
        exec: exec?,
        comment,
    })
}

fn clean_desktop_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|part| !part.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

fn search_apps(apps: &[AppEntry], query: &str) -> Vec<Action> {
    apps.iter()
        .filter_map(|app| {
            let haystack = match &app.comment {
                Some(comment) => format!("{} {comment}", app.name),
                None => app.name.clone(),
            };
            let score = fuzzy_score(&haystack, query)?;
            Some(app_action(app, score + 100))
        })
        .collect()
}

fn app_action(app: &AppEntry, score: i32) -> Action {
    Action::new(
        "App",
        &app.name,
        ActionKind::Launch(app.exec.clone()),
        score,
    )
    .with_subtitle(
        app.comment
            .as_deref()
            .filter(|comment| !comment.trim().is_empty())
            .unwrap_or(&app.exec),
    )
    .with_icon("application-x-executable-symbolic")
}

fn load_named_values(path: &Path) -> Vec<NamedValue> {
    load_lines(path)
        .into_iter()
        .filter_map(|line| parse_named_value(&line))
        .collect()
}

fn load_command_entries(dir: &Path) -> Vec<CommandEntry> {
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

fn parse_command_entry(input: &str) -> Option<CommandEntry> {
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

fn parse_named_value(line: &str) -> Option<NamedValue> {
    let (name_part, value) = line.split_once('=')?;
    let (name, tags) = match name_part.split_once('|') {
        Some((name, tags)) => (name.trim(), parse_tags(tags)),
        None => (name_part.trim(), Vec::new()),
    };

    if name.is_empty() {
        return None;
    }

    Some(NamedValue {
        name: name.to_string(),
        value: value.trim().to_string(),
        tags,
    })
}

fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn load_clipboard_history(path: &Path) -> Vec<String> {
    load_lines(path)
        .into_iter()
        .filter_map(|line| decode_clipboard_line(&line))
        .collect()
}

fn load_aliases(path: &Path) -> HashMap<String, String> {
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

fn write_preferences(path: &Path, preferences: &HashMap<String, String>) -> io::Result<()> {
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

fn load_preferences(path: &Path) -> HashMap<String, String> {
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

fn toml_value_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(value) => Some(value.trim().to_string()),
        toml::Value::Integer(value) => Some(value.to_string()),
        toml::Value::Float(value) => Some(value.to_string()),
        toml::Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn load_lines(path: &Path) -> Vec<String> {
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

fn append_alias(config_dir: &Path, alias: &str, target: &str) -> io::Result<()> {
    fs::create_dir_all(config_dir)?;
    let path = config_dir.join("aliases.txt");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{alias} = {target}")
}

fn write_lines(path: &Path, lines: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn write_clipboard_history(path: &Path, entries: &[String]) -> io::Result<()> {
    let lines = entries
        .iter()
        .map(|entry| encode_clipboard_line(entry))
        .collect::<Vec<_>>();
    write_lines(path, &lines)
}

fn normalize_alias(alias: &str) -> String {
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

fn expand_placeholders(template: &str, context: &PlaceholderContext) -> String {
    let mut output = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);

        let after_start = &after_start[2..];
        let Some(end) = after_start.find("}}") else {
            output.push_str("{{");
            output.push_str(after_start);
            return output;
        };

        let (placeholder, after_end) = after_start.split_at(end);
        output.push_str(&render_placeholder(placeholder.trim(), context));
        rest = &after_end[2..];
    }

    output.push_str(rest);
    output
}

fn render_placeholder(placeholder: &str, context: &PlaceholderContext) -> String {
    let (name, argument) = placeholder
        .split_once(':')
        .map(|(name, argument)| (name.trim(), Some(argument.trim())))
        .unwrap_or((placeholder, None));

    match name {
        "query" => context.query.clone(),
        "clipboard" => context.clipboard.clone(),
        "arg" => argument
            .and_then(|name| context.args.get(name))
            .cloned()
            .unwrap_or_default(),
        "pref" => argument
            .and_then(|name| context.preferences.get(name))
            .cloned()
            .unwrap_or_default(),
        "date" => format_local_time(context.now, argument.unwrap_or("%Y-%m-%d")),
        "time" => format_local_time(context.now, argument.unwrap_or("%H:%M:%S")),
        "datetime" | "timestamp" => {
            format_local_time(context.now, argument.unwrap_or("%Y-%m-%d %H:%M:%S"))
        }
        "calc" => argument
            .and_then(|expr| Calculator::new(expr).parse().ok())
            .map(format_number)
            .unwrap_or_default(),
        _ => format!("{{{{{placeholder}}}}}"),
    }
}

fn format_local_time(time: SystemTime, format: &str) -> String {
    DateTime::<Local>::from(time).format(format).to_string()
}

fn normalize_clipboard_text(text: &str) -> String {
    let text = text.replace('\0', "");
    if text.len() <= MAX_CLIPBOARD_TEXT_BYTES {
        return text.trim().to_string();
    }

    let mut truncated = String::new();
    for ch in text.chars() {
        if truncated.len() + ch.len_utf8() > MAX_CLIPBOARD_TEXT_BYTES {
            break;
        }
        truncated.push(ch);
    }
    truncated.trim().to_string()
}

fn clipboard_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 90 {
        collapsed
    } else {
        let mut preview = collapsed.chars().take(87).collect::<String>();
        preview.push_str("...");
        preview
    }
}

fn tagged_subtitle(value: &str, tags: &[String]) -> String {
    if tags.is_empty() {
        return value.to_string();
    }

    format!("{value}  [{}]", tags.join(", "))
}

fn encode_clipboard_line(text: &str) -> String {
    let mut encoded = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

fn decode_clipboard_line(line: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => decoded.push('\\'),
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some(other) => {
                decoded.push('\\');
                decoded.push(other);
            }
            None => decoded.push('\\'),
        }
    }

    let normalized = normalize_clipboard_text(&decoded);
    (!normalized.is_empty()).then_some(normalized)
}

fn search_named_values(
    category: &str,
    entries: &[NamedValue],
    query: &str,
    target: ActionTarget,
    context: &PlaceholderContext,
) -> Vec<Action> {
    entries
        .iter()
        .filter_map(|entry| {
            let score = if query.trim().is_empty() {
                0
            } else {
                fuzzy_score(&entry.search_text(), query)?
            };
            let value = expand_placeholders(&entry.value, context);
            let kind = match target {
                ActionTarget::OpenUrl => ActionKind::OpenUrl(value.clone()),
                ActionTarget::CopyText => ActionKind::Copy(value.clone()),
            };
            let icon_name = match target {
                ActionTarget::OpenUrl => "emblem-web-symbolic",
                ActionTarget::CopyText => "insert-text-symbolic",
            };
            let subtitle = match target {
                ActionTarget::OpenUrl => tagged_subtitle(&value, &entry.tags),
                ActionTarget::CopyText => tagged_subtitle(&clipboard_preview(&value), &entry.tags),
            };
            Some(
                Action::new(category, &entry.name, kind, score + 80)
                    .with_subtitle(subtitle)
                    .with_icon(icon_name),
            )
        })
        .collect()
}

fn search_commands(
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

fn parse_json_actions(input: &str, category: &str, base_score: i32) -> Vec<Action> {
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

#[derive(Debug, Clone)]
struct CommandMatch {
    score: i32,
    argument: String,
    args: HashMap<String, String>,
    missing: Vec<String>,
    direct: bool,
}

fn match_command_entry(entry: &CommandEntry, query: &str) -> Option<CommandMatch> {
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

fn command_preferences(
    entry: &CommandEntry,
    global_preferences: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut preferences = entry.preferences.clone();
    preferences.extend(global_preferences.clone());
    preferences
}

fn command_env(entry: &CommandEntry, context: &PlaceholderContext) -> HashMap<String, String> {
    entry
        .env
        .iter()
        .map(|(key, value)| (key.clone(), expand_placeholders(value, context)))
        .collect()
}

#[derive(Debug, Clone)]
struct ArgumentBinding {
    values: HashMap<String, String>,
    missing: Vec<String>,
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

fn search_clipboard(entries: &[String], query: &str, explicit: bool) -> Vec<Action> {
    if query.is_empty() {
        return entries
            .iter()
            .take(6)
            .enumerate()
            .map(|(index, entry)| {
                Action::new(
                    "Clipboard",
                    clipboard_preview(entry),
                    ActionKind::Copy(entry.clone()),
                    220 - index as i32,
                )
                .with_subtitle("Copy clipboard history item")
                .with_icon("edit-paste-symbolic")
            })
            .collect();
    }

    let mut matches = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let score = fuzzy_score(entry, query)?;
            Some(
                Action::new(
                    "Clipboard",
                    clipboard_preview(entry),
                    ActionKind::Copy(entry.clone()),
                    score + if explicit { 120 } else { 35 } - index as i32,
                )
                .with_subtitle("Copy clipboard history item")
                .with_icon("edit-paste-symbolic"),
            )
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    matches.truncate(if explicit { MAX_RESULTS } else { 3 });
    matches
}

fn search_files(files: &[FileEntry], query: &str, explicit: bool) -> Vec<Action> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<Action> = files
        .iter()
        .filter_map(|file| {
            let score = fuzzy_score(&file.name, query)?;
            let category = if file.is_dir { "Folder" } else { "File" };
            let subtitle = file
                .path
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default();
            let icon_name = if file.is_dir {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            };
            Some(
                Action::new(
                    category,
                    &file.name,
                    ActionKind::OpenPath(file.path.clone()),
                    score + if explicit { 90 } else { 15 },
                )
                .with_subtitle(subtitle)
                .with_icon(icon_name),
            )
        })
        .collect();

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    matches.truncate(if explicit { MAX_RESULTS } else { 4 });
    matches
}

fn search_system_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("system ") || lower.starts_with("sys ");
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, value)| value.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    system_actions()
        .into_iter()
        .filter(|entry| explicit || !entry.hazardous)
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                explicit.then_some(0)?
            } else {
                fuzzy_score(&haystack, needle)?
            };
            Some(
                Action::new(
                    "System",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + if explicit { 260 } else { 30 },
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn system_actions() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Lock Screen",
            subtitle: "Lock the current login session",
            command: "loginctl lock-session",
            icon_name: "system-lock-screen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Suspend",
            subtitle: "Suspend the machine",
            command: "systemctl suspend",
            icon_name: "weather-clear-night-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Open Settings",
            subtitle: "Open the desktop settings application",
            command: "gnome-control-center || systemsettings || xfce4-settings-manager",
            icon_name: "emblem-system-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Restart",
            subtitle: "Reboot the machine",
            command: "systemctl reboot",
            icon_name: "view-refresh-symbolic",
            hazardous: true,
        },
        SystemActionEntry {
            title: "Power Off",
            subtitle: "Power off the machine",
            command: "systemctl poweroff",
            icon_name: "system-shutdown-symbolic",
            hazardous: true,
        },
    ]
}

fn search_audio_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("audio ")
        || lower.starts_with("vol ")
        || lower.starts_with("volume ")
        || lower == "audio"
        || lower == "vol"
        || lower == "volume";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, v)| v.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    audio_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                explicit.then_some(0)?
            } else {
                fuzzy_score(&haystack, needle)?
            };
            Some(
                Action::new(
                    "Audio",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + if explicit { 260 } else { 30 },
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn audio_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Volume Up",
            subtitle: "Increase output volume by 5%",
            command: "wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+",
            icon_name: "audio-volume-high-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Volume Down",
            subtitle: "Decrease output volume by 5%",
            command: "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-",
            icon_name: "audio-volume-low-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Mute",
            subtitle: "Toggle output mute",
            command: "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle",
            icon_name: "audio-volume-muted-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Mic Mute",
            subtitle: "Toggle microphone mute",
            command: "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle",
            icon_name: "microphone-sensitivity-muted-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Brightness Up",
            subtitle: "Increase screen brightness by 10%",
            command: "brightnessctl set 10%+",
            icon_name: "display-brightness-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Brightness Down",
            subtitle: "Decrease screen brightness by 10%",
            command: "brightnessctl set 10%-",
            icon_name: "display-brightness-symbolic",
            hazardous: false,
        },
    ]
}

fn search_network_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("net ")
        || lower.starts_with("wifi ")
        || lower.starts_with("network ")
        || lower == "net"
        || lower == "wifi"
        || lower == "network";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, v)| v.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    network_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                explicit.then_some(0)?
            } else {
                fuzzy_score(&haystack, needle)?
            };
            Some(
                Action::new(
                    "Network",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + if explicit { 260 } else { 30 },
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn network_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Toggle WiFi",
            subtitle: "Enable or disable wireless networking",
            command: "nmcli radio wifi toggle",
            icon_name: "network-wireless-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Network Settings",
            subtitle: "Open network connection editor",
            command: "nm-connection-editor",
            icon_name: "preferences-system-network-symbolic",
            hazardous: false,
        },
    ]
}

fn search_niri_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let matches_prefix = lower == "niri" || lower.starts_with("niri ");
    if !matches_prefix {
        return Vec::new();
    }

    let needle = lower
        .strip_prefix("niri")
        .unwrap_or_default()
        .trim()
        .to_string();

    niri_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, &needle)
            }?;
            Some(
                Action::new(
                    "Niri",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + 260,
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn niri_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Screenshot",
            subtitle: "Interactive screenshot with region selection",
            command: "niri msg action screenshot",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Screenshot Screen",
            subtitle: "Capture the entire screen",
            command: "niri msg action screenshot-screen",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Screenshot Window",
            subtitle: "Capture the focused window",
            command: "niri msg action screenshot-window",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Fullscreen Window",
            subtitle: "Toggle fullscreen for the focused window",
            command: "niri msg action fullscreen-window",
            icon_name: "view-fullscreen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Close Window",
            subtitle: "Close the focused window",
            command: "niri msg action close-window",
            icon_name: "window-close-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Next Workspace",
            subtitle: "Focus workspace below",
            command: "niri msg action focus-workspace-down",
            icon_name: "go-down-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Previous Workspace",
            subtitle: "Focus workspace above",
            command: "niri msg action focus-workspace-up",
            icon_name: "go-up-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move Window to Next Workspace",
            subtitle: "Move focused window to workspace below",
            command: "niri msg action move-window-to-workspace-down",
            icon_name: "go-down-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move Window to Previous Workspace",
            subtitle: "Move focused window to workspace above",
            command: "niri msg action move-window-to-workspace-up",
            icon_name: "go-up-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Power Off Monitors",
            subtitle: "Turn off all monitors",
            command: "niri msg action power-off-monitors",
            icon_name: "system-shutdown-symbolic",
            hazardous: false,
        },
    ]
}

fn search_processes(query: &str) -> Vec<Action> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    search_process_entries(&load_process_entries(), query)
}

fn search_process_entries(processes: &[ProcessEntry], query: &str) -> Vec<Action> {
    let mut actions = processes
        .iter()
        .filter_map(|process| {
            let haystack = format!("{} {}", process.name, process.command);
            let score = fuzzy_score(&haystack, query)?;
            Some(process_action(process, score + 240))
        })
        .collect::<Vec<_>>();

    actions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    actions.truncate(MAX_RESULTS);
    actions
}

fn process_action(process: &ProcessEntry, score: i32) -> Action {
    Action::new(
        "Process",
        format!("Kill {} ({})", process.name, process.pid),
        ActionKind::Shell(ShellCommand::new(format!("kill {}", process.pid))),
        score,
    )
    .with_subtitle(process_subtitle(process))
    .with_icon("application-x-executable-symbolic")
}

fn process_subtitle(process: &ProcessEntry) -> String {
    if process.command.trim().is_empty() {
        format!("PID {}", process.pid)
    } else {
        format!(
            "PID {} - {}",
            process.pid,
            clipboard_preview(&process.command)
        )
    }
}

fn load_process_entries() -> Vec<ProcessEntry> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
            load_process_entry(pid, &entry.path())
        })
        .collect()
}

fn load_process_entry(pid: u32, path: &Path) -> Option<ProcessEntry> {
    let name = fs::read_to_string(path.join("comm"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let command = fs::read(path.join("cmdline"))
        .ok()
        .map(|value| decode_cmdline(&value))
        .unwrap_or_default();

    Some(ProcessEntry { pid, name, command })
}

fn decode_cmdline(value: &[u8]) -> String {
    value
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn load_file_index(home: &Path) -> Vec<FileEntry> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut roots = Vec::new();

    if let Ok(cwd) = env::current_dir() {
        if let Some(parent) = cwd.parent() {
            roots.push(parent.to_path_buf());
        }
        roots.push(cwd);
    }

    for name in ["Code", "Documents", "Downloads", "Desktop", "Projects"] {
        roots.push(home.join(name));
    }
    roots.push(home.to_path_buf());

    for root in roots {
        visit_files(&root, 0, &mut files, &mut seen);
        if files.len() >= MAX_INDEXED_FILES {
            break;
        }
    }

    files
}

fn visit_files(dir: &Path, depth: usize, files: &mut Vec<FileEntry>, seen: &mut HashSet<PathBuf>) {
    if depth > MAX_FILE_DEPTH || files.len() >= MAX_INDEXED_FILES {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if files.len() >= MAX_INDEXED_FILES {
            return;
        }

        let path = entry.path();
        if !seen.insert(path.clone()) {
            continue;
        }

        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if should_skip_file(&name) {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let is_dir = file_type.is_dir();
        files.push(FileEntry {
            name: name.to_string(),
            path: path.clone(),
            is_dir,
        });

        if is_dir {
            visit_files(&path, depth + 1, files, seen);
        }
    }
}

fn should_skip_file(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }

    matches!(
        name,
        "target"
            | "node_modules"
            | ".git"
            | ".cache"
            | ".cargo"
            | ".rustup"
            | ".local"
            | ".npm"
            | ".var"
            | "Trash"
    )
}

fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }

    let text_lower = text.to_lowercase();
    if text_lower == query {
        return Some(500);
    }
    if text_lower.starts_with(&query) {
        return Some(400 - text.len() as i32);
    }
    if text_lower.contains(&query) {
        return Some(300 - text_lower.find(&query).unwrap_or(0) as i32);
    }

    let mut score = 0;
    let mut last_index = None;
    let mut chars = text_lower.char_indices();

    for wanted in query.chars() {
        let mut found = None;
        for (index, actual) in chars.by_ref() {
            if actual == wanted {
                found = Some(index);
                break;
            }
        }
        let index = found?;
        score += match last_index {
            Some(last) if index == last + 1 => 20,
            Some(last) => 10 - (index.saturating_sub(last) as i32).min(10),
            None => 20 - index as i32,
        };
        last_index = Some(index);
    }

    Some(score)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn spawn_shell(command: &ShellCommand) {
    match Command::new("sh")
        .arg("-c")
        .arg(&command.command)
        .envs(&command.env)
        .spawn()
    {
        Ok(_) => println!("started: {}", command.command),
        Err(error) => eprintln!("failed to start '{}': {error}", command.command),
    }
}

fn spawn_command(program: &str, args: &[&str]) {
    match Command::new(program).args(args).spawn() {
        Ok(_) => println!("started: {program} {}", args.join(" ")),
        Err(error) => eprintln!("failed to start {program}: {error}"),
    }
}

fn copy_to_clipboard(text: &str) {
    let copied =
        copy_with("wl-copy", &[], text) || copy_with("xclip", &["-selection", "clipboard"], text);

    if copied {
        println!("copied to clipboard");
    } else {
        println!("{text}");
        eprintln!("install wl-clipboard or xclip to copy automatically");
    }
}

fn copy_with(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
    else {
        return false;
    };

    let wrote = child
        .stdin
        .as_mut()
        .and_then(|stdin| stdin.write_all(text.as_bytes()).ok())
        .is_some();
    wrote && child.wait().map(|status| status.success()).unwrap_or(false)
}

fn execute_http_request(request: &HttpRequest) -> Option<String> {
    match request {
        HttpRequest::Translate {
            endpoint,
            text,
            target,
            api_key,
        } => {
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(5))
                .build();
            let body = serde_json::json!({
                "q": text,
                "source": "auto",
                "target": target,
                "api_key": api_key,
                "format": "text"
            });
            let resp = agent
                .post(&format!("{endpoint}/translate"))
                .set("Content-Type", "application/json")
                .send_json(body)
                .ok()?;
            let value: serde_json::Value = resp.into_json().ok()?;
            value
                .get("translatedText")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
        HttpRequest::AiChat {
            endpoint,
            model,
            query,
            api_key,
        } => {
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build();
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": query}],
                "max_tokens": 500
            });
            let resp = agent
                .post(&format!("{endpoint}/chat/completions"))
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {api_key}"))
                .send_json(body)
                .ok()?;
            let value: serde_json::Value = resp.into_json().ok()?;
            value
                .get("choices")
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("message"))
                .and_then(|v| v.get("content"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
    }
}

fn search_ai(query: &str, preferences: &HashMap<String, String>) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    if !lower.starts_with("ai ") {
        return Vec::new();
    }
    let text = query.trim()[3..].trim();
    if text.is_empty() {
        return Vec::new();
    }
    let endpoint = preferences
        .get("ai_endpoint")
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
    let model = preferences
        .get("ai_model")
        .cloned()
        .unwrap_or_else(|| "gemma4:e4b".to_string());
    let api_key = preferences
        .get("ai_api_key")
        .cloned()
        .unwrap_or_default();
    let preview = if text.chars().count() > 40 {
        format!("{}...", text.chars().take(37).collect::<String>())
    } else {
        text.to_string()
    };
    vec![
        Action::new(
            "AI",
            format!("Ask AI: {preview}"),
            ActionKind::HttpCopy(HttpRequest::AiChat {
                endpoint,
                model: model.clone(),
                query: text.to_string(),
                api_key,
            }),
            980,
        )
        .with_subtitle(format!("{model} \u{2014} response copied to clipboard"))
        .with_icon("dialog-question-symbolic"),
    ]
}

fn search_translate(query: &str, preferences: &HashMap<String, String>) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let text_part = if lower.starts_with("translate ") {
        query.trim()[10..].trim()
    } else if lower.starts_with("trans ") {
        query.trim()[6..].trim()
    } else {
        return Vec::new();
    };

    if text_part.is_empty() {
        return Vec::new();
    }

    let default_target = preferences
        .get("translate_target")
        .cloned()
        .unwrap_or_else(|| "en".to_string());

    let (text, target) = if let Some(pos) = text_part.rfind(" in ") {
        let suffix = &text_part[pos + 4..];
        let lang_len = suffix.chars().count();
        if lang_len >= 2 && lang_len <= 3 && suffix.chars().all(|c| c.is_ascii_alphabetic()) {
            (&text_part[..pos], suffix.to_string())
        } else {
            (text_part, default_target)
        }
    } else {
        (text_part, default_target)
    };

    if text.trim().is_empty() {
        return Vec::new();
    }

    let endpoint = preferences
        .get("translate_endpoint")
        .cloned()
        .unwrap_or_else(|| "https://libretranslate.com".to_string());
    let api_key = preferences
        .get("translate_api_key")
        .cloned()
        .unwrap_or_default();

    vec![
        Action::new(
            "Translate",
            format!("Translate: {text}"),
            ActionKind::HttpCopy(HttpRequest::Translate {
                endpoint,
                text: text.to_string(),
                target: target.clone(),
                api_key,
            }),
            980,
        )
        .with_subtitle(format!("to {target} \u{2014} result copied to clipboard"))
        .with_icon("preferences-desktop-locale-symbolic"),
    ]
}

fn looks_like_expression(value: &str) -> bool {
    let has_operator = value.chars().any(|ch| matches!(ch, '+' | '-' | '*' | '/'));
    let only_expr_chars = value.chars().all(|ch| {
        ch.is_ascii_digit()
            || ch.is_ascii_whitespace()
            || matches!(ch, '.' | '+' | '-' | '*' | '/' | '(' | ')')
    });
    has_operator && only_expr_chars
}

fn format_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.8}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

struct Calculator<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Calculator<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<f64, String> {
        let value = self.expression()?;
        self.skip_ws();
        if self.pos == self.input.len() {
            Ok(value)
        } else {
            Err("unexpected trailing characters".to_string())
        }
    }

    fn expression(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    value += self.term()?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    value -= self.term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    value *= self.factor()?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn factor(&mut self) -> Result<f64, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                Ok(-self.factor()?)
            }
            Some(b'(') => {
                self.pos += 1;
                let value = self.expression()?;
                self.skip_ws();
                if self.peek() != Some(b')') {
                    return Err("missing ')'".to_string());
                }
                self.pos += 1;
                Ok(value)
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9') | Some(b'.')) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err("expected number".to_string());
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "invalid number".to_string())
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn calculator_handles_precedence_and_parentheses() {
        let result = Calculator::new("(12 + 8) / 5").parse().unwrap();
        assert_eq!(result, 4.0);
    }

    #[test]
    fn calculator_rejects_division_by_zero() {
        assert!(Calculator::new("4 / 0").parse().is_err());
    }

    #[test]
    fn fuzzy_score_matches_subsequence() {
        assert!(fuzzy_score("Visual Studio Code", "vsc").is_some());
    }

    #[test]
    fn desktop_exec_placeholders_are_removed() {
        assert_eq!(
            clean_desktop_exec("firefox %u --new-window %F"),
            "firefox --new-window"
        );
    }

    #[test]
    fn expression_detection_is_conservative() {
        assert!(looks_like_expression("1 + 2 * 3"));
        assert!(!looks_like_expression("firefox"));
    }

    #[test]
    fn aliases_are_normalized_like_raycast() {
        assert_eq!(normalize_alias("  FF!!  "), "ff");
        assert_eq!(normalize_alias("Go   Compose"), "go compose");
    }

    #[test]
    fn file_search_uses_index_without_explicit_prefix() {
        let files = vec![FileEntry {
            name: "project-notes.md".to_string(),
            path: PathBuf::from("/tmp/project-notes.md"),
            is_dir: false,
        }];

        let results = search_files(&files, "notes", false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "File");
    }

    #[test]
    fn clipboard_lines_roundtrip_multiline_text() {
        let text = "hello\nworld\t\\\\";
        let encoded = encode_clipboard_line(text);
        assert_eq!(decode_clipboard_line(&encoded).as_deref(), Some(text));
    }

    #[test]
    fn clipboard_search_returns_copy_actions() {
        let results = search_clipboard(&["alpha beta".to_string()], "beta", true);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "Clipboard");
        assert_eq!(results[0].value(), "alpha beta");
    }

    #[test]
    fn placeholders_expand_query_clipboard_and_calc() {
        let context = PlaceholderContext {
            query: "rust gtk".to_string(),
            clipboard: "token".to_string(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        assert_eq!(
            expand_placeholders("q={{query}} c={{clipboard}} n={{calc:2 + 3}}", &context),
            "q=rust gtk c=token n=5"
        );
    }

    #[test]
    fn placeholders_expand_local_date_time() {
        let context = PlaceholderContext {
            query: String::new(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        assert_eq!(
            expand_placeholders("{{date}}", &context),
            format_local_time(UNIX_EPOCH, "%Y-%m-%d")
        );
        assert_eq!(
            expand_placeholders("{{time:%H:%M}}", &context),
            format_local_time(UNIX_EPOCH, "%H:%M")
        );
        assert_eq!(
            expand_placeholders("{{datetime:%Y}}", &context),
            format_local_time(UNIX_EPOCH, "%Y")
        );
    }

    #[test]
    fn unknown_placeholders_are_preserved() {
        let context = PlaceholderContext {
            query: String::new(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        assert_eq!(expand_placeholders("{{unknown}}", &context), "{{unknown}}");
    }

    #[test]
    fn placeholders_expand_preferences() {
        let context = PlaceholderContext {
            query: String::new(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::from([("workspace".to_string(), "zeshicast".to_string())]),
            now: UNIX_EPOCH,
        };

        assert_eq!(
            expand_placeholders("cd ~/Code/{{pref:workspace}}", &context),
            "cd ~/Code/zeshicast"
        );
    }

    #[test]
    fn secondary_actions_include_pin_for_unpinned_action() {
        let app = Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            clipboard_history: Vec::new(),
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            files: Vec::new(),
            config_dir: PathBuf::from("/tmp/zeshicast-test"),
        };
        let action = Action::new(
            "App",
            "Firefox",
            ActionKind::Launch("firefox".to_string()),
            1,
        );
        let actions = app.available_secondary_actions(&action);
        assert!(
            actions
                .iter()
                .any(|action| action.kind == SecondaryActionKind::Pin)
        );
    }

    #[test]
    fn clipboard_secondary_actions_include_delete_and_clear() {
        let app = Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            clipboard_history: vec!["secret".to_string()],
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            files: Vec::new(),
            config_dir: PathBuf::from("/tmp/zeshicast-test"),
        };
        let action = Action::new(
            "Clipboard",
            "secret",
            ActionKind::Copy("secret".to_string()),
            1,
        );
        let actions = app.available_secondary_actions(&action);
        assert!(
            actions
                .iter()
                .any(|action| action.kind == SecondaryActionKind::DeleteClipboardItem)
        );
        assert!(
            actions
                .iter()
                .any(|action| action.kind == SecondaryActionKind::ClearClipboardHistory)
        );
    }

    #[test]
    fn named_values_parse_tags() {
        let item = parse_named_value("Deploy | work, devops = kubectl apply").unwrap();
        assert_eq!(item.name, "Deploy");
        assert_eq!(item.tags, vec!["work", "devops"]);
        assert_eq!(item.value, "kubectl apply");
    }

    #[test]
    fn named_values_search_by_tag() {
        let entries = vec![parse_named_value("Deploy | work, devops = kubectl apply").unwrap()];
        let context = PlaceholderContext {
            query: "devops".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };
        let results = search_named_values(
            "Snippet",
            &entries,
            "devops",
            ActionTarget::CopyText,
            &context,
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].subtitle.contains("[work, devops]"));
    }

    #[test]
    fn command_entries_parse_toml() {
        let command = parse_command_entry(
            r#"
name = "Open Notes"
mode = "json"
command = "xdg-open ~/Notes"
category = "Workspace"
description = "Open notes folder"
icon = "folder-symbolic"
keyword = "notes"
argument_hint = "<path>"
tags = ["notes", "work"]
arguments = [
  { name = "path", type = "path", required = true }
]
[env]
NOTES_ROOT = "{{pref:notes_root}}"
"#,
        )
        .unwrap();

        assert_eq!(command.name, "Open Notes");
        assert_eq!(command.mode, CommandMode::Json);
        assert_eq!(command.command, "xdg-open ~/Notes");
        assert_eq!(command.category, "Workspace");
        assert_eq!(command.icon_name, "folder-symbolic");
        assert_eq!(command.keyword.as_deref(), Some("notes"));
        assert_eq!(command.argument_hint, "<path>");
        assert_eq!(command.arguments.len(), 1);
        assert_eq!(command.arguments[0].name, "path");
        assert_eq!(command.arguments[0].kind, CommandArgumentKind::Path);
        assert!(command.arguments[0].required);
        assert_eq!(
            command.env.get("NOTES_ROOT").map(String::as_str),
            Some("{{pref:notes_root}}")
        );
        assert_eq!(command.tags, vec!["notes", "work"]);
    }

    #[test]
    fn command_search_matches_tags_and_expands_placeholders() {
        let entries = vec![
            parse_command_entry(
                r#"
name = "Search Man"
command = "man -k '{{query}}'"
description = "Search manual pages"
keyword = "man"
argument_hint = "<term>"
tags = ["docs", "terminal"]
"#,
            )
            .unwrap(),
        ];
        let context = PlaceholderContext {
            query: "printf".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        let results = search_commands(&entries, "docs", &context);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "Command");
        assert_eq!(results[0].value(), "man -k 'docs'");
        assert!(results[0].subtitle.contains("[docs, terminal]"));
    }

    #[test]
    fn command_keyword_uses_trailing_query_as_argument() {
        let entries = vec![
            parse_command_entry(
                r#"
name = "GitHub Search"
command = "xdg-open 'https://github.com/search?q={{query}}'"
keyword = "gh"
argument_hint = "<query>"
arguments = [
  { name = "query", type = "text", required = true }
]
"#,
            )
            .unwrap(),
        ];
        let context = PlaceholderContext {
            query: "gh rust gtk".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        let results = search_commands(&entries, "gh rust gtk", &context);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].value(),
            "xdg-open 'https://github.com/search?q=rust gtk'"
        );
        assert!(results[0].subtitle.contains("gh <query>"));
    }

    #[test]
    fn command_arguments_expand_named_placeholders() {
        let entries = vec![
            parse_command_entry(
                r#"
name = "Deploy"
command = "deploy --env {{arg:env}} --service '{{arg:service}}' --force {{arg:force}}"
keyword = "deploy"
argument_hint = "<env> <service>"
arguments = [
  { name = "env", type = "enum", required = true, options = ["dev", "prod"] },
  { name = "service", type = "text", required = true },
  { name = "force", type = "bool", default = "false" }
]
"#,
            )
            .unwrap(),
        ];
        let context = PlaceholderContext {
            query: "deploy prod api worker".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        let results = search_commands(&entries, "deploy prod api worker", &context);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].value(),
            "deploy --env prod --service 'api worker' --force false"
        );
    }

    #[test]
    fn command_arguments_disable_action_when_required_value_missing() {
        let entries = vec![
            parse_command_entry(
                r#"
name = "Deploy"
command = "deploy --env {{arg:env}}"
keyword = "deploy"
arguments = [
  { name = "env", type = "enum", required = true, options = ["dev", "prod"] }
]
"#,
            )
            .unwrap(),
        ];
        let context = PlaceholderContext {
            query: "deploy".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: UNIX_EPOCH,
        };

        let results = search_commands(&entries, "deploy", &context);
        assert_eq!(results.len(), 1);
        assert!(results[0].form_data().is_some());
        assert!(results[0].subtitle.contains("Missing argument: env"));
    }

    #[test]
    fn command_preferences_use_defaults_and_global_overrides() {
        let entries = vec![
            parse_command_entry(
                r#"
name = "Open Workspace"
command = "xdg-open '{{pref:workspace}}/{{arg:project}}'"
keyword = "ws"
arguments = [
  { name = "project", type = "text", required = true }
]

[preferences]
workspace = "~/Code"
"#,
            )
            .unwrap(),
        ];
        let context = PlaceholderContext {
            query: "ws zeshicast".to_string(),
            clipboard: String::new(),
            args: HashMap::new(),
            preferences: HashMap::from([("workspace".to_string(), "/src".to_string())]),
            now: UNIX_EPOCH,
        };

        let results = search_commands(&entries, "ws zeshicast", &context);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value(), "xdg-open '/src/zeshicast'");
    }

    #[test]
    fn command_env_expands_arguments_and_preferences() {
        let entry = parse_command_entry(
            r#"
name = "Deploy"
command = "deploy"
keyword = "deploy"
arguments = [
  { name = "env", type = "enum", required = true, options = ["dev", "prod"] }
]

[preferences]
token = "default-token"

[env]
DEPLOY_ENV = "{{arg:env}}"
DEPLOY_TOKEN = "{{pref:token}}"
"#,
        )
        .unwrap();
        let command_match = match_command_entry(&entry, "deploy prod").unwrap();
        let context = PlaceholderContext {
            query: command_match.argument,
            clipboard: String::new(),
            args: command_match.args,
            preferences: command_preferences(
                &entry,
                &HashMap::from([("token".to_string(), "user-token".to_string())]),
            ),
            now: UNIX_EPOCH,
        };

        let env = command_env(&entry, &context);
        assert_eq!(env.get("DEPLOY_ENV").map(String::as_str), Some("prod"));
        assert_eq!(
            env.get("DEPLOY_TOKEN").map(String::as_str),
            Some("user-token")
        );
    }

    #[test]
    fn json_actions_parse_result_arrays() {
        let actions = parse_json_actions(
            r#"
[
  {
    "title": "Rust",
    "subtitle": "Open rust-lang.org",
    "icon": "emblem-web-symbolic",
    "action": { "type": "open_url", "value": "https://www.rust-lang.org" }
  },
  {
    "title": "Copy crate",
    "action": { "type": "copy", "value": "gtk4" }
  }
]
"#,
            "Extension",
            900,
        );

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].title, "Rust");
        assert_eq!(actions[0].value(), "https://www.rust-lang.org");
        assert_eq!(actions[1].value(), "gtk4");
    }

    #[test]
    fn json_actions_parse_results_object() {
        let actions = parse_json_actions(
            r#"
{
  "results": [
    { "title": "Open tmp", "action": { "type": "open_path", "value": "/tmp" } }
  ]
}
"#,
            "Extension",
            900,
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].value(), "/tmp");
    }

    #[test]
    fn system_actions_hide_hazardous_entries_without_explicit_prefix() {
        let regular = search_system_actions("power");
        assert!(regular.iter().all(|action| action.title != "Power Off"));

        let explicit = search_system_actions("system power");
        assert!(explicit.iter().any(|action| action.title == "Power Off"));
    }

    #[test]
    fn audio_actions_show_on_vol_prefix() {
        let results = search_audio_actions("vol");
        assert!(!results.is_empty());
        assert!(results.iter().all(|a| a.category == "Audio"));
    }

    #[test]
    fn audio_actions_fuzzy_match_without_prefix() {
        let results = search_audio_actions("mute");
        assert!(!results.is_empty());
        assert!(results.iter().any(|a| a.title.contains("Mute")));
    }

    #[test]
    fn network_actions_show_on_wifi_prefix() {
        let results = search_network_actions("wifi");
        assert!(!results.is_empty());
        assert!(results.iter().all(|a| a.category == "Network"));
    }

    #[test]
    fn niri_actions_only_on_explicit_prefix() {
        assert!(search_niri_actions("screenshot").is_empty());
        assert!(search_niri_actions("workspace").is_empty());
        let with_prefix = search_niri_actions("niri screenshot");
        assert!(!with_prefix.is_empty());
        assert!(with_prefix.iter().all(|a| a.category == "Niri"));
    }

    #[test]
    fn niri_actions_show_all_on_bare_niri() {
        let results = search_niri_actions("niri");
        assert!(!results.is_empty());
    }

    #[test]
    fn process_search_builds_kill_actions() {
        let processes = vec![ProcessEntry {
            pid: 4242,
            name: "zeshicast".to_string(),
            command: "target/debug/zeshicast-gtk --daemon".to_string(),
        }];

        let actions = search_process_entries(&processes, "zesh");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].category, "Process");
        assert_eq!(actions[0].value(), "kill 4242");
        assert!(actions[0].subtitle.contains("target/debug/zeshicast-gtk"));
    }

    #[test]
    fn cmdline_decodes_nul_separated_arguments() {
        assert_eq!(
            decode_cmdline(b"zeshicast-gtk\0--daemon\0"),
            "zeshicast-gtk --daemon"
        );
    }
}
