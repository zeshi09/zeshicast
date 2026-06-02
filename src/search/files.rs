use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Action, ActionKind, MAX_RESULTS, fuzzy_score};

const MAX_FILE_DEPTH: usize = 5;
const MAX_INDEXED_FILES: usize = 10_000;

#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
}

pub(crate) fn search_files(files: &[FileEntry], query: &str, explicit: bool) -> Vec<Action> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<Action> = files
        .iter()
        .filter_map(|file| {
            let score = fuzzy_score(&file.name, query)?;
            let category = if file.is_dir { "Folder" } else { "File" };
            let subtitle = file
                .path
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default();
            let icon_name = if file.is_dir {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            };
            Some(
                Action::new(
                    category,
                    &file.name,
                    ActionKind::OpenPath(file.path.clone()),
                    score + if explicit { 90 } else { 15 },
                )
                .with_subtitle(subtitle)
                .with_icon(icon_name),
            )
        })
        .collect();

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    matches.truncate(if explicit { MAX_RESULTS } else { 4 });
    matches
}

pub(crate) fn load_file_index(home: &Path) -> Vec<FileEntry> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut roots = Vec::new();

    if let Ok(cwd) = env::current_dir() {
        if let Some(parent) = cwd.parent() {
            roots.push(parent.to_path_buf());
        }
        roots.push(cwd);
    }

    for name in ["Code", "Documents", "Downloads", "Desktop", "Projects"] {
        roots.push(home.join(name));
    }
    roots.push(home.to_path_buf());

    for root in roots {
        visit_files(&root, 0, &mut files, &mut seen);
        if files.len() >= MAX_INDEXED_FILES {
            break;
        }
    }

    files
}

fn visit_files(dir: &Path, depth: usize, files: &mut Vec<FileEntry>, seen: &mut HashSet<PathBuf>) {
    if depth > MAX_FILE_DEPTH || files.len() >= MAX_INDEXED_FILES {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if files.len() >= MAX_INDEXED_FILES {
            return;
        }

        let path = entry.path();
        if !seen.insert(path.clone()) {
            continue;
        }

        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if should_skip_file(&name) {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let is_dir = file_type.is_dir();
        files.push(FileEntry {
            name: name.to_string(),
            path: path.clone(),
            is_dir,
        });

        if is_dir {
            visit_files(&path, depth + 1, files, seen);
        }
    }
}

fn should_skip_file(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }

    matches!(
        name,
        "target"
            | "node_modules"
            | ".git"
            | ".cache"
            | ".cargo"
            | ".rustup"
            | ".local"
            | ".npm"
            | ".var"
            | "Trash"
    )
}
