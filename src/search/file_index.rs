//! Persistent, live file index (Q6.1).
//!
//! The index lives in the `files` table of `zeshicast.db`. On start the rows
//! are loaded (search works at once), then only the folders whose mtime
//! changed while the daemon was down are re-read. After that an inotify
//! watcher marks folders dirty; a debounce batches bursts (an unpacked
//! archive, a build) into one rescan of each dirty folder.
//!
//! Bounds: the walk keeps the depth and size limits of the first full scan
//! (`MAX_FILE_DEPTH`, `MAX_INDEXED_FILES`), live additions may grow it to
//! [`LIVE_LIMIT`], at most [`MAX_WATCHES`] folders are watched (shallow ones
//! first), and `files_exclude` keeps names and paths out.

// The headless CLI only loads the stored index; the watcher is the daemon's.
#![cfg_attr(not(feature = "gui"), allow(dead_code))]

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::files::{
    Excludes, FileEntry, MAX_FILE_DEPTH, MAX_INDEXED_FILES, load_file_index_excluding,
    should_skip_file,
};
use super::gitignore::GitIgnores;
use crate::services::storage::{self, FileRow};

/// Live additions may grow the index this far past the first scan's budget,
/// so a file saved into a full index is still found.
pub(crate) const LIVE_LIMIT: usize = MAX_INDEXED_FILES + MAX_INDEXED_FILES / 5;
/// Folders watched at most; the rest is picked up when its parent changes or
/// at the next start.
pub(crate) const MAX_WATCHES: usize = 8192;
/// A folder is rescanned this long after its last event…
const QUIET: Duration = Duration::from_millis(400);
/// …or this long after its first one, whichever comes first.
const MAX_WAIT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Indexed {
    is_dir: bool,
    /// Folders only: nanoseconds since the epoch, 0 when unknown.
    mtime: i64,
}

/// What a rescan changed, for the database and the watcher.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    /// New or updated paths (a folder whose mtime moved is updated).
    pub(crate) upserted: Vec<PathBuf>,
    pub(crate) removed: Vec<PathBuf>,
    /// New folders, to watch.
    pub(crate) dirs_added: Vec<PathBuf>,
    /// Removed folders, to stop watching.
    pub(crate) dirs_removed: Vec<PathBuf>,
}

impl Changes {
    pub(crate) fn is_empty(&self) -> bool {
        self.upserted.is_empty() && self.removed.is_empty()
    }

    fn absorb(&mut self, other: Changes) {
        self.upserted.extend(other.upserted);
        self.removed.extend(other.removed);
        self.dirs_added.extend(other.dirs_added);
        self.dirs_removed.extend(other.dirs_removed);
    }
}

pub(crate) struct FileIndex {
    /// Priority roots, then the catch-all (`$HOME`) last, as in
    /// [`super::files::default_roots`].
    roots: Vec<PathBuf>,
    excludes: Excludes,
    gitignores: GitIgnores,
    limit: usize,
    entries: HashMap<PathBuf, Indexed>,
}

/// `.gitignore` rules for repositories below the catch-all root.
fn gitignores_for(roots: &[PathBuf]) -> GitIgnores {
    GitIgnores::new(roots.last().map(PathBuf::as_path).unwrap_or(Path::new("/")))
}

fn mtime_of(meta: &fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    meta.mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(meta.mtime_nsec())
}

fn dir_mtime(path: &Path) -> Option<i64> {
    let meta = fs::symlink_metadata(path).ok()?;
    meta.is_dir().then(|| mtime_of(&meta))
}

impl FileIndex {
    /// A full walk, as the index was built before Q6.1.
    pub(crate) fn build(roots: Vec<PathBuf>, excludes: Excludes) -> Self {
        let walked = load_file_index_excluding(&roots, MAX_INDEXED_FILES, &excludes);
        let entries = walked
            .into_iter()
            .map(|file| {
                let mtime = if file.is_dir {
                    dir_mtime(&file.path).unwrap_or(0)
                } else {
                    0
                };
                (
                    file.path,
                    Indexed {
                        is_dir: file.is_dir,
                        mtime,
                    },
                )
            })
            .collect();
        Self {
            gitignores: gitignores_for(&roots),
            roots,
            excludes,
            limit: LIVE_LIMIT,
            entries,
        }
    }

    /// The index as the database stored it, minus rows that are no longer
    /// under a root or are excluded now.
    pub(crate) fn from_rows(roots: Vec<PathBuf>, excludes: Excludes, rows: Vec<FileRow>) -> Self {
        let mut index = Self {
            gitignores: gitignores_for(&roots),
            roots,
            excludes,
            limit: LIVE_LIMIT,
            entries: HashMap::new(),
        };
        // Files share folders: judge each folder once, not once per file.
        let mut folders = HashMap::new();
        for row in rows {
            if index.keeps_in(&row.path, row.is_dir, &mut folders) {
                index.entries.insert(
                    row.path,
                    Indexed {
                        is_dir: row.is_dir,
                        mtime: row.mtime,
                    },
                );
            }
        }
        index
    }

    #[cfg(test)]
    fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The root `path` belongs to (the deepest one containing it) and its
    /// depth below that root (`root/x` is 1).
    fn depth_of(&self, path: &Path) -> Option<usize> {
        self.roots
            .iter()
            .filter(|root| path.starts_with(root))
            .map(|root| path.components().count() - root.components().count())
            .min()
    }

    /// Under a root, within the depth, and no component skipped, excluded or
    /// ignored by git.
    fn keeps(&self, path: &Path, is_dir: bool) -> bool {
        let Some(depth) = self.depth_of(path) else {
            return false;
        };
        if depth == 0 || depth > MAX_FILE_DEPTH + 1 {
            return false;
        }
        let mut current = path;
        let mut current_is_dir = is_dir;
        for _ in 0..depth {
            let name = current
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            if should_skip_file(&name)
                || self.excludes.matches(current, &name)
                || self.gitignores.ignores(current, current_is_dir)
            {
                return false;
            }
            current_is_dir = true;
            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }
        true
    }

    /// [`keeps`](Self::keeps), with the verdict on `path`'s folder taken from
    /// (or recorded in) `folders`: its depth below the root when it is kept
    /// or is a root, `None` when it is not.
    fn keeps_in(
        &self,
        path: &Path,
        is_dir: bool,
        folders: &mut HashMap<PathBuf, Option<usize>>,
    ) -> bool {
        let Some(parent) = path.parent() else {
            return false;
        };
        let parent_depth = match folders.get(parent) {
            Some(depth) => *depth,
            None => {
                let depth = self
                    .depth_of(parent)
                    .filter(|depth| *depth == 0 || self.keeps(parent, true));
                folders.insert(parent.to_path_buf(), depth);
                depth
            }
        };
        let Some(parent_depth) = parent_depth else {
            return false;
        };
        if parent_depth + 1 > MAX_FILE_DEPTH + 1 || self.roots.iter().any(|root| root == path) {
            return false;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        !should_skip_file(&name)
            && !self.excludes.matches(path, &name)
            && !self.gitignores.ignores(path, is_dir)
    }

    /// Paths indexed in the search's shape.
    pub(crate) fn snapshot(&self) -> Vec<FileEntry> {
        let mut paths: Vec<_> = self.entries.iter().collect();
        paths.sort_by(|a, b| a.0.cmp(b.0));
        paths
            .into_iter()
            .map(|(path, entry)| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                FileEntry::new(name, path.clone(), entry.is_dir)
            })
            .collect()
    }

    /// Database rows for `paths` that are (still) indexed.
    pub(crate) fn rows(&self, paths: &[PathBuf]) -> Vec<FileRow> {
        paths
            .iter()
            .filter_map(|path| {
                self.entries.get(path).map(|entry| FileRow {
                    path: path.clone(),
                    is_dir: entry.is_dir,
                    mtime: entry.mtime,
                })
            })
            .collect()
    }

    pub(crate) fn all_rows(&self) -> Vec<FileRow> {
        let paths: Vec<_> = self.entries.keys().cloned().collect();
        self.rows(&paths)
    }

    /// Folders to watch: roots first, then indexed folders by depth, capped.
    pub(crate) fn watch_list(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.is_dir)
            .map(|(path, _)| path.clone())
            .collect();
        dirs.sort_by_key(|path| (path.components().count(), path.clone()));
        let mut list = self.roots.clone();
        for dir in dirs {
            if !list.contains(&dir) {
                list.push(dir);
            }
        }
        list.truncate(MAX_WATCHES);
        list
    }

    /// Folders to re-read after a restart: every root (cheap) and every
    /// indexed folder whose mtime is not the stored one.
    pub(crate) fn stale_dirs(&self) -> Vec<PathBuf> {
        let mut stale = self.roots.clone();
        let mut changed: Vec<_> = self
            .entries
            .iter()
            .filter(|(path, entry)| entry.is_dir && dir_mtime(path) != Some(entry.mtime))
            .map(|(path, _)| path.clone())
            .collect();
        changed.sort_by_key(|path| path.components().count());
        stale.extend(changed);
        stale
    }

    fn remove_tree(&mut self, top: &Path, changes: &mut Changes) {
        let doomed: Vec<_> = self
            .entries
            .keys()
            .filter(|path| path.starts_with(top))
            .cloned()
            .collect();
        for path in doomed {
            if let Some(entry) = self.entries.remove(&path) {
                if entry.is_dir {
                    changes.dirs_removed.push(path.clone());
                }
                changes.removed.push(path);
            }
        }
    }

    /// Re-read one folder: drop children that are gone (with everything
    /// below them), add new ones, and walk new subfolders.
    pub(crate) fn rescan_dir(&mut self, dir: &Path) -> Changes {
        let mut changes = Changes::default();
        let is_root = self.roots.iter().any(|root| root == dir);
        if !is_root && !self.entries.contains_key(dir) {
            // Not indexed (excluded, too deep, over budget): its parent's
            // rescan decides whether it comes in.
            return changes;
        }
        let Some(depth) = self.depth_of(dir) else {
            return changes;
        };

        let meta = fs::symlink_metadata(dir).ok().filter(fs::Metadata::is_dir);
        let Some(meta) = meta else {
            if is_root {
                let children: Vec<_> = self
                    .entries
                    .keys()
                    .filter(|path| path.parent() == Some(dir))
                    .cloned()
                    .collect();
                for child in children {
                    self.remove_tree(&child, &mut changes);
                }
            } else {
                self.remove_tree(dir, &mut changes);
            }
            return changes;
        };
        if let Some(entry) = self.entries.get_mut(dir) {
            let mtime = mtime_of(&meta);
            if entry.mtime != mtime {
                entry.mtime = mtime;
                changes.upserted.push(dir.to_path_buf());
            }
        }
        if depth > MAX_FILE_DEPTH {
            return changes;
        }

        let present = self.list(dir);
        let indexed_children: Vec<_> = self
            .entries
            .iter()
            .filter(|(path, _)| path.parent() == Some(dir))
            .map(|(path, entry)| (path.clone(), entry.is_dir))
            .collect();
        for (child, is_dir) in indexed_children {
            if present.get(&child) != Some(&is_dir) {
                self.remove_tree(&child, &mut changes);
            }
        }
        let mut new_dirs = VecDeque::new();
        let mut present: Vec<_> = present.into_iter().collect();
        present.sort();
        for (child, is_dir) in present {
            if self.entries.contains_key(&child) {
                continue;
            }
            if !self.insert(&child, is_dir, &mut changes) {
                break;
            }
            if is_dir && !self.roots.contains(&child) {
                new_dirs.push_back((child, depth + 1));
            }
        }
        self.walk_new(new_dirs, &mut changes);
        changes
    }

    /// Children of `dir` that belong in the index: name → is_dir.
    fn list(&self, dir: &Path) -> HashMap<PathBuf, bool> {
        let Ok(read) = fs::read_dir(dir) else {
            return HashMap::new();
        };
        read.flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let path = entry.path();
                if should_skip_file(&name) || self.excludes.matches(&path, &name) {
                    return None;
                }
                let file_type = entry.file_type().ok()?;
                if file_type.is_symlink() || self.gitignores.ignores(&path, file_type.is_dir()) {
                    return None;
                }
                Some((path, file_type.is_dir()))
            })
            .collect()
    }

    /// Add one path; `false` once the index is full.
    fn insert(&mut self, path: &Path, is_dir: bool, changes: &mut Changes) -> bool {
        if self.entries.len() >= self.limit {
            return false;
        }
        let mtime = if is_dir {
            dir_mtime(path).unwrap_or(0)
        } else {
            0
        };
        self.entries
            .insert(path.to_path_buf(), Indexed { is_dir, mtime });
        changes.upserted.push(path.to_path_buf());
        if is_dir {
            changes.dirs_added.push(path.to_path_buf());
        }
        true
    }

    /// Breadth-first walk of folders that just appeared (a copied tree):
    /// inotify only reports the top folder, its contents were already there.
    fn walk_new(&mut self, mut queue: VecDeque<(PathBuf, usize)>, changes: &mut Changes) {
        let mut seen = HashSet::new();
        while let Some((dir, depth)) = queue.pop_front() {
            if depth > MAX_FILE_DEPTH || !seen.insert(dir.clone()) {
                continue;
            }
            let mut children: Vec<_> = self.list(&dir).into_iter().collect();
            children.sort();
            for (child, is_dir) in children {
                if self.entries.contains_key(&child) {
                    continue;
                }
                if !self.insert(&child, is_dir, changes) {
                    return;
                }
                if is_dir && !self.roots.contains(&child) {
                    queue.push_back((child, depth + 1));
                }
            }
        }
    }

    /// Rescan each folder, parents before children.
    pub(crate) fn rescan_all(&mut self, dirs: impl IntoIterator<Item = PathBuf>) -> Changes {
        // A `.gitignore` may be among the changes: read the rules afresh.
        self.gitignores.forget();
        let mut dirs: Vec<_> = dirs
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        dirs.sort_by_key(|path| path.components().count());
        let mut changes = Changes::default();
        for dir in dirs {
            changes.absorb(self.rescan_dir(&dir));
        }
        changes
    }
}

/// Batches dirty folders: due [`QUIET`] after the last event, or [`MAX_WAIT`]
/// after the first, so a steady stream of writes cannot postpone the rescan
/// forever. Time is passed in, so tests drive it without sleeping.
#[derive(Debug)]
pub(crate) struct Debouncer {
    quiet: Duration,
    max_wait: Duration,
    pending: BTreeSet<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new(QUIET, MAX_WAIT)
    }
}

impl Debouncer {
    pub(crate) fn new(quiet: Duration, max_wait: Duration) -> Self {
        Self {
            quiet,
            max_wait,
            pending: BTreeSet::new(),
            first: None,
            last: None,
        }
    }

    pub(crate) fn push(&mut self, dir: PathBuf, now: Instant) {
        self.pending.insert(dir);
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// How long to wait for more events before [`Self::take`] is due;
    /// `None` when nothing is pending.
    pub(crate) fn wait_for(&self, now: Instant) -> Option<Duration> {
        let (first, last) = (self.first?, self.last?);
        let deadline = (last + self.quiet).min(first + self.max_wait);
        Some(deadline.saturating_duration_since(now))
    }

    /// The batch, when it is due.
    pub(crate) fn take(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        if self.wait_for(now)? > Duration::ZERO {
            return None;
        }
        self.first = None;
        self.last = None;
        Some(std::mem::take(&mut self.pending).into_iter().collect())
    }
}

/// An inotify instance watching folders by path.
pub(crate) struct Watcher {
    fd: rustix::fd::OwnedFd,
    by_wd: HashMap<i32, PathBuf>,
    by_dir: HashMap<PathBuf, i32>,
}

/// What a batch of inotify events means for the index.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WatchEvent {
    /// Something inside this folder was created, deleted or moved.
    Dirty(PathBuf),
    /// The kernel dropped events: everything must be checked.
    Overflow,
}

impl Watcher {
    pub(crate) fn new() -> std::io::Result<Self> {
        use rustix::fs::inotify::{CreateFlags, init};
        Ok(Self {
            fd: init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK)?,
            by_wd: HashMap::new(),
            by_dir: HashMap::new(),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.by_dir.len()
    }

    /// Watch `dir`; `false` past [`MAX_WATCHES`] or when the kernel refuses
    /// (its own `max_user_watches`).
    pub(crate) fn watch(&mut self, dir: &Path) -> bool {
        use rustix::fs::inotify::{WatchFlags, add_watch};
        if self.by_dir.contains_key(dir) {
            return true;
        }
        if self.by_dir.len() >= MAX_WATCHES {
            return false;
        }
        let flags = WatchFlags::CREATE
            | WatchFlags::DELETE
            | WatchFlags::MOVED_FROM
            | WatchFlags::MOVED_TO
            | WatchFlags::DELETE_SELF
            | WatchFlags::MOVE_SELF
            | WatchFlags::DONT_FOLLOW
            | WatchFlags::EXCL_UNLINK;
        match add_watch(&self.fd, dir, flags) {
            Ok(wd) => {
                // A renamed folder keeps its inode, so the kernel can hand
                // back a descriptor we know under the old path.
                if let Some(old) = self.by_wd.insert(wd, dir.to_path_buf()) {
                    self.by_dir.remove(&old);
                }
                self.by_dir.insert(dir.to_path_buf(), wd);
                true
            }
            Err(_) => false,
        }
    }

    pub(crate) fn unwatch(&mut self, dir: &Path) {
        if let Some(wd) = self.by_dir.remove(dir) {
            self.by_wd.remove(&wd);
            let _ = rustix::fs::inotify::remove_watch(&self.fd, wd);
        }
    }

    /// Wait up to `timeout` (forever with `None`) and translate what arrived.
    pub(crate) fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<Vec<WatchEvent>> {
        use rustix::event::{PollFd, PollFlags, Timespec, poll};
        use rustix::fs::inotify::{ReadFlags, Reader};

        let timespec = timeout.map(|timeout| Timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: i64::from(timeout.subsec_nanos()),
        });
        let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
        match poll(&mut fds, timespec.as_ref()) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }

        let mut events = Vec::new();
        let mut buffer = [std::mem::MaybeUninit::<u8>::uninit(); 8192];
        let mut reader = Reader::new(&self.fd, &mut buffer);
        loop {
            let event = match reader.next() {
                Ok(event) => event,
                Err(rustix::io::Errno::AGAIN) => break,
                Err(error) => return Err(error.into()),
            };
            let flags = event.events();
            if flags.contains(ReadFlags::QUEUE_OVERFLOW) {
                events.push(WatchEvent::Overflow);
                continue;
            }
            let Some(dir) = self.by_wd.get(&event.wd()).cloned() else {
                continue;
            };
            if flags.contains(ReadFlags::IGNORED) {
                self.by_wd.remove(&event.wd());
                self.by_dir.remove(&dir);
                continue;
            }
            if flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF) {
                if let Some(parent) = dir.parent() {
                    events.push(WatchEvent::Dirty(parent.to_path_buf()));
                }
                continue;
            }
            events.push(WatchEvent::Dirty(dir));
        }
        Ok(events)
    }
}

/// The index for `config_dir`: the stored rows when there are any (and the
/// folders that changed since), a full walk otherwise. Saved either way.
pub(crate) fn load_or_build(
    config_dir: &Path,
    roots: Vec<PathBuf>,
    excludes: Excludes,
) -> FileIndex {
    let rows = storage::files_load(config_dir);
    if rows.is_empty() {
        let index = FileIndex::build(roots, excludes);
        save(config_dir, &index, &Changes::default(), true);
        return index;
    }
    let mut index = FileIndex::from_rows(roots, excludes, rows);
    let stale = index.stale_dirs();
    let changes = index.rescan_all(stale);
    save(config_dir, &index, &changes, false);
    index
}

fn save(config_dir: &Path, index: &FileIndex, changes: &Changes, replace: bool) {
    let rows = if replace {
        index.all_rows()
    } else {
        index.rows(&changes.upserted)
    };
    if let Err(error) = storage::files_apply(config_dir, &rows, &changes.removed, replace) {
        log::warn!("could not save the file index: {error}");
    }
}

/// Receives each new version of the index; `false` stops the watcher.
pub(crate) type Publish = Box<dyn Fn(Arc<Vec<FileEntry>>) -> bool + Send>;

/// Keep `index` in step with the disk for as long as `publish` accepts
/// snapshots (it returns `false` once nobody listens). Runs on its own thread.
pub(crate) fn watch_forever(
    config_dir: &Path,
    mut index: FileIndex,
    publish: impl Fn(Arc<Vec<FileEntry>>) -> bool,
) {
    let mut watcher = match Watcher::new() {
        Ok(watcher) => watcher,
        Err(error) => {
            log::warn!("file index: no inotify ({error}); new files appear after a restart");
            return;
        }
    };
    for dir in index.watch_list() {
        if !watcher.watch(&dir) {
            break;
        }
    }
    log::info!(
        "file index: {} entries, {} folders watched",
        index.len(),
        watcher.len()
    );

    let mut debouncer = Debouncer::default();
    loop {
        let events = match watcher.wait(debouncer.wait_for(Instant::now())) {
            Ok(events) => events,
            Err(error) => {
                log::warn!("file index: inotify failed ({error}); live updates stop");
                return;
            }
        };
        let now = Instant::now();
        for event in events {
            match event {
                WatchEvent::Dirty(dir) => debouncer.push(dir, now),
                WatchEvent::Overflow => {
                    for dir in index.stale_dirs() {
                        debouncer.push(dir, now);
                    }
                }
            }
        }
        let Some(dirs) = debouncer.take(Instant::now()) else {
            continue;
        };
        let changes = index.rescan_all(dirs);
        if changes.is_empty() {
            continue;
        }
        for dir in &changes.dirs_removed {
            watcher.unwatch(dir);
        }
        for dir in &changes.dirs_added {
            watcher.watch(dir);
        }
        save(config_dir, &index, &changes, false);
        if !publish(Arc::new(index.snapshot())) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("zeshicast-index-{name}-"))
            .tempdir()
            .unwrap()
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "").unwrap();
    }

    fn names(index: &FileIndex) -> Vec<String> {
        let mut names: Vec<_> = index.snapshot().into_iter().map(|file| file.name).collect();
        names.sort();
        names
    }

    fn index_of(home: &Path) -> FileIndex {
        FileIndex::build(
            vec![home.join("Downloads"), home.to_path_buf()],
            Excludes::default(),
        )
    }

    #[test]
    fn stored_rows_are_judged_like_live_paths() {
        let home = Path::new("/home/u");
        let index = FileIndex {
            gitignores: GitIgnores::new(home),
            roots: vec![home.join("Downloads"), home.to_path_buf()],
            excludes: Excludes::parse("~/Documents/secret, build", home),
            limit: LIVE_LIMIT,
            entries: HashMap::new(),
        };
        let paths = [
            "/home/u",
            "/home/u/Downloads",
            "/home/u/Downloads/a.pdf",
            "/home/u/a.txt",
            "/home/u/.config/x",
            "/home/u/Code/.git/HEAD",
            "/home/u/Code/node_modules/x/y.js",
            "/home/u/Documents/secret/k.txt",
            "/home/u/Documents/secretary.txt",
            "/home/u/Code/build/out.o",
            "/home/u/1/2/3/4/5",
            "/home/u/1/2/3/4/5/6",
            "/home/u/1/2/3/4/5/6/7",
            "/home/u/Downloads/1/2/3/4/5/6",
            "/home/u/Downloads/1/2/3/4/5/6/7",
            "/etc/passwd",
            "/",
        ];
        let mut folders = HashMap::new();
        for path in paths {
            let path = Path::new(path);
            assert_eq!(
                index.keeps_in(path, false, &mut folders),
                index.keeps(path, false),
                "{}",
                path.display()
            );
        }
    }

    #[test]
    fn what_git_ignores_stays_out_of_the_index() {
        let home = temp("gitignore");
        let home = home.path();
        let repo = home.join("Code/app");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join(".gitignore"), "dist/\n").unwrap();
        touch(&repo.join("dist/bundle.js"));
        touch(&repo.join("src/main.rs"));
        let mut index = index_of(home);
        let found = names(&index);
        assert!(found.contains(&"main.rs".to_string()), "{found:?}");
        assert!(!found.contains(&"dist".to_string()), "{found:?}");
        assert!(!found.contains(&"bundle.js".to_string()), "{found:?}");

        // Live: a new ignored file never comes in; a new rule applies.
        fs::write(repo.join(".gitignore"), "dist/\n*.log\n").unwrap();
        touch(&repo.join("src/debug.log"));
        index.rescan_all([repo.join("src")]);
        assert!(!names(&index).contains(&"debug.log".to_string()));

        // Stored rows from before a rule are dropped on the next start.
        let rows = index.all_rows();
        fs::write(repo.join(".gitignore"), "dist/\n*.log\nsrc/\n").unwrap();
        let reloaded = FileIndex::from_rows(index.roots.clone(), Excludes::default(), rows);
        assert!(!names(&reloaded).contains(&"main.rs".to_string()));
    }

    #[test]
    fn create_delete_and_rename_update_the_index() {
        let home = temp("live");
        let home = home.path();
        touch(&home.join("Downloads/old.pdf"));
        let mut index = index_of(home);
        assert_eq!(names(&index), vec!["Downloads", "old.pdf"]);

        touch(&home.join("Downloads/invoice.pdf"));
        let changes = index.rescan_dir(&home.join("Downloads"));
        assert_eq!(
            changes.upserted.last(),
            Some(&home.join("Downloads/invoice.pdf"))
        );
        assert!(names(&index).contains(&"invoice.pdf".to_string()));

        fs::remove_file(home.join("Downloads/old.pdf")).unwrap();
        let changes = index.rescan_dir(&home.join("Downloads"));
        assert_eq!(changes.removed, vec![home.join("Downloads/old.pdf")]);

        // A folder moved in with contents: its files come in too.
        touch(&home.join("staging/report/q3.xlsx"));
        fs::rename(home.join("staging/report"), home.join("Downloads/report")).unwrap();
        let changes = index.rescan_all([home.join("Downloads"), home.join("staging")]);
        assert!(changes.dirs_added.contains(&home.join("Downloads/report")));
        assert!(names(&index).contains(&"q3.xlsx".to_string()));

        // Renamed away: the old path and everything below it leave.
        fs::rename(
            home.join("Downloads/report"),
            home.join("Downloads/report-final"),
        )
        .unwrap();
        let changes = index.rescan_dir(&home.join("Downloads"));
        assert!(
            changes
                .removed
                .contains(&home.join("Downloads/report/q3.xlsx"))
        );
        assert!(
            changes
                .dirs_removed
                .contains(&home.join("Downloads/report"))
        );
        assert!(
            index
                .entries
                .contains_key(&home.join("Downloads/report-final/q3.xlsx"))
        );
    }

    #[test]
    fn the_debounce_waits_for_quiet_but_not_forever() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut debouncer = Debouncer::new(ms(400), ms(3000));
        assert_eq!(debouncer.wait_for(start), None);
        assert_eq!(debouncer.take(start), None);

        debouncer.push("/a".into(), start);
        debouncer.push("/a".into(), start + ms(100));
        debouncer.push("/b".into(), start + ms(300));
        assert_eq!(debouncer.wait_for(start + ms(300)), Some(ms(400)));
        assert_eq!(
            debouncer.take(start + ms(600)),
            None,
            "still within 400 ms of /b"
        );
        assert_eq!(
            debouncer.take(start + ms(700)),
            Some(vec![PathBuf::from("/a"), PathBuf::from("/b")]),
            "one batch, each folder once"
        );
        assert_eq!(debouncer.wait_for(start + ms(700)), None);

        // An event every 300 ms never goes quiet; the batch comes at 3 s.
        let t0 = start + ms(10_000);
        for step in 0..12 {
            debouncer.push("/busy".into(), t0 + ms(300 * step));
            if step < 10 {
                assert_eq!(debouncer.take(t0 + ms(300 * step)), None);
            }
        }
        assert!(debouncer.take(t0 + ms(3000)).is_some());
    }

    #[test]
    fn limits_depth_and_excludes_hold_for_live_additions() {
        let home = temp("limits");
        let home = home.path();
        touch(&home.join("a.txt"));
        let excludes = Excludes::parse("Videos, ~/vm", home);
        let mut index = FileIndex::build(vec![home.to_path_buf()], excludes).with_limit(3);

        touch(&home.join("Videos/big.mkv"));
        touch(&home.join("vm/disk.img"));
        touch(&home.join("node_modules/x.js"));
        touch(&home.join("b.txt"));
        touch(&home.join("c.txt"));
        touch(&home.join("d.txt"));
        index.rescan_dir(home);
        let indexed = names(&index);
        assert_eq!(indexed.len(), 3, "capped: {indexed:?}");
        assert!(
            !indexed
                .iter()
                .any(|n| n == "big.mkv" || n == "Videos" || n == "vm")
        );

        let deep = home.join("1/2/3/4/5/6/7/deep.txt");
        touch(&deep);
        let mut index = FileIndex::build(vec![home.to_path_buf()], Excludes::default());
        index.rescan_dir(home);
        assert!(index.entries.contains_key(&home.join("1/2/3/4/5/6")));
        assert!(!index.entries.contains_key(&deep), "below MAX_FILE_DEPTH");
    }

    #[test]
    fn a_restart_reads_the_stored_index_and_rescans_only_changed_folders() {
        let home = temp("restart");
        let config = temp("restart-config");
        let home = home.path();
        touch(&home.join("Downloads/a.pdf"));
        touch(&home.join("Documents/notes/b.md"));
        let roots = vec![home.join("Downloads"), home.to_path_buf()];

        let first = load_or_build(config.path(), roots.clone(), Excludes::default());
        let stored = storage::files_load(config.path());
        assert_eq!(stored.len(), first.len());

        // Nothing changed: only the roots are re-read, nothing is rebuilt.
        let index = FileIndex::from_rows(roots.clone(), Excludes::default(), stored);
        assert_eq!(index.stale_dirs(), roots);

        // A file saved while the daemon was down is found on the next start.
        std::thread::sleep(Duration::from_millis(20));
        touch(&home.join("Documents/notes/c.md"));
        let index = FileIndex::from_rows(
            roots.clone(),
            Excludes::default(),
            storage::files_load(config.path()),
        );
        assert!(index.stale_dirs().contains(&home.join("Documents/notes")));
        let second = load_or_build(config.path(), roots, Excludes::default());
        assert!(names(&second).contains(&"c.md".to_string()));
        assert_eq!(storage::files_load(config.path()).len(), second.len());
    }

    #[test]
    fn a_newly_excluded_folder_leaves_the_stored_index() {
        let home = temp("exclude-later");
        let home = home.path();
        let rows = vec![
            FileRow {
                path: home.join("Videos"),
                is_dir: true,
                mtime: 0,
            },
            FileRow {
                path: home.join("Videos/a.mkv"),
                is_dir: false,
                mtime: 0,
            },
            FileRow {
                path: home.join("notes.md"),
                is_dir: false,
                mtime: 0,
            },
            FileRow {
                path: "/etc/passwd".into(),
                is_dir: false,
                mtime: 0,
            },
        ];
        let index = FileIndex::from_rows(
            vec![home.to_path_buf()],
            Excludes::parse("Videos", home),
            rows,
        );
        assert_eq!(names(&index), vec!["notes.md"]);
    }

    #[test]
    fn a_running_index_finds_a_new_file_without_a_restart() {
        let home = temp("live-loop");
        let config = temp("live-loop-config");
        let downloads = home.path().join("Downloads");
        fs::create_dir_all(&downloads).unwrap();
        let roots = vec![downloads.clone(), home.path().to_path_buf()];
        let index = load_or_build(config.path(), roots, Excludes::default());

        let (sender, receiver) = std::sync::mpsc::channel();
        let config_dir = config.path().to_path_buf();
        std::thread::spawn(move || {
            watch_forever(
                &config_dir,
                index,
                Box::new(move |files| sender.send(files).is_ok()),
            )
        });
        // Let the watches be set up before the file appears.
        std::thread::sleep(Duration::from_millis(200));
        touch(&downloads.join("invoice.pdf"));

        let files = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("a new snapshot after the debounce");
        assert!(files.iter().any(|file| file.name == "invoice.pdf"));
        assert!(
            storage::files_load(config.path())
                .iter()
                .any(|row| row.path == downloads.join("invoice.pdf")),
            "saved, so a restart finds it too"
        );
    }

    #[test]
    fn the_watcher_reports_changes_in_watched_folders() {
        let home = temp("inotify");
        let dir = home.path().join("Downloads");
        fs::create_dir_all(&dir).unwrap();
        let mut watcher = Watcher::new().unwrap();
        assert!(watcher.watch(&dir));
        assert!(watcher.wait(Some(Duration::ZERO)).unwrap().is_empty());

        touch(&dir.join("new.pdf"));
        let events = watcher.wait(Some(Duration::from_secs(2))).unwrap();
        assert_eq!(events, vec![WatchEvent::Dirty(dir.clone())]);

        fs::remove_dir_all(&dir).unwrap();
        let events = watcher.wait(Some(Duration::from_secs(2))).unwrap();
        assert!(
            events.contains(&WatchEvent::Dirty(home.path().to_path_buf())),
            "{events:?}"
        );
    }
}
