use std::process::Command;

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

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}
