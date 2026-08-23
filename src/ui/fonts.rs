//! Self-contained font provisioning.
//!
//! Outfit and JetBrains Mono are embedded in the binary and written to the
//! user font directory on first launch, then registered with fontconfig via
//! `fc-cache`. This keeps the launcher visually correct on systems (e.g. a
//! fresh NixOS) where those families are not installed system-wide.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

use crate::home_dir;

const OUTFIT: &[u8] = include_bytes!("../resources/fonts/Outfit.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../resources/fonts/JetBrainsMono.ttf");

const BUNDLED: &[(&str, &[u8])] = &[
    ("Outfit.ttf", OUTFIT),
    ("JetBrainsMono.ttf", JETBRAINS_MONO),
];

fn font_dir() -> PathBuf {
    home_dir().join(".local/share/fonts/zeshicast")
}

/// A bundled font is already installed when an existing file has exactly the
/// embedded byte length; anything else (missing, stale, partial) needs a write.
fn font_is_current(existing_len: Option<u64>, embedded_len: u64) -> bool {
    existing_len == Some(embedded_len)
}

/// Write the embedded fonts to the user font directory if missing or stale,
/// and refresh the fontconfig cache so the families resolve in this process.
pub fn ensure_fonts() {
    let dir = font_dir();
    let mut wrote = false;

    for (name, bytes) in BUNDLED {
        let path = dir.join(name);
        let existing_len = fs::metadata(&path).map(|meta| meta.len()).ok();
        if font_is_current(existing_len, bytes.len() as u64) {
            continue;
        }
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        if fs::write(&path, bytes).is_ok() {
            wrote = true;
        }
    }

    if wrote {
        // Refresh fontconfig for this directory so the new families become
        // discoverable. `fc-cache -f` can take seconds, so it must never run
        // on the startup thread; fonts land on disk synchronously above and
        // the cache refresh completes asynchronously.
        thread::spawn(move || {
            let _ = Command::new("fc-cache").arg("-f").arg(&dir).status();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::font_is_current;

    #[test]
    fn current_when_existing_file_matches_embedded_length() {
        assert!(font_is_current(Some(1024), 1024));
    }

    #[test]
    fn not_current_when_file_missing() {
        assert!(!font_is_current(None, 1024));
    }

    #[test]
    fn not_current_when_file_stale_or_partial() {
        assert!(!font_is_current(Some(512), 1024));
        assert!(!font_is_current(Some(2048), 1024));
    }
}
