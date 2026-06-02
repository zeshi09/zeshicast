use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Action, ActionKind, fuzzy_score};

#[derive(Debug, Clone)]
pub(crate) struct AppEntry {
    pub(crate) name: String,
    exec: String,
    exec_name: String,
    comment: Option<String>,
    icon_name: String,
}

pub(crate) fn load_apps(home: &Path) -> Vec<AppEntry> {
    let xdg_data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));

    let xdg_data_dirs_raw = env::var_os("XDG_DATA_DIRS")
        .unwrap_or_else(|| std::ffi::OsString::from("/usr/local/share:/usr/share"));
    let mut dirs = vec![xdg_data_home.join("applications")];
    dirs.extend(
        env::split_paths(&xdg_data_dirs_raw)
            .map(|dir| dir.join("applications"))
            .collect::<Vec<_>>(),
    );

    let flatpak_user = home.join(".local/share/flatpak/exports/share/applications");
    if flatpak_user.exists() {
        dirs.push(flatpak_user);
    }

    let flatpak_system = PathBuf::from("/var/lib/flatpak/exports/share/applications");
    if flatpak_system.exists() {
        dirs.push(flatpak_system);
    }

    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path) {
                let key = app.name.to_lowercase();
                if seen.insert(key) {
                    apps.push(app);
                }
            }
        }
    }

    apps
}

fn parse_desktop_file(path: &Path) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut comment = None;
    let mut icon = None;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Exec=") {
            exec = Some(clean_desktop_exec(value));
        } else if let Some(value) = line.strip_prefix("Comment=") {
            comment = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Icon=") {
            icon = Some(value.to_string());
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Hidden=true" {
            hidden = true;
        }
    }

    if no_display || hidden {
        return None;
    }

    let icon_name = icon.unwrap_or_else(|| "application-x-executable-symbolic".to_string());
    let exec_name = exec
        .as_deref()
        .and_then(|e| e.split_whitespace().next())
        .map(|bin| {
            Path::new(bin)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(bin)
                .to_string()
        })
        .unwrap_or_default();

    Some(AppEntry {
        name: name?,
        exec: exec?,
        exec_name,
        comment,
        icon_name,
    })
}

pub(crate) fn clean_desktop_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|part| !part.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn search_apps(apps: &[AppEntry], query: &str) -> Vec<Action> {
    apps.iter()
        .filter_map(|app| {
            let haystack = {
                let mut h = app.name.clone();
                if !app.exec_name.is_empty() && app.exec_name != app.name {
                    h.push(' ');
                    h.push_str(&app.exec_name);
                }
                if let Some(c) = &app.comment {
                    h.push(' ');
                    h.push_str(c);
                }
                h
            };
            let score = fuzzy_score(&haystack, query)?;
            Some(app_action(app, score + 100))
        })
        .collect()
}

pub(crate) fn app_action(app: &AppEntry, score: i32) -> Action {
    Action::new(
        "App",
        &app.name,
        ActionKind::Launch(app.exec.clone()),
        score,
    )
    .with_subtitle(
        app.comment
            .as_deref()
            .filter(|comment| !comment.trim().is_empty())
            .unwrap_or(&app.exec),
    )
    .with_icon(&app.icon_name)
}
