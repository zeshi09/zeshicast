use std::collections::{HashSet, VecDeque};
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

/// Folders under `$HOME` that get a guaranteed share of the index before the
/// rest of `$HOME` is walked: a large tree elsewhere (`~/Git`, `~/Code/…`) must
/// not starve the places users actually look for files they just saved.
const PRIORITY_DIRS: [&str; 5] = ["Desktop", "Downloads", "Documents", "Projects", "Code"];

/// The index roots for `home`: the priority folders, then `home` itself.
///
/// Deliberately independent of the working directory. The daemon inherits it
/// from systemd or the compositor, so it says nothing about the user's intent,
/// and walking `cwd.parent()` from `$HOME` (or `/`) used to fill the whole
/// budget before `~/Downloads` was ever reached (Q0.1).
pub(crate) fn default_roots(home: &Path) -> Vec<PathBuf> {
    PRIORITY_DIRS
        .iter()
        .map(|name| home.join(name))
        .chain(std::iter::once(home.to_path_buf()))
        .collect()
}

pub(crate) fn load_file_index(home: &Path) -> Vec<FileEntry> {
    load_file_index_from(&default_roots(home), MAX_INDEXED_FILES)
}

/// Index `roots` into at most `limit` entries.
///
/// The last root is the catch-all (`$HOME`); the others are priority roots
/// nested inside it. Every root gets an equal guaranteed share, filled
/// breadth-first so shallow entries win over deep ones. Whatever budget is
/// left then goes to the priority roots' overflow first, and to the catch-all
/// last.
pub(crate) fn load_file_index_from(roots: &[PathBuf], limit: usize) -> Vec<FileEntry> {
    let Some((catch_all, priority)) = roots.split_last() else {
        return Vec::new();
    };
    let mut walk = IndexWalk {
        files: Vec::new(),
        seen: HashSet::new(),
        roots: roots.iter().cloned().collect(),
    };
    let share = limit / roots.len();
    let start = |root: &PathBuf| VecDeque::from([(root.clone(), 0)]);

    let mut overflow = Vec::new();
    for root in priority {
        let cap = (walk.files.len() + share).min(limit);
        overflow.push(walk.run(start(root), cap));
    }
    let cap = (walk.files.len() + share).min(limit);
    let catch_all_rest = walk.run(start(catch_all), cap);
    for rest in overflow {
        walk.run(rest, limit);
    }
    walk.run(catch_all_rest, limit);

    walk.files
}

struct IndexWalk {
    files: Vec<FileEntry>,
    /// Every path already considered, so a resumed walk and nested roots never
    /// index an entry twice.
    seen: HashSet<PathBuf>,
    /// Index roots: when one walk meets another root it records the folder but
    /// leaves its contents to that root's own walk and budget.
    roots: HashSet<PathBuf>,
}

impl IndexWalk {
    /// Walk `queue` breadth-first until the index holds `cap` entries. Returns
    /// the unfinished part of the queue so the walk can resume later.
    fn run(
        &mut self,
        mut queue: VecDeque<(PathBuf, usize)>,
        cap: usize,
    ) -> VecDeque<(PathBuf, usize)> {
        while let Some((dir, depth)) = queue.pop_front() {
            if depth > MAX_FILE_DEPTH {
                continue;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };

            for entry in entries.flatten() {
                if self.files.len() >= cap {
                    // Re-read this folder on resume; `seen` skips what is done.
                    queue.push_front((dir, depth));
                    return queue;
                }

                let path = entry.path();
                if !self.seen.insert(path.clone()) {
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
                if is_dir && !self.roots.contains(&path) {
                    queue.push_back((path.clone(), depth + 1));
                }
                self.files
                    .push(FileEntry::new(name.to_string(), path, is_dir));
            }
        }
        queue
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

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().expect("test paths have a parent"))
            .expect("create test folder");
        fs::write(path, b"").expect("create test file");
    }

    fn crowd(dir: &Path, count: usize) {
        for i in 0..count {
            touch(&dir.join(format!("crowd-{i}.txt")));
        }
    }

    fn indexed(files: &[FileEntry], name: &str) -> bool {
        files.iter().any(|file| file.name == name)
    }

    #[test]
    fn default_roots_stay_inside_home_and_ignore_the_working_directory() {
        let home = Path::new("/home/user");
        let roots = default_roots(home);

        assert!(roots.iter().all(|root| root.starts_with(home)));
        assert_eq!(roots.last().map(PathBuf::as_path), Some(home));
        assert!(roots.contains(&home.join("Downloads")));
    }

    #[test]
    fn priority_folders_are_indexed_even_when_home_is_crowded() {
        // Q0.1: a big tree elsewhere in $HOME used to exhaust the budget before
        // ~/Downloads was reached, so freshly downloaded files were unfindable.
        let home = tempfile::tempdir().expect("temp home");
        crowd(&home.path().join("Git/big"), 300);
        touch(&home.path().join("Downloads/deep/er/needle.pdf"));

        let files = load_file_index_from(&default_roots(home.path()), 60);

        assert!(indexed(&files, "needle.pdf"), "~/Downloads must be indexed");
        assert!(files.len() <= 60);
    }

    #[test]
    fn a_huge_priority_folder_does_not_starve_home() {
        let home = tempfile::tempdir().expect("temp home");
        crowd(&home.path().join("Code/monorepo"), 300);
        touch(&home.path().join("notes.md"));

        let files = load_file_index_from(&default_roots(home.path()), 60);

        assert!(
            indexed(&files, "notes.md"),
            "top-level home files keep a share"
        );
        assert!(files.len() <= 60);
    }

    #[test]
    fn spare_budget_finishes_a_priority_folder_before_deep_home_content() {
        let home = tempfile::tempdir().expect("temp home");
        crowd(&home.path().join("Downloads"), 40);
        crowd(&home.path().join("Git/a/b/c"), 300);

        // 6 roots → a share of 10 each; Downloads needs 40.
        let files = load_file_index_from(&default_roots(home.path()), 60);

        let downloads = files
            .iter()
            .filter(|file| file.path.parent() == Some(&home.path().join("Downloads")))
            .count();
        assert_eq!(downloads, 40, "overflow of a priority folder is resumed");
        assert!(files.len() <= 60);
    }

    #[test]
    fn index_has_no_duplicates_and_records_priority_folders_once() {
        let home = tempfile::tempdir().expect("temp home");
        touch(&home.path().join("Downloads/a.txt"));
        touch(&home.path().join("Documents/b.txt"));
        touch(&home.path().join("c.txt"));

        let files = load_file_index_from(&default_roots(home.path()), 1_000);

        let unique: HashSet<_> = files.iter().map(|file| &file.path).collect();
        assert_eq!(unique.len(), files.len());
        for name in ["a.txt", "b.txt", "c.txt", "Downloads", "Documents"] {
            assert!(indexed(&files, name), "{name} is indexed");
        }
    }

    #[test]
    fn a_zero_budget_or_no_roots_yields_an_empty_index() {
        let home = tempfile::tempdir().expect("temp home");
        touch(&home.path().join("Downloads/a.txt"));

        assert!(load_file_index_from(&default_roots(home.path()), 0).is_empty());
        assert!(load_file_index_from(&[], 100).is_empty());
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
