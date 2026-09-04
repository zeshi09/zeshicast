use std::cell::RefCell;
use std::rc::Rc;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use crate::Zeshicast;

pub(crate) fn install_clipboard_monitor(launcher: &Rc<RefCell<Zeshicast>>) {
    if !launcher.borrow().clipboard_history_enabled() {
        return;
    }

    let Some(display) = gdk::Display::default() else {
        return;
    };

    let clipboard = display.clipboard();
    let last = Rc::new(RefCell::new(None::<String>));
    let launcher = Rc::clone(launcher);

    capture_clipboard(&clipboard, &launcher, &last);

    clipboard.connect_changed(move |clipboard| {
        capture_clipboard(clipboard, &launcher, &last);
    });
}

/// Background clipboard capture via `wl-paste --watch`. The gdk
/// `connect_changed` monitor only fires while the launcher window is focused —
/// on Wayland a client receives clipboard events only when focused — so
/// everything copied while the launcher is hidden collapses to just the latest
/// value on next focus, and rapid copies race (the async read sees the newest
/// content). `wl-paste --watch` fires for *every* change in the background.
/// Text only; image copies stay on the gdk path.
pub(crate) fn install_clipboard_background_watcher(launcher: &Rc<RefCell<Zeshicast>>) {
    if !launcher.borrow().clipboard_history_enabled() {
        return;
    }

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || watch_clipboard_text(tx));

    let launcher = Rc::clone(launcher);
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        while let Ok(text) = rx.try_recv() {
            if let Err(error) = launcher.borrow_mut().add_clipboard_text(&text) {
                eprintln!("failed to save clipboard history: {error}");
            }
        }
        glib::ControlFlow::Continue
    });
}

fn watch_clipboard_text(tx: std::sync::mpsc::Sender<String>) {
    watch_clipboard_text_with(tx, || {
        std::process::Command::new("wl-paste")
            .args(["--watch", "sh", "-c", "cat; printf '\\0'"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
    })
}

pub(crate) fn watch_clipboard_text_with(
    tx: std::sync::mpsc::Sender<String>,
    mut spawn: impl FnMut() -> std::io::Result<std::process::Child>,
) {
    use std::io::BufRead;
    use std::io::Read;
    /// Hard cap on bytes read per clipboard record: `MAX_CLIPBOARD_TEXT_BYTES`
    /// plus slack for the NUL terminator, so a multi-gigabyte paste cannot
    /// balloon the read buffer.
    const CLIPBOARD_RECORD_READ_LIMIT: u64 =
        (crate::search::clipboard::MAX_CLIPBOARD_TEXT_BYTES + 4096) as u64;
    loop {
        // Each selection change runs the command with the new content on stdin;
        // we frame entries with a trailing NUL (clipboard text never contains
        // one) so multi-line values stay intact.
        let mut child = match spawn() {
            Ok(child) => child,
            // wl-clipboard not installed — leave the gdk monitor as the only path.
            Err(_) => return,
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };

        let record_truncated;
        {
            let mut reader = std::io::BufReader::new(stdout).take(CLIPBOARD_RECORD_READ_LIMIT);
            let mut buf: Vec<u8> = Vec::new();
            let mut truncated = false;
            loop {
                buf.clear();
                match reader.read_until(0, &mut buf) {
                    Ok(0) => break, // producer exited or the read cap was exhausted
                    Ok(_) => {
                        let terminated = buf.last() == Some(&0);
                        if terminated {
                            buf.pop();
                        }
                        // A record clipped by the read cap can end mid-UTF-8
                        // character; trim to the last valid boundary so it decodes
                        // like any other entry instead of being dropped.
                        let valid_len = match std::str::from_utf8(&buf) {
                            Ok(_) => buf.len(),
                            Err(error) => error.valid_up_to(),
                        };
                        if let Some(text) = decode_clipboard_text(&buf[..valid_len])
                            && tx.send(text).is_err()
                        {
                            return; // receiver gone, stop the thread
                        }
                        if !terminated {
                            truncated = true;
                            break; // oversized record: nothing else in this batch
                        }
                    }
                    Err(_) => break,
                }
            }
            record_truncated = truncated;
        } // reader dropped here: our pipe end closes before we reap the child
        if record_truncated {
            // Log the event only; never echo clipboard content here.
            eprintln!(
                "clipboard record exceeded {CLIPBOARD_RECORD_READ_LIMIT} bytes; truncated"
            );
        }
        // Kill before reaping: an oversized record leaves the producer blocked
        // writing into a pipe nobody drains any more, so a plain wait() would
        // hang forever and silently kill this watcher thread. kill() is also
        // harmless when the child already exited on its own.
        let _ = child.kill();
        let _ = child.wait();
        // wl-paste died (e.g. compositor restart); reconnect shortly, unless the
        // receiver is gone (empty probe doubles as a liveness check).
        std::thread::sleep(std::time::Duration::from_secs(1));
        if tx.send(String::new()).is_err() {
            return;
        }
    }
}

/// Accept a clipboard chunk only if it's valid UTF-8 text without binary control
/// characters — filters out image/binary fragments `wl-paste` delivers for
/// non-text content.
pub(crate) fn decode_clipboard_text(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return None;
    }
    Some(text.to_string())
}

/// Dispatch a clipboard change to image or text capture. Image copies rarely
/// carry a usable text/plain fallback, so an image, when present, wins.
fn capture_clipboard(
    clipboard: &gdk::Clipboard,
    launcher: &Rc<RefCell<Zeshicast>>,
    last: &Rc<RefCell<Option<String>>>,
) {
    if !launcher.borrow().clipboard_history_enabled() || launcher.borrow().clipboard_private_mode()
    {
        return;
    }

    let formats = clipboard.formats();
    if formats.contain_mime_type("image/png") || formats.contains_type(gdk::Texture::static_type())
    {
        capture_clipboard_image(clipboard, launcher, last);
    } else {
        capture_clipboard_text(clipboard, launcher, last);
    }
}

fn capture_clipboard_image(
    clipboard: &gdk::Clipboard,
    launcher: &Rc<RefCell<Zeshicast>>,
    last: &Rc<RefCell<Option<String>>>,
) {
    use std::hash::{Hash, Hasher};
    if !launcher.borrow().clipboard_capture_images() {
        return;
    }

    let launcher = Rc::clone(launcher);
    let last = Rc::clone(last);
    clipboard.read_texture_async(gio::Cancellable::NONE, move |result| {
        let Ok(Some(texture)) = result else {
            return;
        };
        let png = texture.save_to_png_bytes();
        let bytes: &[u8] = &png;

        // Content-addressed cache file so identical images dedupe naturally.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let dir = crate::clipboard_cache_dir();
        let path = dir.join(format!("{:016x}.png", hasher.finish()));
        if !path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if std::fs::create_dir_all(&dir).is_err()
                    || std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                        .is_err()
                {
                    return;
                }
            }
            #[cfg(not(unix))]
            if std::fs::create_dir_all(&dir).is_err() {
                return;
            }
            // Same 0o600 contract as other persisted files (config.rs
            // write_file_atomic): cached clipboard images may hold secrets.
            if crate::write_file_atomic(&path, bytes, 0o600).is_err() {
                return;
            }
        }

        let path_str = path.to_string_lossy().into_owned();
        let value = format!("{}{}", crate::CLIPBOARD_IMAGE_PREFIX, path_str);
        if last.borrow().as_deref() == Some(value.as_str()) {
            return;
        }
        *last.borrow_mut() = Some(value);
        if let Err(error) = launcher.borrow_mut().add_clipboard_image(&path_str) {
            eprintln!("failed to save clipboard image: {error}");
        }
    });
}

fn capture_clipboard_text(
    clipboard: &gdk::Clipboard,
    launcher: &Rc<RefCell<Zeshicast>>,
    last_text: &Rc<RefCell<Option<String>>>,
) {
    let launcher = Rc::clone(launcher);
    let last_text = Rc::clone(last_text);
    clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
        let Ok(Some(text)) = result else {
            return;
        };

        let text = text.to_string();
        if last_text.borrow().as_deref() == Some(text.as_str()) {
            return;
        }

        *last_text.borrow_mut() = Some(text.clone());
        if let Err(error) = launcher.borrow_mut().add_clipboard_text(&text) {
            eprintln!("failed to save clipboard history: {error}");
        }
    });
}
