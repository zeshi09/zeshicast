use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::action::{Action, ActionKind};
use crate::search::MAX_RESULTS;
use crate::search::fuzzy_score_lower;

const MAX_FILE_DEPTH: usize = 5;
const MAX_INDEXED_FILES: usize = 10_000;

#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) name: String,
    /// Lowercased `name`, computed once at index time (M-6).
    pub(crate) name_lower: String,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
}

impl FileEntry {
    pub(crate) fn new(name: impl Into<String>, path: PathBuf, is_dir: bool) -> Self {
        let name = name.into();
        Self {
            name_lower: name.to_lowercase(),
            name,
            path,
            is_dir,
        }
    }
}

/// `true` when the query is routed to one dedicated provider, so the file index
/// (the most expensive one) must not be scanned (M-6).
///
/// `file `/`find ` are deliberately absent: those *are* file queries.
pub(crate) fn is_dedicated_query(query: &str) -> bool {
    let lower = query.trim().to_lowercase();
    [
        "calc ", "shell ", "notify ", "media ", "audio ", "network ", "system ", "ws ", "ai ",
        "chat ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

pub(crate) fn search_files(files: &[FileEntry], query: &str, explicit: bool) -> Vec<Action> {
    // Lowercased once per search, not once per entry (M-6).
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<Action> = files
        .iter()
        .filter_map(|file| {
            let score = fuzzy_score_lower(&file.name_lower, &query)?;
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
        files.push(FileEntry::new(name.to_string(), path.clone(), is_dir));

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn index(count: usize) -> Vec<FileEntry> {
        (0..count)
            .map(|i| {
                FileEntry::new(
                    format!("document-{i}.txt"),
                    PathBuf::from(format!("/home/user/docs/document-{i}.txt")),
                    false,
                )
            })
            .collect()
    }

    #[test]
    fn explicit_queries_do_not_scan_the_index() {
        assert!(is_dedicated_query("shell ls -la"));
        assert!(is_dedicated_query("calc 2+2"));
        assert!(!is_dedicated_query("file notes"));
        assert!(!is_dedicated_query("find notes"));
        assert!(!is_dedicated_query("notes"));
    }

    #[test]
    fn file_search_over_ten_thousand_entries_is_fast() {
        let files = index(10_000);

        let started = Instant::now();
        let matches = search_files(&files, "document-9999", false);
        let elapsed = started.elapsed();

        assert!(!matches.is_empty(), "the indexed file is found");
        println!("10k file search: {elapsed:?}");
        // The plan's target is 10 ms; this debug build measures ~7 ms. The bound
        // is deliberately looser than the target so a loaded CI runner cannot
        // fail the build, while still catching a regression of the class this
        // guards against (per-entry lowercasing/allocation).
        assert!(
            elapsed < std::time::Duration::from_millis(30),
            "searching the index must stay fast (M-6), took {elapsed:?}"
        );
    }
}
