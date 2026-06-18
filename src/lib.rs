use std::io::Write;
use std::process::{Command, Stdio};

mod action;
mod app;
mod config;
mod placeholders;
mod search;
mod services;
#[cfg(feature = "gui")]
pub mod ui;

pub use action::{
    Action, ActionForm, ActionFormField, ActionPanelSection, CommandArgumentKind, LauncherCommand,
    SecondaryAction, SecondaryActionKind,
};
pub(crate) use action::{ActionKind, HttpRequest, ShellCommand};
pub use app::{
    CLIPBOARD_IMAGE_PREFIX, CalcHistoryEntry, ClipboardKind, ClipboardSummary, CommandSummary,
    SnippetSummary, Zeshicast, clipboard_image_path,
};
pub(crate) use config::{
    append_alias, home_dir, load_aliases, load_frequencies, load_lines, load_preferences,
    normalize_alias, toml_value_string, unix_now, write_lines, write_preferences,
};
pub use config::{export_config, import_config};
#[cfg(test)]
pub(crate) use placeholders::format_local_time;
pub(crate) use placeholders::{PlaceholderContext, expand_placeholders};
#[cfg(test)]
pub(crate) use search::apps::clean_desktop_exec;
pub(crate) use search::apps::{AppEntry, app_action, load_apps, search_apps};
pub(crate) use search::calculator::{Calculator, format_number, looks_like_expression};
pub(crate) use search::clipboard::{
    MAX_CLIPBOARD_ENTRIES, clipboard_preview, load_clipboard_history, normalize_clipboard_text,
    search_clipboard,
};
#[cfg(test)]
pub(crate) use search::clipboard::{decode_clipboard_line, encode_clipboard_line};
pub(crate) use search::commands::{CommandEntry, load_command_entries, search_commands};
#[cfg(test)]
pub(crate) use search::commands::{
    CommandMode, command_env, command_preferences, match_command_entry, parse_command_entry,
    parse_json_actions,
};
pub(crate) use search::files::{FileEntry, load_file_index, search_files};
pub(crate) use search::media::search_media_actions;
#[cfg(test)]
pub(crate) use search::named_values::parse_named_value;
pub(crate) use search::named_values::{
    ActionTarget, NamedValue, load_named_values, search_named_values, tagged_subtitle,
};
pub(crate) use search::notifications::search_notification_actions;
pub(crate) use search::processes::search_processes;
#[cfg(test)]
pub(crate) use search::processes::{ProcessEntry, decode_cmdline, search_process_entries};
pub(crate) use search::system::{
    SystemActionEntry, search_audio_actions, search_network_actions, search_system_actions,
};
pub(crate) use search::web::{execute_http_request, search_ai, search_translate};
pub(crate) use search::windows::{
    search_hyprland_actions, search_niri_actions, search_sway_actions, search_windows,
};
pub(crate) use search::{
    AppsProvider, AudioProvider, ClipboardProvider, CommandsProvider, EmojiProvider, FilesProvider,
    HyprlandProvider, MediaProvider, NamedValuesProvider, NetworkProvider, NiriProvider,
    NotificationsProvider, ProcessesProvider, ScriptEntry, ScriptsProvider, SearchContext,
    SearchProvider, SwayProvider, SystemProvider, WebProvider, WindowsProvider,
    load_script_entries,
};
pub use services::audio::{
    AudioDeviceOption, AudioDeviceSnapshot, AudioSnapshot, AudioStreamSnapshot, audio_snapshot,
};
pub use services::battery::{BatteryDeviceSnapshot, BatterySnapshot, battery_snapshot};
pub use services::local_ai::{LocalAiConfig, StreamChunk, ask_local_ai, ask_local_ai_streaming};
pub use services::storage as storage_service;
pub use services::media::{MediaControl, MediaSnapshot, media_control, media_snapshot};
pub use services::compositor::{WorkspaceSnapshot, workspace_snapshot};
pub use services::network::{
    NetworkInterfaceSnapshot, NetworkSnapshot, VpnConnectionSnapshot, WifiNetworkSnapshot,
    net_speed_mbps, network_snapshot,
};
pub use services::notifications::{
    NotificationAction, NotificationEntrySnapshot, NotificationSnapshot, clear_notifications,
    close_notification, mark_server_active, notification_snapshot, push_notification, toggle_dnd,
};
pub use services::system_stats::{
    ProcessSummary, SystemSnapshot, system_snapshot, top_processes_by_memory,
};
pub use services::thermal::{ThermalSnapshot, ThermalZoneSnapshot, thermal_snapshot};

const MAX_RESULTS: usize = 40;

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

pub fn copy_text(text: &str) {
    copy_to_clipboard(text);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
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
    fn clipboard_history_classifies_common_entry_types() {
        assert_eq!(
            app::classify_clipboard_text("https://tokio.rs/tokio/tutorial"),
            ClipboardKind::Url
        );
        assert_eq!(
            app::classify_clipboard_text("cargo check --features gui"),
            ClipboardKind::Command
        );
        assert_eq!(
            app::classify_clipboard_text("let msg = String::from(\"ok\");"),
            ClipboardKind::Code
        );
        assert_eq!(
            app::classify_clipboard_text("plain note"),
            ClipboardKind::Text
        );
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
            scripts: Vec::new(),
            clipboard_timestamps: HashMap::new(),
            calc_history: Vec::new(),
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
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
            scripts: Vec::new(),
            clipboard_timestamps: HashMap::new(),
            calc_history: Vec::new(),
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
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
    fn media_actions_show_on_media_prefix() {
        let results = search_media_actions("media");
        assert!(!results.is_empty());
        assert!(results.iter().any(|a| a.title == "Play/Pause"));
        assert!(results.iter().any(|a| a.title == "Next Track"));
    }

    #[test]
    fn media_actions_fuzzy_match_without_prefix() {
        let results = search_media_actions("pause");
        assert!(results.iter().any(|a| a.title == "Play/Pause"));
    }

    #[test]
    fn notification_actions_show_on_notify_prefix() {
        let results = search_notification_actions("notify");
        assert!(!results.is_empty());
        assert!(results.iter().all(|a| a.category == "Notifications"));
        assert!(results.iter().any(|a| a.title == "Toggle Do Not Disturb"));
    }

    #[test]
    fn notification_actions_match_dnd_prefix() {
        let results = search_notification_actions("dnd");
        assert!(results.iter().any(|a| a.title == "Toggle Do Not Disturb"));
    }

    #[test]
    fn launcher_commands_are_searchable() {
        let app = Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            clipboard_history: Vec::new(),
            scripts: Vec::new(),
            clipboard_timestamps: HashMap::new(),
            calc_history: Vec::new(),
            preferences: HashMap::new(),
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            files: Vec::new(),
            config_dir: PathBuf::from("/tmp/zeshicast-test"),
        };

        assert!(
            app.search("dashboard")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::Dashboard))
        );
        assert!(
            app.search("network")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::Network))
        );
        assert!(
            app.search("audio")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::Audio))
        );
        assert!(
            app.search("media")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::Media))
        );
        assert!(
            app.search("system monitor")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::SystemMonitor))
        );
        assert!(
            app.search("notifications")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::Notifications))
        );
        assert!(
            app.search("ai chat")
                .iter()
                .any(|action| action.launcher_command() == Some(LauncherCommand::AiChat))
        );
    }

    #[test]
    fn feature_toggles_hide_root_actions() {
        let mut preferences = HashMap::new();
        preferences.insert("media_enabled".to_string(), "false".to_string());
        preferences.insert("notifications_enabled".to_string(), "false".to_string());
        preferences.insert("ai_enabled".to_string(), "false".to_string());

        let app = Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            clipboard_history: Vec::new(),
            scripts: Vec::new(),
            clipboard_timestamps: HashMap::new(),
            calc_history: Vec::new(),
            preferences,
            aliases: HashMap::new(),
            pins: HashSet::new(),
            recent: Vec::new(),
            frequencies: HashMap::new(),
            files: Vec::new(),
            config_dir: PathBuf::from("/tmp/zeshicast-test"),
        };

        assert!(
            app.search("media")
                .iter()
                .all(
                    |action| action.launcher_command() != Some(LauncherCommand::Media)
                        && action.category != "Media"
                )
        );
        assert!(
            app.search("notifications")
                .iter()
                .all(
                    |action| action.launcher_command() != Some(LauncherCommand::Notifications)
                        && action.category != "Notifications"
                )
        );
        assert!(
            app.search("ai chat")
                .iter()
                .all(
                    |action| action.launcher_command() != Some(LauncherCommand::AiChat)
                        && action.category != "AI"
                )
        );
    }

    #[test]
    fn quick_ai_defaults_to_ollama_generate() {
        let actions = search_ai("ai explain gtk-rs", &HashMap::new());
        assert_eq!(actions.len(), 1);
        match &actions[0].kind {
            ActionKind::HttpCopy(HttpRequest::LocalAiGenerate {
                endpoint,
                model,
                query,
            }) => {
                assert_eq!(endpoint, "http://localhost:11434");
                assert!(model.is_empty());
                assert_eq!(query, "explain gtk-rs");
            }
            other => panic!("unexpected quick AI action: {other:?}"),
        }
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
    fn hyprland_actions_only_on_explicit_prefix() {
        assert!(search_hyprland_actions("screenshot").is_empty());
        assert!(search_hyprland_actions("workspace").is_empty());
        let with_hypr = search_hyprland_actions("hypr screenshot");
        assert!(!with_hypr.is_empty());
        assert!(with_hypr.iter().all(|a| a.category == "Hyprland"));
        let with_hyprland = search_hyprland_actions("hyprland fullscreen");
        assert!(!with_hyprland.is_empty());
        assert!(with_hyprland.iter().all(|a| a.category == "Hyprland"));
    }

    #[test]
    fn hyprland_actions_show_all_on_bare_prefix() {
        let results_hypr = search_hyprland_actions("hypr");
        assert!(!results_hypr.is_empty());
        let results_hyprland = search_hyprland_actions("hyprland");
        assert!(!results_hyprland.is_empty());
    }

    #[test]
    fn sway_actions_only_on_explicit_prefix() {
        assert!(search_sway_actions("screenshot").is_empty());
        assert!(search_sway_actions("workspace").is_empty());
        let with_prefix = search_sway_actions("sway screenshot");
        assert!(!with_prefix.is_empty());
        assert!(with_prefix.iter().all(|a| a.category == "Sway"));
    }

    #[test]
    fn sway_actions_show_all_on_bare_sway() {
        let results = search_sway_actions("sway");
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
