use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{home_dir, MAX_CLIPBOARD_ENTRIES};

/// Sentinel prefix marking a clipboard history entry as an image. The rest of
/// the stored value is the path to the cached PNG. The leading SOH control char
/// keeps it from colliding with any real copied text.
pub const CLIPBOARD_IMAGE_PREFIX: &str = "\u{1}zeshicast-image:";

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClipboardItem {
    Text(String),
    Image(PathBuf),
}

#[allow(dead_code)]
impl ClipboardItem {
    pub fn parse(raw: &str) -> Self {
        if let Some(path) = clipboard_image_path(raw) {
            Self::Image(PathBuf::from(path))
        } else {
            Self::Text(raw.to_string())
        }
    }

    pub fn to_raw(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Image(path) => format!("{CLIPBOARD_IMAGE_PREFIX}{}", path.display()),
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    pub fn image_path(&self) -> Option<&Path> {
        match self {
            Self::Image(path) => Some(path.as_path()),
            Self::Text(_) => None,
        }
    }
}

/// If `value` is an image entry, return the cached PNG path.
pub fn clipboard_image_path(value: &str) -> Option<&str> {
    value.strip_prefix(CLIPBOARD_IMAGE_PREFIX)
}

pub fn clipboard_cache_dir() -> PathBuf {
    home_dir().join(".cache/zeshicast/clipboard")
}

pub fn clipboard_retention_value(preferences: &HashMap<String, String>) -> usize {
    preferences
        .get("clipboard_retention")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(MAX_CLIPBOARD_ENTRIES)
        .clamp(1, 1000)
}

fn referenced_clipboard_image_paths(entries: &[String]) -> HashSet<PathBuf> {
    entries
        .iter()
        .filter_map(|entry| clipboard_image_path(entry).map(PathBuf::from))
        .collect()
}

fn is_png_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

pub fn prune_clipboard_image_cache_dir(
    cache_dir: &Path,
    entries: &[String],
) -> io::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    let referenced = referenced_clipboard_image_paths(entries);
    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if is_png_file(&path) && !referenced.contains(&path) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn prune_clipboard_image_cache(entries: &[String]) -> io::Result<()> {
    prune_clipboard_image_cache_dir(&clipboard_cache_dir(), entries)
}

pub fn clear_clipboard_image_cache_dir(cache_dir: &Path) -> io::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if is_png_file(&path) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn clear_clipboard_image_cache() -> io::Result<()> {
    clear_clipboard_image_cache_dir(&clipboard_cache_dir())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Url,
    Command,
    Code,
    Image,
}

impl ClipboardKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Url => "URL",
            Self::Command => "Command",
            Self::Code => "Code",
            Self::Image => "Image",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Text => "insert-text-symbolic",
            Self::Url => "emblem-shared-symbolic",
            Self::Command => "utilities-terminal-symbolic",
            Self::Code => "applications-engineering-symbolic",
            Self::Image => "image-x-generic-symbolic",
        }
    }

    pub fn mime_hint(self) -> &'static str {
        match self {
            Self::Text => "text/plain;charset=utf-8",
            Self::Url => "text/uri-list",
            Self::Command | Self::Code => "text/plain;charset=utf-8",
            Self::Image => "image/png",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardSummary {
    pub preview: String,
    pub value: String,
    pub kind: ClipboardKind,
    pub size_bytes: usize,
    pub timestamp: Option<i64>,
}

pub fn classify_clipboard_text(text: &str) -> ClipboardKind {
    if text.starts_with(CLIPBOARD_IMAGE_PREFIX) {
        return ClipboardKind::Image;
    }
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    let first_word = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('$');

    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("file://")
    {
        return ClipboardKind::Url;
    }

    let is_single_line = !trimmed.contains('\n');
    let known_commands = [
        "cargo",
        "cd",
        "codex",
        "docker",
        "git",
        "grep",
        "ls",
        "make",
        "nix",
        "npm",
        "pnpm",
        "rg",
        "sudo",
        "systemctl",
        "vim",
        "yarn",
    ];
    if is_single_line
        && (trimmed.starts_with("$ ") || known_commands.contains(&first_word))
        && trimmed.split_whitespace().count() > 1
    {
        return ClipboardKind::Command;
    }

    let code_markers = [
        "fn ",
        "let ",
        "use ",
        "pub ",
        "impl ",
        "match ",
        "struct ",
        "enum ",
        "mod ",
        "import ",
        "export ",
        "const ",
        "class ",
        "function ",
        "#[",
        "//",
        "/*",
        "*/",
        "&mut",
        "=>",
        "::",
        "</",
        "{",
        "};",
    ];
    if trimmed.lines().count() > 1 || code_markers.iter().any(|marker| lower.contains(marker)) {
        return ClipboardKind::Code;
    }

    ClipboardKind::Text
}
