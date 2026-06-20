use std::io::{BufRead, BufReader};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone, Default)]
pub struct WorkspaceSnapshot {
    pub active_idx: u32,
    pub active_name: Option<String>,
    pub window_count: usize,
    pub total_workspaces: usize,
}

impl WorkspaceSnapshot {
    pub fn label(&self) -> String {
        if let Some(name) = &self.active_name {
            format!("WS {} – {}", self.active_idx, name)
        } else {
            format!("WS {}", self.active_idx)
        }
    }
}

pub fn workspace_snapshot() -> WorkspaceSnapshot {
    workspace_niri()
        .or_else(workspace_hyprland)
        .or_else(workspace_sway)
        .unwrap_or_default()
}

// ── Niri ─────────────────────────────────────────────────────────────────────

fn workspace_niri() -> Option<WorkspaceSnapshot> {
    let ws_json = run("niri", &["msg", "--json", "workspaces"])?;
    let workspaces: Vec<serde_json::Value> = serde_json::from_str(&ws_json).ok()?;

    let focused = workspaces
        .iter()
        .find(|w| w["is_focused"].as_bool().unwrap_or(false))?;

    let focused_id = focused["id"].as_u64()?;
    let active_idx = focused["idx"].as_u64().unwrap_or(1) as u32;
    let active_name = focused["name"].as_str().map(str::to_string);
    let total_workspaces = workspaces.len();

    // Count windows on focused workspace
    let window_count = run("niri", &["msg", "--json", "windows"])
        .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(&j).ok())
        .map(|windows| {
            windows
                .iter()
                .filter(|w| w["workspace_id"].as_u64() == Some(focused_id))
                .count()
        })
        .unwrap_or(0);

    Some(WorkspaceSnapshot {
        active_idx,
        active_name,
        window_count,
        total_workspaces,
    })
}

// ── Hyprland ──────────────────────────────────────────────────────────────────

fn workspace_hyprland() -> Option<WorkspaceSnapshot> {
    let json = run("hyprctl", &["-j", "activeworkspace"])?;
    let ws: serde_json::Value = serde_json::from_str(&json).ok()?;

    let active_idx = ws["id"].as_u64().unwrap_or(1) as u32;
    let active_name = ws["name"]
        .as_str()
        .filter(|n| !n.is_empty() && *n != active_idx.to_string().as_str())
        .map(str::to_string);
    let window_count = ws["windows"].as_u64().unwrap_or(0) as usize;

    // Total workspaces from hyprctl workspaces
    let total_workspaces = run("hyprctl", &["-j", "workspaces"])
        .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(&j).ok())
        .map(|v| v.len())
        .unwrap_or(1);

    Some(WorkspaceSnapshot {
        active_idx,
        active_name,
        window_count,
        total_workspaces,
    })
}

// ── Sway ─────────────────────────────────────────────────────────────────────

fn workspace_sway() -> Option<WorkspaceSnapshot> {
    let json = run("swaymsg", &["-t", "get_workspaces"])?;
    let workspaces: Vec<serde_json::Value> = serde_json::from_str(&json).ok()?;

    let focused = workspaces
        .iter()
        .find(|w| w["focused"].as_bool().unwrap_or(false))?;

    let active_idx = focused["num"].as_u64().unwrap_or(1) as u32;
    let active_name = focused["name"]
        .as_str()
        .filter(|n| *n != active_idx.to_string().as_str())
        .map(str::to_string);

    Some(WorkspaceSnapshot {
        active_idx,
        active_name,
        window_count: 0,
        total_workspaces: workspaces.len(),
    })
}

// ── Keyboard layout ──────────────────────────────────────────────────────────

/// Short code for the currently active keyboard layout (e.g. "en", "ru"),
/// queried from the running compositor. `None` if it can't be determined.
pub fn keyboard_layout() -> Option<String> {
    keyboard_layout_niri()
        .or_else(keyboard_layout_hyprland)
        .or_else(keyboard_layout_sway)
        .map(|name| layout_short_code(&name))
}

fn keyboard_layout_niri() -> Option<String> {
    let json = run("niri", &["msg", "--json", "keyboard-layouts"])?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    let idx = value["current_idx"].as_u64()? as usize;
    value["names"]
        .as_array()?
        .get(idx)?
        .as_str()
        .map(str::to_string)
}

fn keyboard_layout_hyprland() -> Option<String> {
    let json = run("hyprctl", &["-j", "devices"])?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    let keyboards = value["keyboards"].as_array()?;
    let keyboard = keyboards
        .iter()
        .find(|k| k["main"].as_bool().unwrap_or(false))
        .or_else(|| keyboards.first())?;
    keyboard["active_keymap"].as_str().map(str::to_string)
}

fn keyboard_layout_sway() -> Option<String> {
    let json = run("swaymsg", &["-t", "get_inputs"])?;
    let inputs: Vec<serde_json::Value> = serde_json::from_str(&json).ok()?;
    inputs
        .iter()
        .find_map(|input| input["xkb_active_layout_name"].as_str())
        .map(str::to_string)
}

/// Spawn a background watcher that pushes a layout code (e.g. "ru") whenever the
/// active keyboard layout *changes*. Returns a receiver to drain on the main
/// thread. Uses niri's push-based event stream for instant feedback; on other
/// compositors the watcher simply produces nothing.
pub fn layout_change_receiver() -> Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || watch_layout_niri(tx));
    rx
}

fn watch_layout_niri(tx: Sender<String>) {
    loop {
        let mut child = match Command::new("niri")
            .args(["msg", "--json", "event-stream"])
            .stdout(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            // niri isn't running (or isn't the compositor) — nothing to watch.
            Err(_) => return,
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };

        let mut names: Vec<String> = Vec::new();
        let mut last_idx: Option<usize> = None;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };

            // niri emits `KeyboardLayoutsChanged` (names + current idx) on start
            // and `KeyboardLayoutSwitched` (idx only) on a switch.
            if let Some(payload) = event.get("KeyboardLayoutsChanged") {
                let layouts = &payload["keyboard_layouts"];
                if let Some(array) = layouts["names"].as_array() {
                    names = array
                        .iter()
                        .filter_map(|n| n.as_str().map(str::to_string))
                        .collect();
                }
                let idx = layouts["current_idx"].as_u64().map(|i| i as usize);
                emit_layout_change(&tx, &names, &mut last_idx, idx);
            } else if let Some(payload) = event.get("KeyboardLayoutSwitched") {
                let idx = payload["idx"].as_u64().map(|i| i as usize);
                emit_layout_change(&tx, &names, &mut last_idx, idx);
            }
        }

        let _ = child.wait();
        // Stream ended (niri restarted / reloaded); reconnect after a beat. The
        // empty probe is ignored by the consumer but tells us whether anyone is
        // still listening — if not, stop the thread.
        std::thread::sleep(std::time::Duration::from_secs(1));
        if tx.send(String::new()).is_err() {
            return;
        }
    }
}

fn emit_layout_change(
    tx: &Sender<String>,
    names: &[String],
    last_idx: &mut Option<usize>,
    idx: Option<usize>,
) {
    let Some(idx) = idx else { return };
    // Don't fire on the initial state, only on an actual change.
    let changed = last_idx.is_some_and(|prev| prev != idx);
    *last_idx = Some(idx);
    if changed {
        if let Some(name) = names.get(idx) {
            let _ = tx.send(layout_short_code(name));
        }
    }
}

/// Map a human layout name ("English (US)", "Russian") to a 2-letter code.
pub(crate) fn layout_short_code(name: &str) -> String {
    let lower = name.to_lowercase();
    for (needle, code) in [
        ("english", "en"),
        ("russian", "ru"),
        ("ukrainian", "uk"),
        ("german", "de"),
        ("french", "fr"),
        ("spanish", "es"),
        ("italian", "it"),
        ("polish", "pl"),
    ] {
        if lower.contains(needle) {
            return code.to_string();
        }
    }
    name.chars()
        .filter(char::is_ascii_alphabetic)
        .take(2)
        .collect::<String>()
        .to_lowercase()
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_codes_map_common_names() {
        assert_eq!(layout_short_code("English (US)"), "en");
        assert_eq!(layout_short_code("Russian"), "ru");
        assert_eq!(layout_short_code("German (Deutschland)"), "de");
        // Unknown name falls back to its first two letters.
        assert_eq!(layout_short_code("Norwegian"), "no");
    }
}
