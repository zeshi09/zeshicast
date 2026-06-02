use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone)]
struct NotificationActionEntry {
    title: &'static str,
    subtitle: &'static str,
    command: &'static str,
    icon_name: &'static str,
}

pub(crate) fn search_notification_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("notify ")
        || lower.starts_with("notifications ")
        || lower.starts_with("notification ")
        || lower.starts_with("dnd ")
        || lower == "notify"
        || lower == "notifications"
        || lower == "notification"
        || lower == "dnd";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, value)| value.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    notification_action_entries()
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
                    "Notifications",
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

fn notification_action_entries() -> Vec<NotificationActionEntry> {
    vec![
        NotificationActionEntry {
            title: "Toggle Do Not Disturb",
            subtitle: "Toggle DND in swaync or dunst",
            command: "swaync-client --toggle-dnd || dunstctl set-paused toggle",
            icon_name: "notifications-disabled-symbolic",
        },
        NotificationActionEntry {
            title: "Close All Notifications",
            subtitle: "Dismiss visible notifications in swaync or dunst",
            command: "swaync-client --close-all || dunstctl close-all",
            icon_name: "edit-clear-all-symbolic",
        },
        NotificationActionEntry {
            title: "Open Notification Panel",
            subtitle: "Open notification panel when swaync is available",
            command: "swaync-client --toggle-panel",
            icon_name: "preferences-system-notifications-symbolic",
        },
    ]
}
