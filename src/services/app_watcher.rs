use std::path::PathBuf;
use std::sync::mpsc;

/// Watch XDG application directories for .desktop file changes.
/// Returns a Receiver that yields () whenever a change is detected.
/// The watcher thread runs until the Sender is dropped.
pub fn start_app_watcher(dirs: Vec<PathBuf>) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::sync_channel::<()>(1);
    std::thread::spawn(move || watch_dirs(dirs, tx));
    rx
}

fn watch_dirs(dirs: Vec<PathBuf>, tx: mpsc::SyncSender<()>) {
    use inotify::{Inotify, WatchMask};

    let Ok(mut inotify) = Inotify::init() else {
        return;
    };

    let mask = WatchMask::CREATE
        | WatchMask::DELETE
        | WatchMask::MODIFY
        | WatchMask::MOVED_TO
        | WatchMask::MOVED_FROM;

    for dir in &dirs {
        if dir.exists() {
            let _ = inotify.watches().add(dir, mask);
        }
    }

    let mut buf = [0u8; 4096];
    loop {
        let Ok(events) = inotify.read_events_blocking(&mut buf) else {
            break;
        };
        let has_desktop = events.into_iter().any(|ev| {
            ev.name
                .map(|n| n.to_string_lossy().ends_with(".desktop"))
                .unwrap_or(false)
        });
        if has_desktop {
            // Use try_send to drop redundant signals while GTK is busy
            let _ = tx.try_send(());
        }
    }
}
