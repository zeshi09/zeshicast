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

use crate::app::preference_enabled_value;
use crate::{
    Action, ActionKind, ActionTarget, AppEntry, AppsProvider, AudioProvider, BrowserTabsProvider,
    CalcHistoryEntry, ClipboardProvider, CommandEntry, CommandsProvider, EmojiProvider,
    ExtensionManifest, ExtensionsProvider, FileEntry, FilesProvider, HyprlandProvider,
    LauncherCommand, MAX_RESULTS, MediaProvider, NamedValue, NamedValuesProvider, NetworkProvider,
    NiriProvider, NotificationsProvider, PlaceholderContext, ProcessesProvider, ScriptEntry,
    ScriptsProvider, SearchContext, SearchProvider, ShellCommand, SwayProvider, SystemProvider,
    WebProvider, WindowsProvider, app_action, fuzzy_score, normalize_alias, search_audio_actions,
    search_media_actions, search_network_actions, search_notification_actions,
    search_system_actions,
};

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
    pub files: Vec<FileEntry>,
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
        let mut actions = Vec::new();
        let trimmed = query.trim();
        let lower = trimmed.to_lowercase();
        let context = PlaceholderContext::new(trimmed, self.clipboard_history.first())
            .with_preferences(&self.preferences);

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
        providers.push(Box::new(FilesProvider { files: &self.files }));
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
