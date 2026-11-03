//! The search inputs, detached from the running application (M-1).
//!
//! `Zeshicast` lives in an `Rc<RefCell<..>>` shared with the widgets, so it can
//! never cross a thread boundary. Search only *reads* its inputs, so
//! [`SearchData`] takes a snapshot of exactly those (plain `Vec`/`HashMap`
//! clones) and owns the whole search path. That lets the palette run a search on
//! a worker thread and keep the main loop responsive while providers fork
//! processes.
//!
//! Kept in sync with the fields of `Zeshicast` by construction: `search_data()`
//! builds it, and a missing field would not compile.

use std::collections::{HashMap, HashSet};

use crate::action::{Action, ActionKind, LauncherCommand, ShellCommand};
use crate::app::CalcHistoryEntry;
use crate::app::preference_enabled_value;
use crate::config::normalize_alias;
use crate::extensions::ExtensionManifest;
use crate::placeholders::PlaceholderContext;
use crate::search::apps::{AppEntry, app_action};
use crate::search::commands::CommandEntry;
use crate::search::files;
use crate::search::files::FileEntry;
use crate::search::layout;
use crate::search::media::search_media_actions;
use crate::search::named_values::{ActionTarget, NamedValue};
use crate::search::notifications::search_notification_actions;
use crate::search::scripts::ScriptEntry;
use crate::search::system::{search_audio_actions, search_network_actions, search_system_actions};
use crate::search::{
    AppsProvider, AudioProvider, BluetoothProvider, BrowserTabsProvider, ClipboardProvider,
    CommandsProvider, EmojiProvider, ExtensionsProvider, FilesProvider, HyprlandProvider,
    MAX_RESULTS, MediaProvider, NamedValuesProvider, NetworkProvider, NiriProvider,
    NotificationsProvider, ProcessesProvider, ScriptsProvider, SearchContext, SearchProvider,
    SwayProvider, SystemProvider, WebProvider, WindowsProvider, fuzzy_score,
};

/// Score handicap for results found by reading a Cyrillic query as QWERTY.
const WRONG_LAYOUT_PENALTY: i32 = 50;

/// Everything `search` reads from the application.
#[derive(Debug, Clone)]
pub(crate) struct SearchData {
    pub apps: Vec<AppEntry>,
    pub quicklinks: Vec<NamedValue>,
    pub snippets: Vec<NamedValue>,
    pub commands: Vec<CommandEntry>,
    pub scripts: Vec<ScriptEntry>,
    pub clipboard_history: Vec<String>,
    pub preferences: HashMap<String, String>,
    pub aliases: HashMap<String, String>,
    pub pins: HashSet<String>,
    pub recent: Vec<String>,
    pub frequencies: HashMap<String, u32>,
    pub extensions: Vec<ExtensionManifest>,
    pub files: std::sync::Arc<Vec<FileEntry>>,
    pub calc_history: Vec<CalcHistoryEntry>,
}

impl SearchData {
    fn preference_enabled(&self, key: &str, default_value: bool) -> bool {
        preference_enabled_value(&self.preferences, key, default_value)
    }

    fn notifications_history_enabled(&self) -> bool {
        self.preference_enabled("notifications_enabled", true)
            && self.preference_enabled("notifications_history_enabled", true)
    }

    pub fn search(&self, query: &str) -> Vec<Action> {
        let results = self.search_as_typed(query);
        let Some(qwerty) = layout::qwerty_from_jcuken(query.trim()) else {
            return results;
        };

        // Q1.1: a Cyrillic query may have been typed with the wrong layout
        // active (`ашкуащч` → `firefox`). "Nothing found as typed" is not a
        // usable signal: long Russian clipboard texts almost always contain
        // a weak subsequence match. So both readings compete; the remapped
        // one pays a penalty and wins only with a clearly better match.
        let mut merged = results;
        let mut seen: HashSet<String> = merged.iter().map(Action::identity).collect();
        for mut action in self.search_as_typed(&qwerty) {
            if seen.insert(action.identity()) {
                action.score -= WRONG_LAYOUT_PENALTY;
                merged.push(action);
            }
        }
        merged.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        });
        merged.truncate(MAX_RESULTS);
        merged
    }

    fn search_as_typed(&self, query: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        let trimmed = query.trim();
        let lower = trimmed.to_lowercase();
        let context = PlaceholderContext::new(trimmed, self.clipboard_history.first())
            .with_preferences(&self.preferences);

        if trimmed.is_empty() {
            actions.extend(self.default_actions(&context));
            return self.rank(actions, trimmed);
        }

        // Q0.3: `file `/`find ` asks for files only. Without this the
        // clipboard, apps and every other fuzzy provider answered too, and could
        // push the files the user asked for out of the list.
        if files::is_file_query(trimmed) {
            let search_context = SearchContext {
                query: trimmed,
                placeholders: &context,
            };
            let files = FilesProvider {
                files: &self.files,
                in_root: true,
            }
            .search(&search_context);
            return self.rank(files, trimmed);
        }

        actions.extend(self.launcher_actions(trimmed));

        if lower.starts_with("calc ") || crate::search::calculator::looks_like_expression(trimmed) {
            let expr = trimmed.strip_prefix("calc ").unwrap_or(trimmed).trim();
            match crate::search::calculator::Calculator::new(expr).parse() {
                Ok(_) => actions.extend(crate::search::calculator::calc_action(expr)),
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

        let mut providers: Vec<Box<dyn SearchProvider + '_>> = vec![
            Box::new(AppsProvider { apps: &self.apps }),
            Box::new(NamedValuesProvider {
                category: "Quicklink",
                entries: &self.quicklinks,
                target: ActionTarget::OpenUrl,
            }),
            Box::new(NamedValuesProvider {
                category: "Snippet",
                entries: &self.snippets,
                target: ActionTarget::CopyText,
            }),
            Box::new(CommandsProvider {
                commands: &self.commands,
            }),
            Box::new(SystemProvider),
            Box::new(AudioProvider),
        ];

        if self.preference_enabled("network_enabled", true) {
            providers.push(Box::new(NetworkProvider));
        }
        if self.preference_enabled("bluetooth_enabled", true) {
            providers.push(Box::new(BluetoothProvider));
        }
        if self.preference_enabled("media_enabled", true) {
            providers.push(Box::new(MediaProvider));
        }
        if self.notifications_history_enabled() {
            providers.push(Box::new(NotificationsProvider));
        }

        providers.push(Box::new(NiriProvider));
        providers.push(Box::new(HyprlandProvider));
        providers.push(Box::new(SwayProvider));
        providers.push(Box::new(WindowsProvider));

        if self.preference_enabled("ai_enabled", true) {
            providers.push(Box::new(WebProvider));
        }

        providers.push(Box::new(ScriptsProvider {
            entries: &self.scripts,
        }));
        providers.push(Box::new(EmojiProvider));
        providers.push(Box::new(ClipboardProvider {
            entries: &self.clipboard_history,
        }));
        // M-6: the file index is the most expensive provider, so skip it for a
        // query another provider already owns.
        if !files::is_dedicated_query(trimmed) {
            providers.push(Box::new(FilesProvider {
                files: &self.files,
                in_root: self.preference_enabled("files_in_root", true),
            }));
        }
        providers.push(Box::new(ProcessesProvider));
        providers.push(Box::new(BrowserTabsProvider));
        providers.push(Box::new(ExtensionsProvider {
            manifests: &self.extensions,
        }));

        for provider in providers {
            actions.extend(provider.search(&search_context));
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
                    .with_icon("utilities-terminal-symbolic")
                    .with_risk(crate::action::ActionRisk::Shell),
                );
            }
        }

        self.rank(actions, trimmed)
    }

    /// Apply aliases/pins/usage scoring, sort best-first and keep the top
    /// `MAX_RESULTS`.
    fn rank(&self, mut actions: Vec<Action>, query: &str) -> Vec<Action> {
        for action in &mut actions {
            action.score += self.config_score(action, query);
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

    fn default_actions(&self, context: &PlaceholderContext<'_>) -> Vec<Action> {
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
            (
                "Window Grid",
                "Tile and snap active window across monitor",
                "view-grid-symbolic",
                "window grid tile snap split resize left right fullscreen thirds",
                LauncherCommand::WindowGrid,
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
                    Action::new(
                        "Zeshicast",
                        *title,
                        ActionKind::Launcher(command.clone()),
                        score,
                    )
                    .with_subtitle(*subtitle)
                    .with_icon(*icon),
                )
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data_matching_everywhere() -> SearchData {
        let app = crate::search::apps::parse_desktop_content(
            "[Desktop Entry]\nType=Application\nName=Invoice Maker\nExec=invoice-maker\n",
        )
        .expect("valid desktop entry");
        SearchData {
            apps: vec![app],
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            clipboard_history: vec!["invoice for march".to_string()],
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            extensions: Vec::new(),
            files: std::sync::Arc::new(vec![FileEntry::new(
                "invoice.pdf",
                PathBuf::from("/home/user/Downloads/invoice.pdf"),
                false,
            )]),
            calc_history: Vec::new(),
        }
    }

    #[test]
    fn file_prefix_returns_only_files() {
        let data = data_matching_everywhere();

        for query in ["file invoice", "find invoice", "  FILE invoice"] {
            let results = data.search(query);
            assert!(!results.is_empty(), "{query}: the file is found");
            assert!(
                results
                    .iter()
                    .all(|action| matches!(action.category.as_str(), "File" | "Folder")),
                "{query}: only files, got {:?}",
                results.iter().map(|a| &a.category).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn unprefixed_queries_still_mix_providers() {
        let data = data_matching_everywhere();

        let categories: HashSet<String> = data
            .search("invoice")
            .into_iter()
            .map(|action| action.category)
            .collect();

        for category in ["File", "Clipboard"] {
            assert!(
                categories.contains(category),
                "{category} in {categories:?}"
            );
        }
    }

    fn titles(data: &SearchData, query: &str) -> Vec<(String, String)> {
        data.search(query)
            .into_iter()
            .map(|action| (action.category, action.title))
            .collect()
    }

    #[test]
    fn an_app_ranks_above_a_file_with_the_same_name() {
        let mut data = data_matching_everywhere();
        data.files = std::sync::Arc::new(vec![FileEntry::new(
            "Invoice Maker",
            PathBuf::from("/home/user/Downloads/Invoice Maker"),
            false,
        )]);
        let results = titles(&data, "invoice maker");
        let app = results.iter().position(|(category, _)| category == "App");
        let file = results.iter().position(|(category, _)| category == "File");
        assert!(
            app.unwrap() < file.expect("the file is shown too"),
            "{results:?}"
        );
    }

    #[test]
    fn weak_file_matches_stay_out_of_root_search() {
        let mut data = data_matching_everywhere();
        data.files = std::sync::Arc::new(vec![
            FileEntry::new("invoice.pdf", PathBuf::from("/h/invoice.pdf"), false),
            // `inv` as a scattered subsequence and as an acronym only.
            FileEntry::new(
                "ink-and-vellum.txt",
                PathBuf::from("/h/ink-and-vellum.txt"),
                false,
            ),
            FileEntry::new(
                "in_new_version.md",
                PathBuf::from("/h/in_new_version.md"),
                false,
            ),
        ]);
        let root: Vec<_> = titles(&data, "inv")
            .into_iter()
            .filter(|(category, _)| category == "File")
            .map(|(_, title)| title)
            .collect();
        assert_eq!(root, vec!["invoice.pdf"]);
        // Asked for files: the weak matches are there.
        let explicit = titles(&data, "file inv");
        assert!(explicit.len() >= 2, "{explicit:?}");
    }

    #[test]
    fn files_in_root_can_be_turned_off() {
        let mut data = data_matching_everywhere();
        data.preferences
            .insert("files_in_root".to_string(), "false".to_string());
        assert!(
            !titles(&data, "invoice")
                .iter()
                .any(|(category, _)| category == "File")
        );
        assert!(
            !titles(&data, "file invoice").is_empty(),
            "file … still works"
        );
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn data(apps: &[&str], clipboard: &[&str]) -> SearchData {
        SearchData {
            apps: apps
                .iter()
                .map(|name| {
                    crate::search::apps::parse_desktop_content(&format!(
                        "[Desktop Entry]\nType=Application\nName={name}\nExec={}\n",
                        name.to_lowercase()
                    ))
                    .expect("valid desktop entry")
                })
                .collect(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            clipboard_history: clipboard.iter().map(|text| (*text).to_string()).collect(),
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            extensions: Vec::new(),
            files: Default::default(),
            calc_history: Vec::new(),
        }
    }

    #[test]
    fn a_query_typed_in_the_russian_layout_finds_the_app() {
        let results = data(&["Firefox"], &[]).search("ашкуащч");
        assert_eq!(results.first().map(|a| a.title.as_str()), Some("Firefox"));
    }

    #[test]
    fn the_wrong_layout_beats_weak_matches_in_russian_text() {
        // "еуду" is "tele" on the Russian layout. The clipboard text contains
        // е…у…д…у as a weak subsequence, which must not hide Telegram.
        let results = data(&["Telegram"], &["перед ужином думаю у окна"]).search("еуду");
        assert_eq!(results.first().map(|a| a.title.as_str()), Some("Telegram"));
    }

    #[test]
    fn a_cyrillic_query_that_matches_is_not_remapped() {
        let results = data(&["Firefox"], &["привет мир"]).search("привет");
        assert!(results.iter().any(|a| a.category == "Clipboard"));
        assert!(results.iter().all(|a| a.title != "Firefox"));
    }
}
