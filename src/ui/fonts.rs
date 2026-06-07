//! Self-contained font provisioning.
//!
//! Outfit and JetBrains Mono are embedded in the binary and written to the
//! user font directory on first launch, then registered with fontconfig via
//! `fc-cache`. This keeps the launcher visually correct on systems (e.g. a
//! fresh NixOS) where those families are not installed system-wide.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

/// Write the embedded fonts to the user font directory if missing or stale,
/// and refresh the fontconfig cache so the families resolve in this process.
pub fn ensure_fonts() {
    let dir = font_dir();
    let mut wrote = false;

    for (name, bytes) in BUNDLED {
        let path = dir.join(name);
        let up_to_date = fs::metadata(&path)
            .map(|meta| meta.len() == bytes.len() as u64)
            .unwrap_or(false);
        if up_to_date {
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
        // Refresh fontconfig for this directory so the new families are
        // discoverable immediately (and on every subsequent launch).
        let _ = Command::new("fc-cache")
            .arg("-f")
            .arg(&dir)
            .status();
    }
}
