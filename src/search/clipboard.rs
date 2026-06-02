use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::{Action, ActionKind, MAX_RESULTS, fuzzy_score};

pub(crate) const MAX_CLIPBOARD_ENTRIES: usize = 100;
const MAX_CLIPBOARD_TEXT_BYTES: usize = 20_000;

pub(crate) fn load_clipboard_history(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(decode_clipboard_line)
        .collect()
}

pub(crate) fn write_clipboard_history(path: &Path, entries: &[String]) -> io::Result<()> {
    let lines = entries
        .iter()
        .map(|entry| encode_clipboard_line(entry))
        .collect::<Vec<_>>();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    Ok(())
}

pub(crate) fn normalize_clipboard_text(text: &str) -> String {
    let text = text.replace('\0', "");
    if text.len() <= MAX_CLIPBOARD_TEXT_BYTES {
        return text.trim().to_string();
    }

    let mut truncated = String::new();
    for ch in text.chars() {
        if truncated.len() + ch.len_utf8() > MAX_CLIPBOARD_TEXT_BYTES {
            break;
        }
        truncated.push(ch);
    }
    truncated.trim().to_string()
}

pub(crate) fn clipboard_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 90 {
        collapsed
    } else {
        let mut preview = collapsed.chars().take(87).collect::<String>();
        preview.push_str("...");
        preview
    }
}

pub(crate) fn encode_clipboard_line(text: &str) -> String {
    let mut encoded = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

pub(crate) fn decode_clipboard_line(line: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => decoded.push('\\'),
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some(other) => {
                decoded.push('\\');
                decoded.push(other);
            }
            None => decoded.push('\\'),
        }
    }

    let normalized = normalize_clipboard_text(&decoded);
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn search_clipboard(entries: &[String], query: &str, explicit: bool) -> Vec<Action> {
    if query.is_empty() {
        return entries
            .iter()
            .take(6)
            .enumerate()
            .map(|(index, entry)| {
                Action::new(
                    "Clipboard",
                    clipboard_preview(entry),
                    ActionKind::Copy(entry.clone()),
                    220 - index as i32,
                )
                .with_subtitle("Copy clipboard history item")
                .with_icon("edit-paste-symbolic")
            })
            .collect();
    }

    let mut matches = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let score = fuzzy_score(entry, query)?;
            Some(
                Action::new(
                    "Clipboard",
                    clipboard_preview(entry),
                    ActionKind::Copy(entry.clone()),
                    score + if explicit { 120 } else { 35 } - index as i32,
                )
                .with_subtitle("Copy clipboard history item")
                .with_icon("edit-paste-symbolic"),
            )
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    matches.truncate(if explicit { MAX_RESULTS } else { 3 });
    matches
}
