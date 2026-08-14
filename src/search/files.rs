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

pub(crate) fn should_skip_file(name: &str) -> bool {
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

#[allow(dead_code)]
pub(crate) fn should_skip_path(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if should_skip_file(&name) {
            return true;
        }
    }
    false
}

#[allow(dead_code)]
pub(crate) fn add_file_entry(files: &mut Vec<FileEntry>, path: PathBuf) {
    if should_skip_path(&path) {
        return;
    }
    if let Some(file_name) = path.file_name() {
        let name = file_name.to_string_lossy().to_string();
        let is_dir = path.is_dir();
        // Remove existing entry with same path if present
        files.retain(|f| f.path != path);
        if files.len() < MAX_INDEXED_FILES {
            files.push(FileEntry { name, path, is_dir });
        }
    }
}

#[allow(dead_code)]
pub(crate) fn remove_file_entry(files: &mut Vec<FileEntry>, path: &Path) {
    files.retain(|f| f.path != path && !f.path.starts_with(path));
}

pub(crate) struct FileWatcherHandle {
    _watcher: Option<notify::RecommendedWatcher>,
}

#[allow(dead_code)]
impl FileWatcherHandle {
    pub(crate) fn start<F>(home: &Path, on_event: F) -> Self
    where
        F: Fn(notify::Event) + Send + Sync + 'static,
    {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

        let mut watcher = match RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    on_event(event);
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to initialize file watcher: {}", e);
                return Self { _watcher: None };
            }
        };

        for name in ["Code", "Documents", "Downloads", "Desktop", "Projects"] {
            let dir = home.join(name);
            if dir.is_dir() {
                let _ = watcher.watch(&dir, RecursiveMode::Recursive);
            }
        }

        Self {
            _watcher: Some(watcher),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_files_and_paths() {
        assert!(should_skip_file(".git"));
        assert!(should_skip_file("node_modules"));
        assert!(should_skip_file("target"));
        assert!(!should_skip_file("main.rs"));

        assert!(should_skip_path(Path::new("/home/user/project/target/debug/app")));
        assert!(should_skip_path(Path::new("/home/user/project/node_modules/pkg/index.js")));
        assert!(!should_skip_path(Path::new("/home/user/Documents/report.pdf")));
    }

    #[test]
    fn test_add_and_remove_file_entry() {
        let mut files = Vec::new();
        let path = PathBuf::from("/tmp/test_file.txt");
        add_file_entry(&mut files, path.clone());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test_file.txt");

        remove_file_entry(&mut files, &path);
        assert_eq!(files.len(), 0);
    }
}
