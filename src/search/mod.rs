pub(crate) mod apps;
pub(crate) mod browser_tabs;
pub(crate) mod calculator;
pub(crate) mod clipboard;
pub(crate) mod commands;
pub(crate) mod emoji;
pub(crate) mod extensions;
pub(crate) mod files;
pub(crate) mod media;
pub(crate) mod named_values;
pub(crate) mod notifications;
pub(crate) mod processes;
pub(crate) mod scripts;
pub(crate) mod system;
pub(crate) mod web;
pub(crate) mod windows;

use crate::{
    Action, ActionTarget, AppEntry, CommandEntry, FileEntry, NamedValue, PlaceholderContext,
    search_ai, search_apps, search_audio_actions, search_clipboard, search_commands, search_files,
    search_hyprland_actions, search_media_actions, search_named_values, search_network_actions,
    search_niri_actions, search_notification_actions, search_processes, search_sway_actions,
    search_system_actions, search_translate, search_windows,
};
use emoji::search_emoji;
use scripts::search_scripts;

pub(crate) struct SearchContext<'a> {
    pub(crate) query: &'a str,
    pub(crate) placeholders: &'a PlaceholderContext<'a>,
}

pub(crate) trait SearchProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action>;
}

pub(crate) struct AppsProvider<'a> {
    pub(crate) apps: &'a [AppEntry],
}

impl SearchProvider for AppsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_apps(self.apps, context.query)
    }
}

pub(crate) struct NamedValuesProvider<'a> {
    pub(crate) category: &'static str,
    pub(crate) entries: &'a [NamedValue],
    pub(crate) target: ActionTarget,
}

impl SearchProvider for NamedValuesProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_named_values(
            self.category,
            self.entries,
            context.query,
            self.target,
            context.placeholders,
        )
    }
}

pub(crate) struct CommandsProvider<'a> {
    pub(crate) commands: &'a [CommandEntry],
}

impl SearchProvider for CommandsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_commands(self.commands, context.query, context.placeholders)
    }
}

pub(crate) struct SystemProvider;

impl SearchProvider for SystemProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_system_actions(context.query)
    }
}

pub(crate) struct AudioProvider;

impl SearchProvider for AudioProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_audio_actions(context.query)
    }
}

pub(crate) struct NetworkProvider;

impl SearchProvider for NetworkProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_network_actions(context.query)
    }
}

pub(crate) struct MediaProvider;

impl SearchProvider for MediaProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_media_actions(context.query)
    }
}

pub(crate) struct NotificationsProvider;

impl SearchProvider for NotificationsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_notification_actions(context.query)
    }
}

pub(crate) struct NiriProvider;

impl SearchProvider for NiriProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_niri_actions(context.query)
    }
}

pub(crate) struct HyprlandProvider;

impl SearchProvider for HyprlandProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_hyprland_actions(context.query)
    }
}

pub(crate) struct SwayProvider;

impl SearchProvider for SwayProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_sway_actions(context.query)
    }
}

pub(crate) struct WindowsProvider;

impl SearchProvider for WindowsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_windows(context.query)
    }
}

pub(crate) struct WebProvider;

impl SearchProvider for WebProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let mut actions = search_ai(context.query, &context.placeholders.preferences);
        actions.extend(search_translate(
            context.query,
            &context.placeholders.preferences,
        ));
        actions
    }
}

pub(crate) struct ClipboardProvider<'a> {
    pub(crate) entries: &'a [String],
}

impl SearchProvider for ClipboardProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        let lower = query.trim().to_lowercase();
        if lower.starts_with("clip ") || lower.starts_with("clipboard ") {
            let needle = query
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_clipboard(self.entries, needle, true)
        } else if query.trim().len() >= 3 {
            search_clipboard(self.entries, query, false)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct FilesProvider<'a> {
    pub(crate) files: &'a [FileEntry],
}

impl SearchProvider for FilesProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        let lower = query.trim().to_lowercase();
        if lower.starts_with("file ") || lower.starts_with("find ") {
            let needle = query
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_files(self.files, needle, true)
        } else if query.trim().len() >= 2 {
            search_files(self.files, query, false)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct ProcessesProvider;

impl SearchProvider for ProcessesProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        let lower = query.trim().to_lowercase();
        if lower.starts_with("proc ") || lower.starts_with("process ") {
            let needle = query
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_processes(needle)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct ScriptsProvider<'a> {
    pub(crate) entries: &'a [scripts::ScriptEntry],
}

impl SearchProvider for ScriptsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_scripts(self.entries, context.query)
    }
}

pub(crate) use scripts::{ScriptEntry, load_extension_script_entries, load_script_entries};

pub(crate) struct EmojiProvider;

impl SearchProvider for EmojiProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_emoji(context.query)
    }
}

pub(crate) fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
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

pub(crate) struct BrowserTabsProvider;

impl SearchProvider for BrowserTabsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        browser_tabs::search_browser_tabs(context.query)
    }
}

pub(crate) struct ExtensionsProvider<'a> {
    pub(crate) manifests: &'a [crate::ExtensionManifest],
}

impl SearchProvider for ExtensionsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let mut actions = Vec::new();
        for manifest in self.manifests {
            for cmd_path in &manifest.commands {
                actions.extend(extensions::search_extension(
                    cmd_path,
                    &manifest.origin.name,
                    context.query,
                ));
            }
        }
        actions
    }
}

