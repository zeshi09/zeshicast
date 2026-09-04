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

/// Background clipboard capture via `wl-paste --watch`.
/// Text copies use `--type text` so images never pollute text history with
/// raw binary fragments. Image copies use `--type image/png` and are streamed
/// directly into the content-addressed cache.
pub(crate) fn install_clipboard_background_watcher(launcher: &Rc<RefCell<Zeshicast>>) {
    if !launcher.borrow().clipboard_history_enabled() {
        return;
    }

    let (tx_text, rx_text) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || watch_clipboard_text(tx_text));

    let (tx_img, rx_img) = std::sync::mpsc::channel::<String>();
    if launcher.borrow().clipboard_capture_images() {
        std::thread::spawn(move || watch_clipboard_image(tx_img));
    }

    let launcher = Rc::clone(launcher);
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        while let Ok(text) = rx_text.try_recv() {
            if !text.is_empty()
                && let Err(error) = launcher.borrow_mut().add_clipboard_text(&text)
            {
                eprintln!("failed to save clipboard history: {error}");
            }
        }
        while let Ok(path) = rx_img.try_recv() {
            if !path.is_empty()
                && let Err(error) = launcher.borrow_mut().add_clipboard_image(&path)
            {
                eprintln!("failed to save clipboard image: {error}");
            }
        }
        glib::ControlFlow::Continue
    });
}

fn watch_clipboard_text(tx: std::sync::mpsc::Sender<String>) {
    watch_clipboard_text_with(tx, || {
        std::process::Command::new("wl-paste")
            .args(["--type", "text", "--watch", "sh", "-c", "cat; printf '\\0'"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
    })
}

fn watch_clipboard_image(tx: std::sync::mpsc::Sender<String>) {
    watch_clipboard_image_with(tx, || {
        std::process::Command::new("wl-paste")
            .args(["--type", "image/png", "--watch", "cat"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
    })
}

pub(crate) fn watch_clipboard_image_with(
    tx: std::sync::mpsc::Sender<String>,
    mut spawn: impl FnMut() -> std::io::Result<std::process::Child>,
) {
    loop {
        let mut child = match spawn() {
            Ok(child) => child,
            Err(_) => return,
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };

        let mut reader = std::io::BufReader::new(stdout);
        while let Ok(Some(png_bytes)) = read_png_from_stream(&mut reader) {
            if let Ok(path) = crate::save_clipboard_image(&png_bytes)
                && tx.send(path).is_err()
            {
                return;
            }
        }

        let _ = child.kill();
        let _ = child.wait();
        std::thread::sleep(std::time::Duration::from_secs(1));
        if tx.send(String::new()).is_err() {
            return;
        }
    }
}

pub(crate) fn read_png_from_stream<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut sig = [0u8; 8];
    match reader.read_exact(&mut sig) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if sig != PNG_SIG {
        return Ok(None);
    }

    let mut data = Vec::with_capacity(65536);
    data.extend_from_slice(&sig);

    const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024; // 50 MB safety cap

    loop {
        let mut chunk_header = [0u8; 8];
        match reader.read_exact(&mut chunk_header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let chunk_len = u32::from_be_bytes([
            chunk_header[0],
            chunk_header[1],
            chunk_header[2],
            chunk_header[3],
        ]) as usize;
        let is_iend = &chunk_header[4..8] == b"IEND";

        data.extend_from_slice(&chunk_header);

        if data.len() + chunk_len + 4 > MAX_IMAGE_BYTES {
            return Err(std::io::Error::other("image exceeded maximum size"));
        }

        let old_len = data.len();
        data.resize(old_len + chunk_len + 4, 0);
        reader.read_exact(&mut data[old_len..])?;

        if is_iend {
            break;
        }
    }

    Ok(Some(data))
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
    // Reject image/binary magic headers and PNG chunk identifiers as defense in depth
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"RIFF")
        || bytes == b"IHDR"
        || bytes == b"IEND"
    {
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
        let Ok(path_str) = crate::save_clipboard_image(png.as_ref()) else {
            return;
        };

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
