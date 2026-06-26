use std::fs;
use std::path::Path;

use crate::{
    Action, ActionKind, ActionRisk, MAX_RESULTS, ShellCommand, clipboard_preview, fuzzy_score,
};

#[derive(Debug, Clone)]
pub(crate) struct ProcessEntry {
    pub(crate) pid: u32,
    pub(crate) name: String,
    pub(crate) command: String,
}

pub(crate) fn search_processes(query: &str) -> Vec<Action> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    search_process_entries(&load_process_entries(), query)
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

fn load_process_entry(pid: u32, path: &Path) -> Option<ProcessEntry> {
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
