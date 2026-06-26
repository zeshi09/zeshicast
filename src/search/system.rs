use crate::{Action, ActionKind, ActionRisk, ShellCommand, fuzzy_score};

#[derive(Debug, Clone)]
pub(crate) struct SystemActionEntry {
    pub(crate) title: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) command: &'static str,
    pub(crate) icon_name: &'static str,
    pub(crate) hazardous: bool,
}

pub(crate) fn search_system_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("system ") || lower.starts_with("sys ");
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, value)| value.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    system_actions()
        .into_iter()
        .filter(|entry| explicit || !entry.hazardous)
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                explicit.then_some(0)?
            } else {
                fuzzy_score(&haystack, needle)?
            };
            let mut action = Action::new(
                "System",
                entry.title,
                ActionKind::Shell(ShellCommand::new(entry.command)),
                score + if explicit { 260 } else { 30 },
            )
            .with_subtitle(entry.subtitle)
            .with_icon(entry.icon_name);
            if entry.hazardous {
                action = action.with_risk(ActionRisk::SystemPower);
            }
            Some(action)
        })
        .collect()
}

fn system_actions() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Lock Screen",
            subtitle: "Lock the current login session",
            command: "loginctl lock-session",
            icon_name: "system-lock-screen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Suspend",
            subtitle: "Suspend the machine",
            command: "systemctl suspend",
            icon_name: "weather-clear-night-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Open Settings",
            subtitle: "Open the desktop settings application",
            command: "gnome-control-center || systemsettings || xfce4-settings-manager",
            icon_name: "emblem-system-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Restart",
            subtitle: "Reboot the machine",
            command: "systemctl reboot",
            icon_name: "view-refresh-symbolic",
            hazardous: true,
        },
        SystemActionEntry {
            title: "Power Off",
            subtitle: "Power off the machine",
            command: "systemctl poweroff",
            icon_name: "system-shutdown-symbolic",
            hazardous: true,
        },
    ]
}

pub(crate) fn search_audio_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("audio ")
        || lower.starts_with("vol ")
        || lower.starts_with("volume ")
        || lower == "audio"
        || lower == "vol"
        || lower == "volume";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, v)| v.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    audio_action_entries()
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
                    "Audio",
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

fn audio_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Volume Up",
            subtitle: "Increase output volume by 5%",
            command: "wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+",
            icon_name: "audio-volume-high-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Volume Down",
            subtitle: "Decrease output volume by 5%",
            command: "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-",
            icon_name: "audio-volume-low-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Mute",
            subtitle: "Toggle output mute",
            command: "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle",
            icon_name: "audio-volume-muted-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Mic Mute",
            subtitle: "Toggle microphone mute",
            command: "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle",
            icon_name: "microphone-sensitivity-muted-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Brightness Up",
            subtitle: "Increase screen brightness by 10%",
            command: "brightnessctl set 10%+",
            icon_name: "display-brightness-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Brightness Down",
            subtitle: "Decrease screen brightness by 10%",
            command: "brightnessctl set 10%-",
            icon_name: "display-brightness-symbolic",
            hazardous: false,
        },
    ]
}

pub(crate) fn search_network_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("net ")
        || lower.starts_with("wifi ")
        || lower.starts_with("network ")
        || lower == "net"
        || lower == "wifi"
        || lower == "network";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, v)| v.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    network_action_entries()
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
                    "Network",
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

fn network_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Toggle WiFi",
            subtitle: "Enable or disable wireless networking",
            command: "nmcli radio wifi toggle",
            icon_name: "network-wireless-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Network Settings",
            subtitle: "Open network connection editor",
            command: "nm-connection-editor",
            icon_name: "preferences-system-network-symbolic",
            hazardous: false,
        },
    ]
}
