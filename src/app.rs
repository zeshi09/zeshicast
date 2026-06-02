use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::{
    Action, ActionKind, ActionTarget, AppEntry, AppsProvider, AudioProvider, ClipboardProvider,
    CommandEntry, CommandsProvider, FileEntry, FilesProvider, HyprlandProvider, LauncherCommand,
    MAX_CLIPBOARD_ENTRIES, MAX_RESULTS, MediaProvider, NamedValue, NamedValuesProvider,
    NetworkProvider, NiriProvider, NotificationsProvider, PlaceholderContext, ProcessesProvider,
    ScriptEntry, ScriptsProvider, SearchContext, SearchProvider, SecondaryAction,
    SecondaryActionKind, ShellCommand, SwayProvider, SystemProvider, WebProvider, WindowsProvider,
    app_action, append_alias, expand_placeholders, fuzzy_score, home_dir, load_aliases, load_apps,
    load_clipboard_history, load_clipboard_timestamps, load_command_entries, load_file_index,
    load_frequencies, load_lines, load_named_values, load_preferences, load_script_entries,
    normalize_alias, search_audio_actions,
    search_media_actions, search_network_actions, search_notification_actions,
    search_system_actions, spawn_shell, unix_now, write_clipboard_history,
    write_clipboard_timestamps, write_frequencies, write_lines, write_preferences,
};

#[derive(Debug, Clone)]
pub struct Zeshicast {
    pub(crate) apps: Vec<AppEntry>,
    pub(crate) quicklinks: Vec<NamedValue>,
    pub(crate) snippets: Vec<NamedValue>,
    pub(crate) commands: Vec<CommandEntry>,
    pub(crate) scripts: Vec<ScriptEntry>,
    pub(crate) clipboard_history: Vec<String>,
    pub(crate) clipboard_timestamps: HashMap<String, i64>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) aliases: HashMap<String, String>,
    pub(crate) pins: HashSet<String>,
    pub(crate) recent: Vec<String>,
    pub(crate) frequencies: HashMap<String, u32>,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) config_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Url,
    Command,
    Code,
}

impl ClipboardKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Url => "URL",
            Self::Command => "Command",
            Self::Code => "Code",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Text => "insert-text-symbolic",
            Self::Url => "emblem-shared-symbolic",
            Self::Command => "utilities-terminal-symbolic",
            Self::Code => "applications-engineering-symbolic",
        }
    }

    pub fn mime_hint(self) -> &'static str {
        match self {
            Self::Text => "text/plain;charset=utf-8",
            Self::Url => "text/uri-list",
            Self::Command | Self::Code => "text/plain;charset=utf-8",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardSummary {
    pub preview: String,
    pub value: String,
    pub kind: ClipboardKind,
    pub size_bytes: usize,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SnippetSummary {
    pub name: String,
    pub preview: String,
    pub value: String,
    pub tags: Vec<String>,
}

pub fn classify_clipboard_text(text: &str) -> ClipboardKind {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    let first_word = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('$');

    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("file://")
    {
        return ClipboardKind::Url;
    }

    let is_single_line = !trimmed.contains('\n');
    let known_commands = [
        "cargo",
        "cd",
        "codex",
        "docker",
        "git",
        "grep",
        "ls",
        "make",
        "nix",
        "npm",
        "pnpm",
        "rg",
        "sudo",
        "systemctl",
        "vim",
        "yarn",
    ];
    if is_single_line
        && (trimmed.starts_with("$ ") || known_commands.contains(&first_word))
        && trimmed.split_whitespace().count() > 1
    {
        return ClipboardKind::Command;
    }

    let code_markers = [
        "fn ",
        "let ",
        "use ",
        "pub ",
        "impl ",
        "match ",
        "struct ",
        "enum ",
        "mod ",
        "import ",
        "export ",
        "const ",
        "class ",
        "function ",
        "#[",
        "//",
        "/*",
        "*/",
        "&mut",
        "=>",
        "::",
        "</",
        "{",
        "};",
    ];
    if trimmed.lines().count() > 1 || code_markers.iter().any(|marker| lower.contains(marker)) {
        return ClipboardKind::Code;
    }

    ClipboardKind::Text
}

impl Zeshicast {
    pub fn load() -> Self {
        let home = home_dir();
        let config_dir = home.join(".config/zeshicast");
        let preferences = load_preferences(&config_dir.join("preferences.toml"));
        let clipboard_history = load_clipboard_history(&config_dir.join("clipboard.txt"));
        let clipboard_timestamps =
            load_clipboard_timestamps(&config_dir.join("clipboard_timestamps.json"));
        let script_dirs = preference_script_dirs(&preferences, &config_dir);
        Self {
            apps: load_apps(&home),
            quicklinks: load_named_values(&config_dir.join("quicklinks.txt")),
            snippets: load_named_values(&config_dir.join("snippets.txt")),
            commands: load_command_entries(&config_dir.join("commands")),
            scripts: load_script_entries(&script_dirs),
            clipboard_history,
            clipboard_timestamps,
            preferences,
            aliases: load_aliases(&config_dir.join("aliases.txt")),
            pins: load_lines(&config_dir.join("pins.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            recent: load_lines(&config_dir.join("recent.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            frequencies: load_frequencies(&config_dir.join("frequencies.txt")),
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

        actions.extend(self.launcher_actions(trimmed));

        if lower.starts_with("calc ") || crate::looks_like_expression(trimmed) {
            let expr = trimmed.strip_prefix("calc ").unwrap_or(trimmed).trim();
            match crate::Calculator::new(expr).parse() {
                Ok(value) => actions.push(
                    Action::new(
                        "Calculator",
                        format!("{expr} = {}", crate::format_number(value)),
                        ActionKind::Copy(crate::format_number(value)),
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

        let search_context = SearchContext {
            query: trimmed,
            placeholders: &context,
        };
        actions.extend(AppsProvider { apps: &self.apps }.search(&search_context));
        actions.extend(
            NamedValuesProvider {
                category: "Quicklink",
                entries: &self.quicklinks,
                target: ActionTarget::OpenUrl,
            }
            .search(&search_context),
        );
        actions.extend(
            NamedValuesProvider {
                category: "Snippet",
                entries: &self.snippets,
                target: ActionTarget::CopyText,
            }
            .search(&search_context),
        );
        actions.extend(
            CommandsProvider {
                commands: &self.commands,
            }
            .search(&search_context),
        );
        actions.extend(SystemProvider.search(&search_context));
        actions.extend(AudioProvider.search(&search_context));
        if self.preference_enabled("network_enabled", true) {
            actions.extend(NetworkProvider.search(&search_context));
        }
        if self.preference_enabled("media_enabled", true) {
            actions.extend(MediaProvider.search(&search_context));
        }
        if self.preference_enabled("notifications_enabled", true) {
            actions.extend(NotificationsProvider.search(&search_context));
        }
        actions.extend(NiriProvider.search(&search_context));
        actions.extend(HyprlandProvider.search(&search_context));
        actions.extend(SwayProvider.search(&search_context));
        actions.extend(WindowsProvider.search(&search_context));
        if self.preference_enabled("ai_enabled", true) {
            actions.extend(WebProvider.search(&search_context));
        }
        actions.extend(ScriptsProvider { entries: &self.scripts }.search(&search_context));
        actions.extend(
            ClipboardProvider {
                entries: &self.clipboard_history,
            }
            .search(&search_context),
        );
        actions.extend(FilesProvider { files: &self.files }.search(&search_context));
        actions.extend(ProcessesProvider.search(&search_context));

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
        use crate::ActionPanelSection as S;
        let mut actions = vec![
            SecondaryAction::new(
                SecondaryActionKind::Run,
                "Run",
                "media-playback-start-symbolic",
                S::Primary,
            ),
            SecondaryAction::new(
                SecondaryActionKind::CopyValue,
                "Copy Value",
                "edit-copy-symbolic",
                S::Primary,
            ),
        ];

        if action.parent_dir().is_some() {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::OpenParent,
                "Open Containing Folder",
                "folder-open-symbolic",
                S::Primary,
            ));
        }

        if action.category == "Clipboard" {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::DeleteClipboardItem,
                "Delete Clipboard Item",
                "edit-delete-symbolic",
                S::Clipboard,
            ));
            actions.push(SecondaryAction::new(
                SecondaryActionKind::ClearClipboardHistory,
                "Clear Clipboard History",
                "edit-clear-symbolic",
                S::Danger,
            ));
        }

        if self.is_pinned(action) {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Unpin,
                "Unpin",
                "view-pin-symbolic",
                S::Manage,
            ));
        } else {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Pin,
                "Pin",
                "view-pin-symbolic",
                S::Manage,
            ));
        }

        actions
    }

    pub fn is_recent(&self, action: &Action) -> bool {
        let identity = action.identity().to_lowercase();
        self.recent.iter().any(|entry| entry == &identity)
    }

    pub fn recent_top_identities(&self, count: usize) -> Vec<String> {
        self.recent
            .iter()
            .filter(|id| !self.pins.contains(*id))
            .take(count)
            .cloned()
            .collect()
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
        let text = crate::normalize_clipboard_text(text);
        if text.is_empty() {
            return Ok(false);
        }

        self.clipboard_timestamps.entry(text.clone()).or_insert_with(unix_now);
        self.clipboard_history.retain(|entry| entry != &text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(MAX_CLIPBOARD_ENTRIES);
        self.write_clipboard()?;
        Ok(true)
    }

    pub fn delete_clipboard_item(&mut self, action: &Action) -> io::Result<()> {
        let value = action.value();
        self.delete_clipboard_value(&value)
    }

    pub fn delete_clipboard_value(&mut self, value: &str) -> io::Result<()> {
        self.clipboard_history.retain(|entry| entry != value);
        self.clipboard_timestamps.remove(value);
        self.write_clipboard()
    }

    pub fn clear_clipboard_history(&mut self) -> io::Result<()> {
        self.clipboard_history.clear();
        self.clipboard_timestamps.clear();
        self.write_clipboard()
    }

    fn write_clipboard(&self) -> io::Result<()> {
        write_clipboard_history(
            &self.config_dir.join("clipboard.txt"),
            &self.clipboard_history,
        )?;
        write_clipboard_timestamps(
            &self.config_dir.join("clipboard_timestamps.json"),
            &self.clipboard_history,
            &self.clipboard_timestamps,
        )
    }

    pub fn list_clipboard_history(&self) -> Vec<ClipboardSummary> {
        self.clipboard_history
            .iter()
            .map(|entry| ClipboardSummary {
                preview: crate::clipboard_preview(entry),
                value: entry.clone(),
                kind: classify_clipboard_text(entry),
                size_bytes: entry.len(),
                timestamp: self.clipboard_timestamps.get(entry).copied(),
            })
            .collect()
    }

    pub fn list_snippets(&self) -> Vec<SnippetSummary> {
        self.snippets
            .iter()
            .map(|snippet| SnippetSummary {
                name: snippet.name.clone(),
                preview: crate::clipboard_preview(&snippet.value),
                value: snippet.value.clone(),
                tags: snippet.tags.clone(),
            })
            .collect()
    }

    pub fn delete_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        self.snippets
            .retain(|snippet| snippet.name != name || snippet.value != value);
        self.write_snippets()
    }

    pub fn add_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snippet name and value are required",
            ));
        }
        self.snippets.push(NamedValue {
            name: name.to_string(),
            value: value.to_string(),
            tags: vec!["ai".to_string()],
        });
        self.write_snippets()
    }

    fn write_snippets(&self) -> io::Result<()> {
        let lines = self
            .snippets
            .iter()
            .map(|snippet| {
                if snippet.tags.is_empty() {
                    format!("{} = {}", snippet.name, snippet.value)
                } else {
                    format!(
                        "{} | {} = {}",
                        snippet.name,
                        snippet.tags.join(", "),
                        snippet.value
                    )
                }
            })
            .collect::<Vec<_>>();
        write_lines(&self.config_dir.join("snippets.txt"), &lines)
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
        let search_context = SearchContext {
            query: "",
            placeholders: context,
        };

        actions.extend(self.apps.iter().map(|app| app_action(app, 20)));
        actions.extend(self.launcher_actions(""));
        actions.extend(search_system_actions("system"));
        actions.extend(search_audio_actions("audio"));
        if self.preference_enabled("network_enabled", true) {
            actions.extend(search_network_actions("network"));
        }
        if self.preference_enabled("media_enabled", true) {
            actions.extend(search_media_actions("media"));
        }
        if self.preference_enabled("notifications_enabled", true) {
            actions.extend(search_notification_actions("notify"));
        }
        actions.extend(
            NamedValuesProvider {
                category: "Quicklink",
                entries: &self.quicklinks,
                target: ActionTarget::OpenUrl,
            }
            .search(&search_context),
        );
        actions.extend(
            NamedValuesProvider {
                category: "Snippet",
                entries: &self.snippets,
                target: ActionTarget::CopyText,
            }
            .search(&search_context),
        );
        actions.extend(
            CommandsProvider {
                commands: &self.commands,
            }
            .search(&search_context),
        );

        actions
    }

    fn launcher_actions(&self, query: &str) -> Vec<Action> {
        let candidates = [
            (
                "AI Chat",
                "Ask a local Ollama-compatible model",
                "system-search-symbolic",
                "ai chat assistant ollama local model",
                LauncherCommand::AiChat,
                "ai_enabled",
            ),
            (
                "Audio Mixer",
                "Open output, input and application volumes",
                "audio-volume-high-symbolic",
                "audio mixer volume output input microphone applications pipewire wpctl",
                LauncherCommand::Audio,
                "audio_enabled",
            ),
            (
                "Dashboard",
                "Open system dashboard",
                "view-dashboard-symbolic",
                "dashboard system monitor status control center",
                LauncherCommand::Dashboard,
                "dashboard_enabled",
            ),
            (
                "System Monitor",
                "Open detailed system status",
                "utilities-system-monitor-symbolic",
                "system monitor processes cpu memory disk load status",
                LauncherCommand::SystemMonitor,
                "dashboard_enabled",
            ),
            (
                "Media",
                "Open media status",
                "media-playback-start-symbolic",
                "media music player mpris playback",
                LauncherCommand::Media,
                "media_enabled",
            ),
            (
                "Network",
                "Open network status",
                "network-wireless-symbolic",
                "network wifi ethernet internet ip",
                LauncherCommand::Network,
                "network_enabled",
            ),
            (
                "Notifications",
                "Open notification status",
                "preferences-system-notifications-symbolic",
                "notifications notification center dnd history alerts",
                LauncherCommand::Notifications,
                "notifications_enabled",
            ),
        ];

        candidates
            .iter()
            .filter_map(|(title, subtitle, icon, text, command, preference)| {
                if !self.preference_enabled(preference, true) {
                    return None;
                }
                let score = if query.trim().is_empty() {
                    90
                } else {
                    fuzzy_score(text, query)? + 120
                };
                Some(
                    Action::new("Zeshicast", *title, ActionKind::Launcher(*command), score)
                        .with_subtitle(*subtitle)
                        .with_icon(*icon),
                )
            })
            .collect()
    }

    fn preference_enabled(&self, key: &str, default_value: bool) -> bool {
        self.preferences
            .get(key)
            .and_then(|value| parse_bool_preference(value))
            .unwrap_or(default_value)
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
                permissions: e.permissions.clone(),
            })
            .collect()
    }

    fn record_recent(&mut self, action: &Action) -> io::Result<()> {
        let identity = action.identity().to_lowercase();
        self.recent.retain(|entry| entry != &identity);
        self.recent.insert(0, identity.clone());
        self.recent.truncate(50);
        write_lines(&self.config_dir.join("recent.txt"), &self.recent)?;
        *self.frequencies.entry(identity).or_insert(0) += 1;
        write_frequencies(&self.config_dir.join("frequencies.txt"), &self.frequencies)
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

        if let Some(&count) = self.frequencies.get(&identity_lower) {
            let freq_score = ((count as f32).ln() * 15.0).min(100.0) as i32;
            score += if query.is_empty() {
                freq_score
            } else {
                freq_score / 2
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

fn parse_bool_preference(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn preference_script_dirs(
    preferences: &HashMap<String, String>,
    config_dir: &std::path::Path,
) -> Vec<std::path::PathBuf> {
    let custom = preferences.get("script_dirs").map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
    });
    custom.unwrap_or_else(|| vec![config_dir.join("scripts")])
}

#[derive(Debug, Clone)]
pub struct CommandSummary {
    pub name: String,
    pub category: String,
    pub description: String,
    pub keyword: Option<String>,
    pub icon_name: String,
    pub tags: Vec<String>,
    pub permissions: Vec<String>,
}
