use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{
    Action, ActionKind, ActionRisk, MAX_RESULTS, ShellCommand, clipboard_preview, fuzzy_score,
};

const PROCESS_CACHE_TTL: Duration = Duration::from_secs(2);

type ProcessLoader = fn() -> Vec<ProcessEntry>;

#[derive(Debug, Clone)]
pub(crate) struct ProcessEntry {
    pub(crate) pid: u32,
    pub(crate) name: String,
    pub(crate) command: String,
}

#[derive(Debug, Clone)]
struct ProcessSnapshot {
    entries: Vec<ProcessEntry>,
    captured_at: Instant,
}

fn process_snapshot_cache() -> &'static Mutex<Option<ProcessSnapshot>> {
    static CACHE: OnceLock<Mutex<Option<ProcessSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn search_processes(query: &str) -> Vec<Action> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    search_process_entries(&cached_process_entries(), query)
}

fn cached_process_entries() -> Vec<ProcessEntry> {
    let now = Instant::now();
    if let Some(cached) = process_snapshot_cache().lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .filter(|snapshot| now.duration_since(snapshot.captured_at) <= PROCESS_CACHE_TTL)
            .cloned()
    }) {
        return cached.entries;
    }

    let snapshot = ProcessSnapshot {
        entries: process_entry_loader()(),
        captured_at: Instant::now(),
    };
    if let Ok(mut cached) = process_snapshot_cache().lock() {
        *cached = Some(snapshot.clone());
    }
    snapshot.entries
}

// Test-only seam: lets cache tests count loads instead of reading /proc.
#[cfg(test)]
static TEST_LOADER_OVERRIDE: OnceLock<Mutex<Option<ProcessLoader>>> = OnceLock::new();

#[cfg(test)]
fn process_entry_loader() -> ProcessLoader {
    let guard = TEST_LOADER_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok();
    let loader = guard.as_deref().copied().flatten();
    loader.unwrap_or(load_process_entries)
}

#[cfg(not(test))]
fn process_entry_loader() -> ProcessLoader {
    load_process_entries
}

pub(crate) fn search_process_entries(processes: &[ProcessEntry], query: &str) -> Vec<Action> {
    let mut actions = processes
        .iter()
        .filter_map(|process| {
            let haystack = format!("{} {}", process.name, process.command);
            let score = fuzzy_score(&haystack, query)?;
            Some(process_action(process, score + 240))
        })
        .collect::<Vec<_>>();

    actions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    actions.truncate(MAX_RESULTS);
    actions
}

fn process_action(process: &ProcessEntry, score: i32) -> Action {
    Action::new(
        "Process",
        format!("Kill {} ({})", process.name, process.pid),
        ActionKind::Shell(ShellCommand::new(format!("kill {}", process.pid))),
        score,
    )
    .with_subtitle(process_subtitle(process))
    .with_icon("application-x-executable-symbolic")
    .with_risk(ActionRisk::ProcessKill)
}

fn process_subtitle(process: &ProcessEntry) -> String {
    if process.command.trim().is_empty() {
        format!("PID {}", process.pid)
    } else {
        format!(
            "PID {} - {}",
            process.pid,
            clipboard_preview(&process.command)
        )
    }
}

fn load_process_entries() -> Vec<ProcessEntry> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
            load_process_entry(pid, &entry.path())
        })
        .collect()
}

#[cfg(unix)]
fn is_current_user_process(path: &Path) -> bool {
    let current_uid = rustix::process::getuid().as_raw();
    if let Ok(stat) = rustix::fs::stat(path) {
        stat.st_uid == current_uid
    } else {
        false
    }
}

#[cfg(not(unix))]
fn is_current_user_process(_path: &Path) -> bool {
    true
}

fn load_process_entry(pid: u32, path: &Path) -> Option<ProcessEntry> {
    if !is_current_user_process(path) {
        return None;
    }
    let name = fs::read_to_string(path.join("comm"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let command = fs::read(path.join("cmdline"))
        .ok()
        .map(|value| decode_cmdline(&value))
        .unwrap_or_default();

    Some(ProcessEntry { pid, name, command })
}

pub(crate) fn decode_cmdline(value: &[u8]) -> String {
    value
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static LOADER_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn counting_test_loader() -> Vec<ProcessEntry> {
        LOADER_CALLS.fetch_add(1, Ordering::SeqCst);
        vec![ProcessEntry {
            pid: 4242,
            name: "zeshicast".to_string(),
            command: "target/debug/zeshicast-gtk --daemon".to_string(),
        }]
    }

    fn set_loader_override(loader: Option<fn() -> Vec<ProcessEntry>>) {
        *TEST_LOADER_OVERRIDE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap() = loader;
    }

    #[test]
    fn process_snapshot_is_reused_within_ttl_and_reload_after_expiry() {
        // Isolate from any snapshot left behind by a previous run.
        if let Ok(mut cached) = process_snapshot_cache().lock() {
            *cached = None;
        }
        set_loader_override(Some(counting_test_loader));
        LOADER_CALLS.store(0, Ordering::SeqCst);

        let first = search_processes("zesh");
        assert_eq!(first.len(), 1);
        assert_eq!(LOADER_CALLS.load(Ordering::SeqCst), 1);

        // Second query within TTL must reuse the cached snapshot.
        let second = search_processes("zesh");
        assert_eq!(second.len(), 1);
        assert_eq!(LOADER_CALLS.load(Ordering::SeqCst), 1);

        // Backdate the snapshot past the TTL: the next query reloads.
        if let Ok(mut cached) = process_snapshot_cache().lock()
            && let Some(snapshot) = cached.as_mut()
        {
            snapshot.captured_at = Instant::now() - PROCESS_CACHE_TTL - Duration::from_secs(1);
        }
        let third = search_processes("zesh");
        assert_eq!(third.len(), 1);
        assert_eq!(LOADER_CALLS.load(Ordering::SeqCst), 2);

        // Restore the real loader and drop the test snapshot.
        set_loader_override(None);
        if let Ok(mut cached) = process_snapshot_cache().lock() {
            *cached = None;
        }
    }
}
