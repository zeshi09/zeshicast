use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{Action, ActionKind, ShellCommand, SystemActionEntry, fuzzy_score};

const WINDOW_QUERY_TIMEOUT: Duration = Duration::from_millis(200);
const WINDOW_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowBackend {
    Niri,
    Hyprland,
    Sway,
}

#[derive(Debug, Clone)]
struct WindowSnapshot {
    backend: WindowBackend,
    windows: Vec<serde_json::Value>,
    captured_at: Instant,
}

fn window_snapshot_cache() -> &'static Mutex<Option<WindowSnapshot>> {
    static CACHE: OnceLock<Mutex<Option<WindowSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

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
    let Some(needle) = window_query_needle(query) else {
        return Vec::new();
    };

    if let Some(snapshot) = fresh_window_snapshot() {
        return window_snapshot_actions(&snapshot, &needle);
    }

    let Some(snapshot) = load_window_snapshot() else {
        return stale_window_snapshot()
            .map(|snapshot| window_snapshot_actions(&snapshot, &needle))
            .unwrap_or_default();
    };

    if let Ok(mut cached) = window_snapshot_cache().lock() {
        *cached = Some(snapshot.clone());
    }
    window_snapshot_actions(&snapshot, &needle)
}

fn window_query_needle(query: &str) -> Option<String> {
    let lower = query.trim().to_lowercase();
    if let Some(rest) = lower
        .strip_prefix("windows ")
        .or_else(|| lower.strip_prefix("window "))
        .or_else(|| lower.strip_prefix("win "))
    {
        Some(rest.trim().to_string())
    } else if lower == "win" || lower == "window" || lower == "windows" {
        Some(String::new())
    } else {
        None
    }
}

fn fresh_window_snapshot() -> Option<WindowSnapshot> {
    let cached = window_snapshot_cache().lock().ok()?.clone()?;
    (cached.captured_at.elapsed() <= WINDOW_CACHE_TTL).then_some(cached)
}

fn stale_window_snapshot() -> Option<WindowSnapshot> {
    window_snapshot_cache().lock().ok()?.clone()
}

fn load_window_snapshot() -> Option<WindowSnapshot> {
    for backend in window_backend_candidates() {
        if let Some(snapshot) = load_backend_snapshot(backend) {
            return Some(snapshot);
        }
    }
    None
}

fn window_backend_candidates() -> Vec<WindowBackend> {
    let env = std::env::vars().collect::<HashMap<_, _>>();
    window_backend_candidates_from_env(&env)
}

fn window_backend_candidates_from_env(env: &HashMap<String, String>) -> Vec<WindowBackend> {
    if env
        .get("NIRI_SOCKET")
        .is_some_and(|value| !value.is_empty())
        || env
            .get("XDG_CURRENT_DESKTOP")
            .is_some_and(|value| value.to_lowercase().contains("niri"))
    {
        return vec![WindowBackend::Niri];
    }
    if env
        .get("HYPRLAND_INSTANCE_SIGNATURE")
        .is_some_and(|value| !value.is_empty())
        || env
            .get("XDG_CURRENT_DESKTOP")
            .is_some_and(|value| value.to_lowercase().contains("hyprland"))
    {
        return vec![WindowBackend::Hyprland];
    }
    if env.get("SWAYSOCK").is_some_and(|value| !value.is_empty())
        || env
            .get("XDG_CURRENT_DESKTOP")
            .is_some_and(|value| value.to_lowercase().contains("sway"))
    {
        return vec![WindowBackend::Sway];
    }

    vec![
        WindowBackend::Niri,
        WindowBackend::Hyprland,
        WindowBackend::Sway,
    ]
}

fn load_backend_snapshot(backend: WindowBackend) -> Option<WindowSnapshot> {
    let windows = match backend {
        WindowBackend::Niri => command_json_value("niri", &["msg", "windows"])
            .and_then(|value| value.as_array().cloned())?,
        WindowBackend::Hyprland => command_json_value("hyprctl", &["clients", "-j"])
            .and_then(|value| value.as_array().cloned())?,
        WindowBackend::Sway => {
            let tree = command_json_value("swaymsg", &["-t", "get_tree"])?;
            let mut nodes = Vec::new();
            collect_sway_windows(&tree, &mut nodes);
            nodes.into_iter().cloned().collect()
        }
    };

    (!windows.is_empty()).then_some(WindowSnapshot {
        backend,
        windows,
        captured_at: Instant::now(),
    })
}

fn command_json_value(program: &str, args: &[&str]) -> Option<serde_json::Value> {
    let output = command_output_with_timeout(program, args, WINDOW_QUERY_TIMEOUT)?;
    if !output.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&output.stdout).ok()?;
    serde_json::from_str(text).ok()
}

fn command_output_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Option<std::process::Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
}

fn window_snapshot_actions(snapshot: &WindowSnapshot, needle: &str) -> Vec<Action> {
    match snapshot.backend {
        WindowBackend::Niri => niri_window_actions(&snapshot.windows, needle),
        WindowBackend::Hyprland => hyprland_window_actions(&snapshot.windows, needle),
        WindowBackend::Sway => {
            let windows = snapshot.windows.iter().collect::<Vec<_>>();
            sway_window_actions(&windows, needle)
        }
    }
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
    if node.get("type").and_then(|t| t.as_str()) == Some("con")
        && node
            .get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|n| !n.is_empty())
    {
        out.push(node);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_query_needle_accepts_window_prefixes() {
        assert_eq!(
            window_query_needle("win firefox").as_deref(),
            Some("firefox")
        );
        assert_eq!(window_query_needle("window Code").as_deref(), Some("code"));
        assert_eq!(window_query_needle("windows").as_deref(), Some(""));
        assert_eq!(window_query_needle("firefox"), None);
    }

    #[test]
    fn backend_candidates_prefer_detected_compositor() {
        let mut env = HashMap::new();
        env.insert("HYPRLAND_INSTANCE_SIGNATURE".to_string(), "abc".to_string());
        assert_eq!(
            window_backend_candidates_from_env(&env),
            vec![WindowBackend::Hyprland]
        );

        env.clear();
        env.insert("XDG_CURRENT_DESKTOP".to_string(), "sway".to_string());
        assert_eq!(
            window_backend_candidates_from_env(&env),
            vec![WindowBackend::Sway]
        );
    }

    #[test]
    fn backend_candidates_probe_all_without_compositor_env() {
        let env = HashMap::new();
        assert_eq!(
            window_backend_candidates_from_env(&env),
            vec![
                WindowBackend::Niri,
                WindowBackend::Hyprland,
                WindowBackend::Sway,
            ]
        );
    }

    #[test]
    fn command_output_with_timeout_stops_slow_process() {
        let started_at = Instant::now();
        let output = command_output_with_timeout("sleep", &["1"], Duration::from_millis(50));

        assert!(output.is_none());
        assert!(started_at.elapsed() < Duration::from_millis(500));
    }
}
