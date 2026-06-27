use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::services::storage;
use crate::{
    Action, ActionKind, ActionTarget, AppEntry, AppsProvider, AudioProvider, ClipboardProvider,
    CommandEntry, CommandsProvider, EmojiProvider, ExecutionDecision, ExecutionPolicy, FileEntry,
    FilesProvider, HyprlandProvider, LauncherCommand, MAX_CLIPBOARD_ENTRIES, MAX_RESULTS,
    MediaProvider, NamedValue, NamedValuesProvider, NetworkProvider, NiriProvider,
    NotificationsProvider, PlaceholderContext, ProcessesProvider, ScriptEntry, ScriptsProvider,
    SearchContext, SearchProvider, SecondaryAction, SecondaryActionKind, ShellCommand,
    SwayProvider, SystemProvider, WebProvider, WindowsProvider, app_action, append_alias,
    expand_placeholders, expand_placeholders_shell, fuzzy_score, home_dir, load_aliases, load_apps,
    load_clipboard_history, load_command_entries, load_extension_command_entries,
    load_extension_manifests, load_extension_script_entries, load_file_index, load_frequencies,
    load_lines, load_named_values, load_preferences, load_script_entries, normalize_alias,
    search_audio_actions, search_media_actions, search_network_actions,
    search_notification_actions, search_system_actions, spawn_shell, write_lines,
    write_preferences,
};

#[derive(Debug, Clone)]
pub struct CalcHistoryEntry {
    pub expr: String,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct Zeshicast {
    pub(crate) apps: Vec<AppEntry>,
    pub(crate) quicklinks: Vec<NamedValue>,
    pub(crate) snippets: Vec<NamedValue>,
    pub(crate) commands: Vec<CommandEntry>,
    pub(crate) scripts: Vec<ScriptEntry>,
    pub(crate) clipboard_history: Vec<String>,
    pub(crate) clipboard_timestamps: HashMap<String, i64>,
    pub(crate) calc_history: Vec<CalcHistoryEntry>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) aliases: HashMap<String, String>,
    pub(crate) pins: HashSet<String>,
    pub(crate) recent: Vec<String>,
    pub(crate) frequencies: HashMap<String, u32>,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) config_dir: PathBuf,
}

/// Sentinel prefix marking a clipboard history entry as an image. The rest of
/// the stored value is the path to the cached PNG. The leading SOH control char
/// keeps it from colliding with any real copied text.
pub const CLIPBOARD_IMAGE_PREFIX: &str = "\u{1}zeshicast-image:";

/// If `value` is an image entry, return the cached PNG path.
pub fn clipboard_image_path(value: &str) -> Option<&str> {
    value.strip_prefix(CLIPBOARD_IMAGE_PREFIX)
}

pub fn clipboard_cache_dir() -> PathBuf {
    home_dir().join(".cache/zeshicast/clipboard")
}

fn preference_enabled_value(
    preferences: &HashMap<String, String>,
    key: &str,
    default_value: bool,
) -> bool {
    preferences
        .get(key)
        .and_then(|value| parse_bool_preference(value))
        .unwrap_or(default_value)
}

fn clipboard_retention_value(preferences: &HashMap<String, String>) -> usize {
    preferences
        .get("clipboard_retention")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(MAX_CLIPBOARD_ENTRIES)
        .clamp(1, 1000)
}

fn referenced_clipboard_image_paths(entries: &[String]) -> HashSet<PathBuf> {
    entries
        .iter()
        .filter_map(|entry| clipboard_image_path(entry).map(PathBuf::from))
        .collect()
}

fn is_png_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

pub(crate) fn prune_clipboard_image_cache_dir(
    cache_dir: &Path,
    entries: &[String],
) -> io::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    let referenced = referenced_clipboard_image_paths(entries);
    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if is_png_file(&path) && !referenced.contains(&path) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn prune_clipboard_image_cache(entries: &[String]) -> io::Result<()> {
    prune_clipboard_image_cache_dir(&clipboard_cache_dir(), entries)
}

pub(crate) fn clear_clipboard_image_cache_dir(cache_dir: &Path) -> io::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if is_png_file(&path) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn clear_clipboard_image_cache() -> io::Result<()> {
    clear_clipboard_image_cache_dir(&clipboard_cache_dir())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Url,
    Command,
    Code,
    Image,
}

impl ClipboardKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Url => "URL",
            Self::Command => "Command",
            Self::Code => "Code",
            Self::Image => "Image",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Text => "insert-text-symbolic",
            Self::Url => "emblem-shared-symbolic",
            Self::Command => "utilities-terminal-symbolic",
            Self::Code => "applications-engineering-symbolic",
            Self::Image => "image-x-generic-symbolic",
        }
    }

    pub fn mime_hint(self) -> &'static str {
        match self {
            Self::Text => "text/plain;charset=utf-8",
            Self::Url => "text/uri-list",
            Self::Command | Self::Code => "text/plain;charset=utf-8",
            Self::Image => "image/png",
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
    if text.starts_with(CLIPBOARD_IMAGE_PREFIX) {
        return ClipboardKind::Image;
    }
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
        Self::load_inner(true)
    }

    /// Like `load`, but skips the (potentially slow) filesystem index so the GUI
    /// can present its window without blocking on a `$HOME` walk. Pair with
    /// [`build_file_index`](Self::build_file_index) +
    /// [`set_file_index`](Self::set_file_index) to fill it in on a worker thread.
    #[cfg(feature = "gui")]
    pub(crate) fn load_deferred_files() -> Self {
        Self::load_inner(false)
    }

    /// Build the filesystem index. Safe to call off the main thread.
    #[cfg(feature = "gui")]
    pub(crate) fn build_file_index() -> Vec<FileEntry> {
        load_file_index(&home_dir())
    }

    /// Install a filesystem index built elsewhere (see `build_file_index`).
    #[cfg(feature = "gui")]
    pub(crate) fn set_file_index(&mut self, files: Vec<FileEntry>) {
        self.files = files;
    }

    fn load_inner(index_files: bool) -> Self {
        let home = home_dir();
        let config_dir = home.join(".config/zeshicast");
        let preferences = load_preferences(&config_dir.join("preferences.toml"));
        let script_dirs = preference_script_dirs(&preferences, &config_dir);
        let extensions = load_extension_manifests(&config_dir);

        // Migrate text-file clipboard to SQLite on first run
        if !storage::clipboard_has_data(&config_dir) {
            let legacy = load_clipboard_history(&config_dir.join("clipboard.txt"));
            if !legacy.is_empty() {
                storage::migrate_clipboard(&config_dir, &legacy).ok();
            }
        }

        // Migrate text-file usage history to SQLite on first run
        if !storage::usage_has_data(&config_dir) {
            let recent_legacy = load_lines(&config_dir.join("recent.txt"))
                .into_iter()
                .map(|l| l.to_lowercase())
                .collect::<Vec<_>>();
            let freq_legacy = load_frequencies(&config_dir.join("frequencies.txt"));
            if !recent_legacy.is_empty() {
                storage::migrate_usage(&config_dir, &recent_legacy, &freq_legacy).ok();
            }
        }

        let clipboard_retention = clipboard_retention_value(&preferences);
        let clip_rows = storage::clipboard_load_with_limit(&config_dir, clipboard_retention);
        let clipboard_history: Vec<String> = clip_rows.iter().map(|(t, _)| t.clone()).collect();
        let clipboard_timestamps: HashMap<String, i64> = clip_rows.into_iter().collect();
        let _ = prune_clipboard_image_cache(&clipboard_history);

        let recent = storage::usage_recent(&config_dir, 50);
        let frequencies = storage::usage_frequencies(&config_dir);

        let calc_history = load_calc_history(&config_dir.join("calc_history.json"));
        let mut commands = load_command_entries(&config_dir.join("commands"));
        commands.extend(load_extension_command_entries(&extensions));
        let mut scripts = load_script_entries(&script_dirs);
        scripts.extend(load_extension_script_entries(&extensions));

        Self {
            apps: load_apps(&home),
            quicklinks: load_named_values(&config_dir.join("quicklinks.txt")),
            snippets: load_named_values(&config_dir.join("snippets.txt")),
            commands,
            scripts,
            clipboard_history,
            clipboard_timestamps,
            calc_history,
            preferences,
            aliases: load_aliases(&config_dir.join("aliases.txt")),
            pins: load_lines(&config_dir.join("pins.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            recent,
            frequencies,
            files: if index_files {
                load_file_index(&home)
            } else {
                Vec::new()
            },
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
        if self.notifications_history_enabled() {
            actions.extend(NotificationsProvider.search(&search_context));
        }
        actions.extend(NiriProvider.search(&search_context));
        actions.extend(HyprlandProvider.search(&search_context));
        actions.extend(SwayProvider.search(&search_context));
        actions.extend(WindowsProvider.search(&search_context));
        if self.preference_enabled("ai_enabled", true) {
            actions.extend(WebProvider.search(&search_context));
        }
        actions.extend(
            ScriptsProvider {
                entries: &self.scripts,
            }
            .search(&search_context),
        );
        actions.extend(EmojiProvider.search(&search_context));
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
                    .with_icon("utilities-terminal-symbolic")
                    .with_risk(crate::ActionRisk::Shell),
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

    pub fn run_action(&mut self, action: &Action) -> ExecutionDecision {
        self.run_action_with_policy(action, ExecutionPolicy::interactive())
    }

    pub fn run_action_confirmed(&mut self, action: &Action) -> ExecutionDecision {
        self.run_action_with_policy(action, ExecutionPolicy::confirmed())
    }

    fn run_action_with_policy(
        &mut self,
        action: &Action,
        policy: ExecutionPolicy,
    ) -> ExecutionDecision {
        let decision = action.run_with_policy(policy);
        if !matches!(decision, ExecutionDecision::RunNow) {
            return decision;
        }
        if action.category == "Calculator"
            && let Some((expr, result)) = action.title.split_once(" = ")
        {
            self.record_calc(expr.trim(), result.trim());
        }
        if let Err(error) = self.record_recent(action) {
            eprintln!("failed to record recent action: {error}");
        }
        decision
    }

    pub fn available_secondary_actions(&self, action: &Action) -> Vec<SecondaryAction> {
        use crate::ActionPanelSection as S;
        let can_type = is_wtype_available();
        let is_text_action = matches!(action.category.as_str(), "Snippet" | "Clipboard");
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

        if can_type && is_text_action {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::TypeText,
                "Expand (type text)",
                "input-keyboard-symbolic",
                S::Primary,
            ));
        }

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

    pub fn record_calc(&mut self, expr: &str, result: &str) {
        self.calc_history.retain(|e| e.expr != expr);
        self.calc_history.insert(
            0,
            CalcHistoryEntry {
                expr: expr.to_string(),
                result: result.to_string(),
            },
        );
        self.calc_history.truncate(20);
        save_calc_history(
            &self.config_dir.join("calc_history.json"),
            &self.calc_history,
        );
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
            SecondaryActionKind::Run => {
                self.run_action(action);
            }
            SecondaryActionKind::CopyValue => action.copy_value(),
            SecondaryActionKind::TypeText => type_text_via_wtype(&action.value()),
            SecondaryActionKind::OpenParent => action.open_parent_dir(),
            SecondaryActionKind::Pin => self.pin_action(action)?,
            SecondaryActionKind::Unpin => self.unpin_action(action)?,
            SecondaryActionKind::DeleteClipboardItem => self.delete_clipboard_item(action)?,
            SecondaryActionKind::ClearClipboardHistory => self.clear_clipboard_history()?,
        }
        Ok(())
    }

    pub fn add_clipboard_text(&mut self, text: &str) -> io::Result<bool> {
        if !self.clipboard_history_enabled() || self.clipboard_private_mode() {
            return Ok(false);
        }
        let text = crate::normalize_clipboard_text(text);
        if text.is_empty() {
            return Ok(false);
        }
        let retention = self.clipboard_retention();
        storage::clipboard_insert_with_limit(&self.config_dir, &text, retention)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_timestamps
            .entry(text.clone())
            .or_insert_with(crate::unix_now);
        self.clipboard_history.retain(|e| e != &text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(retention);
        prune_clipboard_image_cache(&self.clipboard_history)?;
        Ok(true)
    }

    pub fn delete_clipboard_item(&mut self, action: &Action) -> io::Result<()> {
        self.delete_clipboard_value(&action.value())
    }

    pub fn delete_clipboard_value(&mut self, value: &str) -> io::Result<()> {
        storage::clipboard_delete(&self.config_dir, value)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.retain(|e| e != value);
        self.clipboard_timestamps.remove(value);
        prune_clipboard_image_cache(&self.clipboard_history)?;
        Ok(())
    }

    pub fn clear_clipboard_history(&mut self) -> io::Result<()> {
        storage::clipboard_clear(&self.config_dir).map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.clear();
        self.clipboard_timestamps.clear();
        clear_clipboard_image_cache()?;
        Ok(())
    }

    pub fn list_clipboard_history(&self) -> Vec<ClipboardSummary> {
        self.clipboard_history
            .iter()
            .map(|entry| {
                let kind = classify_clipboard_text(entry);
                let preview = if kind == ClipboardKind::Image {
                    "Image".to_string()
                } else {
                    crate::clipboard_preview(entry)
                };
                ClipboardSummary {
                    preview,
                    value: entry.clone(),
                    kind,
                    size_bytes: entry.len(),
                    timestamp: self.clipboard_timestamps.get(entry).copied(),
                }
            })
            .collect()
    }

    /// Record a captured image (already cached as a PNG at `path`).
    pub fn add_clipboard_image(&mut self, path: &str) -> io::Result<bool> {
        if !self.clipboard_capture_images() {
            return Ok(false);
        }
        self.add_clipboard_text(&format!("{CLIPBOARD_IMAGE_PREFIX}{path}"))
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
        if self.notifications_history_enabled() {
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

        for (i, entry) in self.calc_history.iter().enumerate() {
            actions.push(
                Action::new(
                    "Calculator",
                    format!("{} = {}", entry.expr, entry.result),
                    ActionKind::Copy(entry.result.clone()),
                    15 - i as i32,
                )
                .with_subtitle("Recent calculation · copy result")
                .with_icon("accessories-calculator-symbolic"),
            );
        }

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
            (
                "Emoji Picker",
                "Browse and copy emoji by category",
                "face-smile-symbolic",
                "emoji picker emoticons symbols faces",
                LauncherCommand::Emoji,
                "dashboard_enabled",
            ),
            (
                "Font Browser",
                "Browse installed system fonts with live preview",
                "applications-fonts-symbolic",
                "fonts typography typeface preview system",
                LauncherCommand::Fonts,
                "dashboard_enabled",
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
        preference_enabled_value(&self.preferences, key, default_value)
    }

    pub fn clipboard_history_enabled(&self) -> bool {
        self.preference_enabled("clipboard_history_enabled", true)
    }

    pub fn clipboard_private_mode(&self) -> bool {
        self.preference_enabled("clipboard_private_mode", false)
    }

    pub fn clipboard_capture_images(&self) -> bool {
        self.preference_enabled("clipboard_capture_images", true)
            && self.clipboard_history_enabled()
            && !self.clipboard_private_mode()
    }

    pub fn notifications_history_enabled(&self) -> bool {
        self.preference_enabled("notifications_enabled", true)
            && self.preference_enabled("notifications_history_enabled", true)
    }

    fn clipboard_retention(&self) -> usize {
        clipboard_retention_value(&self.preferences)
    }

    fn enforce_clipboard_retention(&mut self) -> io::Result<()> {
        let retention = self.clipboard_retention();
        storage::clipboard_prune(&self.config_dir, retention)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.clipboard_history.truncate(retention);
        prune_clipboard_image_cache(&self.clipboard_history)
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
        let command = expand_placeholders_shell(&form.command, &context);
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
        let should_prune_clipboard = key == "clipboard_retention";
        if value.is_empty() {
            self.preferences.remove(&key);
        } else {
            self.preferences.insert(key, value);
        }
        write_preferences(&self.config_dir.join("preferences.toml"), &self.preferences)?;
        if should_prune_clipboard {
            self.enforce_clipboard_retention()?;
        }
        Ok(())
    }

    pub fn list_commands(&self) -> Vec<CommandSummary> {
        let mut summaries = self
            .commands
            .iter()
            .map(|e| CommandSummary {
                kind: "Command".to_string(),
                name: e.name.clone(),
                category: e.category.clone(),
                description: e.description.clone(),
                keyword: e.keyword.clone(),
                icon_name: e.icon_name.clone(),
                tags: e.tags.clone(),
                permissions: e.permissions.clone(),
                capabilities: e
                    .capabilities
                    .iter()
                    .map(|capability| capability.label().to_string())
                    .collect(),
                extension: e.origin.as_ref().map(ExtensionSummary::from_origin),
                enabled: true,
            })
            .collect::<Vec<_>>();

        summaries.extend(self.scripts.iter().map(|script| {
            CommandSummary {
                kind: "Script".to_string(),
                name: script.title.clone(),
                category: "Script".to_string(),
                description: script.description.clone(),
                keyword: None,
                icon_name: script.icon.clone(),
                tags: Vec::new(),
                permissions: script
                    .origin
                    .as_ref()
                    .map(|origin| origin.capabilities.clone())
                    .unwrap_or_default(),
                capabilities: script
                    .origin
                    .as_ref()
                    .map(|origin| origin.capabilities.clone())
                    .unwrap_or_default(),
                extension: script.origin.as_ref().map(ExtensionSummary::from_origin),
                enabled: true,
            }
        }));

        summaries.sort_by(|a, b| {
            a.extension_group()
                .cmp(&b.extension_group())
                .then(a.kind.cmp(&b.kind))
                .then(a.name.cmp(&b.name))
        });
        summaries
    }

    fn record_recent(&mut self, action: &Action) -> io::Result<()> {
        let identity = action.identity().to_lowercase();
        storage::usage_record(&self.config_dir, &identity)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.recent.retain(|e| e != &identity);
        self.recent.insert(0, identity.clone());
        self.recent.truncate(50);
        *self.frequencies.entry(identity).or_insert(0) += 1;
        Ok(())
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

fn load_calc_history(path: &std::path::Path) -> Vec<CalcHistoryEntry> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(array) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let Some(items) = array.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let expr = item.get("e")?.as_str()?.to_string();
            let result = item.get("r")?.as_str()?.to_string();
            Some(CalcHistoryEntry { expr, result })
        })
        .collect()
}

fn save_calc_history(path: &std::path::Path, history: &[CalcHistoryEntry]) {
    let array: Vec<serde_json::Value> = history
        .iter()
        .map(|e| serde_json::json!({"e": e.expr, "r": e.result}))
        .collect();
    if let Ok(json) = serde_json::to_string(&array) {
        crate::write_file_atomic(path, json.as_bytes(), 0o600).ok();
    }
}

fn is_wtype_available() -> bool {
    std::process::Command::new("which")
        .arg("wtype")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn type_text_via_wtype(text: &str) {
    std::thread::spawn({
        let text = text.to_string();
        move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::Command::new("wtype").arg(&text).spawn().ok();
        }
    });
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
    pub kind: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub keyword: Option<String>,
    pub icon_name: String,
    pub tags: Vec<String>,
    pub permissions: Vec<String>,
    pub capabilities: Vec<String>,
    pub extension: Option<ExtensionSummary>,
    pub enabled: bool,
}

impl CommandSummary {
    pub fn extension_group(&self) -> String {
        self.extension
            .as_ref()
            .map(|extension| extension.name.clone())
            .unwrap_or_else(|| "Built-in".to_string())
    }

    pub fn extension_detail(&self) -> String {
        self.extension
            .as_ref()
            .map(|extension| format!("{}@{}", extension.id, extension.version))
            .unwrap_or_else(|| "legacy command directories".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

impl ExtensionSummary {
    pub(crate) fn from_origin(origin: &crate::ExtensionOrigin) -> Self {
        Self {
            id: origin.id.clone(),
            name: origin.name.clone(),
            version: origin.version.clone(),
            capabilities: origin.capabilities.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_cache_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zeshicast-{name}-{}-{nanos}", std::process::id()))
    }

    fn image_entry(path: &Path) -> String {
        format!("{CLIPBOARD_IMAGE_PREFIX}{}", path.display())
    }

    fn test_app(config_dir: PathBuf, preferences: HashMap<String, String>) -> Zeshicast {
        Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            clipboard_history: Vec::new(),
            clipboard_timestamps: HashMap::new(),
            calc_history: Vec::new(),
            preferences,
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            files: Vec::new(),
            config_dir,
        }
    }

    #[test]
    fn clipboard_clear_removes_image_cache_files() {
        let dir = test_cache_dir("clipboard-clear");
        fs::create_dir_all(&dir).unwrap();
        let image = dir.join("image.png");
        let metadata = dir.join("metadata.txt");
        fs::write(&image, b"png").unwrap();
        fs::write(&metadata, b"keep").unwrap();

        clear_clipboard_image_cache_dir(&dir).unwrap();

        assert!(!image.exists());
        assert!(metadata.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clipboard_prune_removes_orphan_images() {
        let dir = test_cache_dir("clipboard-prune");
        fs::create_dir_all(&dir).unwrap();
        let kept = dir.join("kept.png");
        let orphan = dir.join("orphan.png");
        fs::write(&kept, b"png").unwrap();
        fs::write(&orphan, b"png").unwrap();
        let entries = vec![image_entry(&kept)];

        prune_clipboard_image_cache_dir(&dir, &entries).unwrap();

        assert!(kept.exists());
        assert!(!orphan.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clipboard_delete_keeps_still_referenced_image() {
        let dir = test_cache_dir("clipboard-delete");
        fs::create_dir_all(&dir).unwrap();
        let image = dir.join("shared.png");
        fs::write(&image, b"png").unwrap();
        let entries = vec![image_entry(&image), "plain text".to_string()];

        prune_clipboard_image_cache_dir(&dir, &entries).unwrap();

        assert!(image.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clipboard_disabled_does_not_record_text() {
        let config_dir = test_cache_dir("clipboard-disabled-config");
        let mut app = test_app(
            config_dir.clone(),
            HashMap::from([("clipboard_history_enabled".to_string(), "false".to_string())]),
        );

        assert!(!app.add_clipboard_text("secret").unwrap());
        assert!(app.clipboard_history.is_empty());
        assert!(!config_dir.join("zeshicast.db").exists());
    }

    #[test]
    fn private_mode_does_not_record_clipboard() {
        let config_dir = test_cache_dir("clipboard-private-config");
        let mut app = test_app(
            config_dir.clone(),
            HashMap::from([("clipboard_private_mode".to_string(), "true".to_string())]),
        );

        assert!(!app.add_clipboard_text("secret").unwrap());
        assert!(app.clipboard_history.is_empty());
        assert!(!config_dir.join("zeshicast.db").exists());
    }
}
