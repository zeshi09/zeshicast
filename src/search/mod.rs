pub(crate) mod apps;
pub(crate) mod calculator;
pub(crate) mod clipboard;
pub(crate) mod commands;
pub(crate) mod emoji;
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
    pub(crate) placeholders: &'a PlaceholderContext,
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
