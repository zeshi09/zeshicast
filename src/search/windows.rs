use std::process::Command;

use crate::{Action, ActionKind, ShellCommand, SystemActionEntry, fuzzy_score};

pub(crate) fn search_niri_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let matches_prefix = lower == "niri" || lower.starts_with("niri ");
    if !matches_prefix {
        return Vec::new();
    }

    let needle = lower
        .strip_prefix("niri")
        .unwrap_or_default()
        .trim()
        .to_string();

    niri_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, &needle)
            }?;
            Some(
                Action::new(
                    "Niri",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + 260,
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn niri_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Screenshot",
            subtitle: "Interactive screenshot with region selection",
            command: "niri msg action screenshot",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Screenshot Screen",
            subtitle: "Capture the entire screen",
            command: "niri msg action screenshot-screen",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Screenshot Window",
            subtitle: "Capture the focused window",
            command: "niri msg action screenshot-window",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Fullscreen Window",
            subtitle: "Toggle fullscreen for the focused window",
            command: "niri msg action fullscreen-window",
            icon_name: "view-fullscreen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Close Window",
            subtitle: "Close the focused window",
            command: "niri msg action close-window",
            icon_name: "window-close-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Next Workspace",
            subtitle: "Focus workspace below",
            command: "niri msg action focus-workspace-down",
            icon_name: "go-down-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Previous Workspace",
            subtitle: "Focus workspace above",
            command: "niri msg action focus-workspace-up",
            icon_name: "go-up-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move Window to Next Workspace",
            subtitle: "Move focused window to workspace below",
            command: "niri msg action move-window-to-workspace-down",
            icon_name: "go-down-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move Window to Previous Workspace",
            subtitle: "Move focused window to workspace above",
            command: "niri msg action move-window-to-workspace-up",
            icon_name: "go-up-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Power Off Monitors",
            subtitle: "Turn off all monitors",
            command: "niri msg action power-off-monitors",
            icon_name: "system-shutdown-symbolic",
            hazardous: false,
        },
    ]
}

pub(crate) fn search_hyprland_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let matches_prefix = lower == "hypr"
        || lower == "hyprland"
        || lower.starts_with("hypr ")
        || lower.starts_with("hyprland ");
    if !matches_prefix {
        return Vec::new();
    }

    let needle = if lower.starts_with("hyprland") {
        lower
            .strip_prefix("hyprland")
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        lower
            .strip_prefix("hypr")
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    hyprland_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, &needle)
            }?;
            Some(
                Action::new(
                    "Hyprland",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + 260,
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn hyprland_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Screenshot",
            subtitle: "Interactive screenshot with region selection",
            command: "grim -g \"$(slurp)\" ~/Pictures/screenshot-$(date +%Y%m%d-%H%M%S).png",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Fullscreen",
            subtitle: "Toggle fullscreen for the focused window",
            command: "hyprctl dispatch fullscreen 0",
            icon_name: "view-fullscreen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Float Toggle",
            subtitle: "Toggle floating mode for the focused window",
            command: "hyprctl dispatch togglefloating",
            icon_name: "window-pop-out-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Close Window",
            subtitle: "Close the active window",
            command: "hyprctl dispatch killactive",
            icon_name: "window-close-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Reload Config",
            subtitle: "Reload Hyprland configuration",
            command: "hyprctl reload",
            icon_name: "view-refresh-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Next Workspace",
            subtitle: "Focus the next workspace",
            command: "hyprctl dispatch workspace e+1",
            icon_name: "go-next-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Previous Workspace",
            subtitle: "Focus the previous workspace",
            command: "hyprctl dispatch workspace e-1",
            icon_name: "go-previous-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move to Next Workspace",
            subtitle: "Move focused window to next workspace",
            command: "hyprctl dispatch movetoworkspace e+1",
            icon_name: "go-next-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move to Previous Workspace",
            subtitle: "Move focused window to previous workspace",
            command: "hyprctl dispatch movetoworkspace e-1",
            icon_name: "go-previous-symbolic",
            hazardous: false,
        },
    ]
}

pub(crate) fn search_sway_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let matches_prefix = lower == "sway" || lower.starts_with("sway ");
    if !matches_prefix {
        return Vec::new();
    }

    let needle = lower
        .strip_prefix("sway")
        .unwrap_or_default()
        .trim()
        .to_string();

    sway_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, &needle)
            }?;
            Some(
                Action::new(
                    "Sway",
                    entry.title,
                    ActionKind::Shell(ShellCommand::new(entry.command)),
                    score + 260,
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn sway_action_entries() -> Vec<SystemActionEntry> {
    vec![
        SystemActionEntry {
            title: "Screenshot",
            subtitle: "Capture the entire screen",
            command: "grim ~/Pictures/screenshot-$(date +%Y%m%d-%H%M%S).png",
            icon_name: "camera-photo-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Fullscreen",
            subtitle: "Toggle fullscreen for the focused window",
            command: "swaymsg fullscreen toggle",
            icon_name: "view-fullscreen-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Float Toggle",
            subtitle: "Toggle floating mode for the focused window",
            command: "swaymsg floating toggle",
            icon_name: "window-pop-out-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Close Window",
            subtitle: "Close the focused window",
            command: "swaymsg kill",
            icon_name: "window-close-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Reload Config",
            subtitle: "Reload sway configuration",
            command: "swaymsg reload",
            icon_name: "view-refresh-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Next Workspace",
            subtitle: "Focus the next workspace",
            command: "swaymsg workspace next",
            icon_name: "go-next-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Previous Workspace",
            subtitle: "Focus the previous workspace",
            command: "swaymsg workspace prev",
            icon_name: "go-previous-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move to Next Workspace",
            subtitle: "Move focused window to next workspace",
            command: "swaymsg move container to workspace next",
            icon_name: "go-next-symbolic",
            hazardous: false,
        },
        SystemActionEntry {
            title: "Move to Previous Workspace",
            subtitle: "Move focused window to previous workspace",
            command: "swaymsg move container to workspace prev",
            icon_name: "go-previous-symbolic",
            hazardous: false,
        },
    ]
}

pub(crate) fn search_windows(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let (_has_prefix, needle) = if let Some(rest) = lower
        .strip_prefix("windows ")
        .or_else(|| lower.strip_prefix("window "))
        .or_else(|| lower.strip_prefix("win "))
    {
        (true, rest.trim().to_string())
    } else if lower == "win" || lower == "window" || lower == "windows" {
        (true, String::new())
    } else {
        return Vec::new();
    };

    if let Ok(output) = Command::new("niri").args(["msg", "windows"]).output() {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                if let Ok(windows) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(arr) = windows.as_array() {
                        if !arr.is_empty() {
                            return niri_window_actions(arr, &needle);
                        }
                    }
                }
            }
        }
    }

    if let Ok(output) = Command::new("hyprctl").args(["clients", "-j"]).output() {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                if let Ok(windows) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(arr) = windows.as_array() {
                        if !arr.is_empty() {
                            return hyprland_window_actions(arr, &needle);
                        }
                    }
                }
            }
        }
    }

    if let Ok(output) = Command::new("swaymsg").args(["-t", "get_tree"]).output() {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                if let Ok(tree) = serde_json::from_str::<serde_json::Value>(text) {
                    let mut nodes = Vec::new();
                    collect_sway_windows(&tree, &mut nodes);
                    if !nodes.is_empty() {
                        return sway_window_actions(&nodes, &needle);
                    }
                }
            }
        }
    }

    Vec::new()
}

fn niri_window_actions(windows: &[serde_json::Value], needle: &str) -> Vec<Action> {
    windows
        .iter()
        .filter_map(|w| {
            let id = w.get("id")?.as_u64()?;
            let title = w.get("title")?.as_str().unwrap_or("(no title)");
            let app_id = w.get("app_id")?.as_str().unwrap_or("");
            let haystack = format!("{title} {app_id}");
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, needle)
            }?;
            Some(
                Action::new(
                    "Window",
                    title,
                    ActionKind::Shell(ShellCommand::new(format!(
                        "niri msg action focus-window --id {id}"
                    ))),
                    score + 280,
                )
                .with_subtitle(app_id)
                .with_icon("window-symbolic"),
            )
        })
        .collect()
}

fn hyprland_window_actions(windows: &[serde_json::Value], needle: &str) -> Vec<Action> {
    windows
        .iter()
        .filter_map(|w| {
            let addr = w.get("address")?.as_str()?;
            let title = w.get("title")?.as_str().unwrap_or("(no title)");
            let class = w.get("class")?.as_str().unwrap_or("");
            let haystack = format!("{title} {class}");
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, needle)
            }?;
            Some(
                Action::new(
                    "Window",
                    title,
                    ActionKind::Shell(ShellCommand::new(format!(
                        "hyprctl dispatch focuswindow address:{addr}"
                    ))),
                    score + 280,
                )
                .with_subtitle(class)
                .with_icon("window-symbolic"),
            )
        })
        .collect()
}

fn collect_sway_windows<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
    if node.get("type").and_then(|t| t.as_str()) == Some("con") {
        if node
            .get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|n| !n.is_empty())
        {
            out.push(node);
        }
    }
    if let Some(nodes) = node.get("nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            collect_sway_windows(child, out);
        }
    }
    if let Some(nodes) = node.get("floating_nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            collect_sway_windows(child, out);
        }
    }
}

fn sway_window_actions(windows: &[&serde_json::Value], needle: &str) -> Vec<Action> {
    windows
        .iter()
        .filter_map(|w| {
            let id = w.get("id")?.as_u64()?;
            let title = w.get("name")?.as_str().unwrap_or("(no title)");
            let app_id = w.get("app_id").and_then(|v| v.as_str()).unwrap_or("");
            let haystack = format!("{title} {app_id}");
            let score = if needle.is_empty() {
                Some(0)
            } else {
                fuzzy_score(&haystack, needle)
            }?;
            Some(
                Action::new(
                    "Window",
                    title,
                    ActionKind::Shell(ShellCommand::new(format!("swaymsg '[con_id={id}] focus'"))),
                    score + 280,
                )
                .with_subtitle(app_id)
                .with_icon("window-symbolic"),
            )
        })
        .collect()
}
