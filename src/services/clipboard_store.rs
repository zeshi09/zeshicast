use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::config::home_dir;
use crate::search::clipboard::MAX_CLIPBOARD_ENTRIES;

/// Sentinel prefix marking a clipboard history entry as an image. The rest of
/// the stored value is the path to the cached PNG. The leading SOH control char
/// keeps it from colliding with any real copied text.
pub const CLIPBOARD_IMAGE_PREFIX: &str = "\u{1}zeshicast-image:";

/// If `value` is a *validated* image entry, return the cached PNG path.
///
/// A value that merely carries the sentinel prefix is **not** enough: the path
/// must be an existing `.png` file directly inside the clipboard cache dir, so
/// pasted text like `\x01zeshicast-image:/etc/passwd` stays text and can never
/// make the daemon read (or copy) an arbitrary file (M-15).
pub fn clipboard_image_path(value: &str) -> Option<&str> {
    let raw = value.strip_prefix(CLIPBOARD_IMAGE_PREFIX)?;
    validated_image_path(raw).map(|_| raw)
}

/// The single image validator. Returns the path only when it is safe to read.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub fn validated_clipboard_image(value: &str) -> Option<PathBuf> {
    let raw = value.strip_prefix(CLIPBOARD_IMAGE_PREFIX)?;
    validated_image_path(raw)
}

fn validated_image_path(raw: &str) -> Option<PathBuf> {
    let path = Path::new(raw);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    if !is_png_file(path) {
        return None;
    }
    if !is_directly_inside_clipboard_cache(path) {
        return None;
    }
    Some(path.to_path_buf())
}

/// `path` must resolve (symlinks included) to a file directly inside the
/// clipboard cache directory. Canonicalising both sides rejects symlinks that
/// point outside the cache.
fn is_directly_inside_clipboard_cache(path: &Path) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    let Ok(cache) = clipboard_cache_dir().canonicalize() else {
        return false;
    };
    canonical.parent() == Some(cache.as_path())
}

pub fn clipboard_cache_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = test_cache_dir_override() {
        return dir;
    }
    home_dir().join(".cache/zeshicast/clipboard")
}

#[cfg(test)]
thread_local! {
    static CACHE_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_cache_dir_override() -> Option<PathBuf> {
    CACHE_DIR_OVERRIDE.with(|slot| slot.borrow().clone())
}

/// Run `body` with [`clipboard_cache_dir`] pointing at `dir`, so tests can
/// exercise the real validation rules against a temp directory.
#[cfg(test)]
pub(crate) fn with_clipboard_cache_dir<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
    let previous = CACHE_DIR_OVERRIDE.with(|slot| slot.replace(Some(dir.to_path_buf())));
    let result = body();
    CACHE_DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = previous);
    result
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

pub fn prune_clipboard_image_cache_dir(cache_dir: &Path, entries: &[String]) -> io::Result<()> {
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
    // Only a validated path counts as an image; a forged prefix falls through
    // and is classified as ordinary text.
    if clipboard_image_path(text).is_some() {
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

/// Save a PNG byte slice to the cache directory as content-addressed `<hash>.png`.
/// Returns the absolute path string to the cached image.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub fn save_clipboard_image(bytes: &[u8]) -> io::Result<String> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let dir = clipboard_cache_dir();
    let path = dir.join(format!("{:016x}.png", hasher.finish()));
    if !path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) = fs::create_dir_all(&dir) {
                log::warn!("could not create {}: {error}", dir.display());
            } else if let Err(error) = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            {
                log::warn!("could not restrict {} to 0700: {error}", dir.display());
            }
        }
        #[cfg(not(unix))]
        if let Err(error) = fs::create_dir_all(&dir) {
            log::warn!("could not create {}: {error}", dir.display());
        }

        crate::config::write_file_atomic(&path, bytes, 0o600)?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Put a cached image back on the clipboard as `image/png` (wl-clipboard, with
/// an xclip fallback). Refuses any path outside the clipboard cache (M-15).
pub fn copy_clipboard_image(value: &str) -> bool {
    let Some(path) = validated_image_path(value) else {
        return false;
    };
    let spawned = fs::File::open(&path).ok().and_then(|file| {
        std::process::Command::new("wl-copy")
            .args(["--type", "image/png"])
            .stdin(std::process::Stdio::from(file))
            .spawn()
            .ok()
    });
    if let Some(mut child) = spawned {
        return child.wait().is_ok();
    }
    std::process::Command::new("xclip")
        .args([
            "-selection",
            "clipboard",
            "-t",
            "image/png",
            "-i",
            &path.to_string_lossy(),
        ])
        .spawn()
        .is_ok()
}
