use crate::app::Zeshicast;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether image capture is currently allowed. The `wl-paste` watcher runs on
/// its own thread and cannot borrow the launcher, so the main thread mirrors
/// [`Zeshicast::clipboard_capture_images`] here before the watcher writes a PNG.
static IMAGE_CAPTURE_ALLOWED: AtomicBool = AtomicBool::new(false);

fn image_capture_allowed() -> bool {
    IMAGE_CAPTURE_ALLOWED.load(Ordering::Relaxed)
}

fn set_image_capture_allowed(allowed: bool) {
    IMAGE_CAPTURE_ALLOWED.store(allowed, Ordering::Relaxed);
}

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

/// Put a clipboard entry back in its original format, offered next to its
/// plain text so a terminal still gets text (Q6.4). `wl-copy` offers one
/// type per call, gdk offers several. It must run while a zeshicast window
/// has keyboard focus: Wayland lets only the focused client set the
/// selection, and the selection stays ours after the window hides.
pub(crate) fn offer_rich_clipboard(mime: &str, data: &[u8], plain: &str) -> bool {
    let Some(display) = gdk::Display::default() else {
        return false;
    };
    let bytes = |data: &[u8]| glib::Bytes::from(data);
    let mut providers = vec![gdk::ContentProvider::for_bytes(mime, &bytes(data))];
    if mime == crate::services::clipboard_formats::URI_LIST {
        // What Nautilus and other GTK file managers paste from.
        let uris = String::from_utf8_lossy(data).replace("\r\n", "\n");
        let gnome = format!("copy\n{}", uris.trim_end());
        providers.push(gdk::ContentProvider::for_bytes(
            "x-special/gnome-copied-files",
            &bytes(gnome.as_bytes()),
        ));
    }
    providers.push(gdk::ContentProvider::for_bytes(
        "text/plain;charset=utf-8",
        &bytes(plain.as_bytes()),
    ));
    // The GType string provider adds text/plain, UTF8_STRING and friends.
    providers.push(gdk::ContentProvider::for_value(&plain.to_value()));
    let union = gdk::ContentProvider::new_union(&providers);
    let clipboard = display.clipboard();
    clipboard.set_content(Some(&union)).is_ok() && clipboard.is_local()
}

/// The comma separated `clipboard_ignore_apps`, mirrored here from the main
/// thread like [`IMAGE_CAPTURE_ALLOWED`], for the watcher thread to read.
static IGNORED_APPS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Set once `wl-paste --watch` runs: the gdk monitor then stays out of the
/// way, since it cannot tell which app a copy came from.
static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

fn set_ignored_apps(apps: Vec<String>) {
    if let Ok(mut ignored) = IGNORED_APPS.lock() {
        *ignored = apps;
    }
}

fn ignored_apps() -> Vec<String> {
    IGNORED_APPS
        .lock()
        .map(|apps| apps.clone())
        .unwrap_or_default()
}

/// One clipboard change worth keeping, as the watcher hands it over.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Captured {
    /// Text, with its original format when it had a richer one.
    Text {
        text: String,
        extra: Option<(&'static str, Vec<u8>)>,
    },
    /// PNG bytes, saved into the cache by the caller.
    Image(Vec<u8>),
}

/// How the watcher reads the clipboard; tests pass a fake.
pub(crate) trait ClipboardSource {
    /// The offered MIME types.
    fn types(&self) -> Vec<String>;
    /// The content as `mime`, at most `cap` bytes; `None` when it is not
    /// offered, fails or is larger than `cap`.
    fn read(&self, mime: &str, cap: usize) -> Option<Vec<u8>>;
    /// Like [`Self::read`], but a longer content is cut to its first `cap`
    /// bytes instead of refused: a huge text copy is kept, shortened.
    fn read_prefix(&self, mime: &str, cap: usize) -> Option<Vec<u8>>;
    fn focused_app(&self) -> Option<String>;
}

const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;
/// The HTML of a rich copy is kept up to this size; above it, only the text.
const MAX_HTML_BYTES: usize = 1024 * 1024;

/// Decide what one change is (Q6.4) and read just that: the richest kept
/// format, nothing for a password or an ignored app.
pub(crate) fn capture_change(
    source: &dyn ClipboardSource,
    ignored: &[String],
    images_allowed: bool,
) -> Option<Captured> {
    use crate::services::clipboard_formats::{
        Capture, HTML, PNG, URI_LIST, choose, uri_list_paths,
    };
    let text_cap = crate::search::clipboard::MAX_CLIPBOARD_TEXT_BYTES + 4096;
    let types = source.types();
    // Asking the compositor costs a process; only when someone is ignored.
    let app = if ignored.is_empty() {
        None
    } else {
        source.focused_app()
    };
    let read_text = |mime: &str| {
        let bytes = source.read_prefix(mime, text_cap)?;
        // A cut can land inside a UTF-8 character.
        let valid = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(error) => error.valid_up_to(),
        };
        decode_clipboard_text(&bytes[..valid])
    };
    match choose(&types, app.as_deref(), ignored) {
        Capture::Skip(_) => None,
        Capture::Image => {
            if !images_allowed {
                return None;
            }
            let bytes = source.read(PNG, MAX_IMAGE_BYTES)?;
            bytes
                .starts_with(b"\x89PNG\r\n\x1a\n")
                .then_some(Captured::Image(bytes))
        }
        Capture::Files => {
            let raw = source.read(URI_LIST, text_cap)?;
            let list = String::from_utf8(raw.clone()).ok()?;
            let paths = uri_list_paths(&list);
            if paths.is_empty() {
                return None;
            }
            Some(Captured::Text {
                text: paths.join("\n"),
                extra: Some((URI_LIST, raw)),
            })
        }
        Capture::Html => {
            let text = read_text("text")?;
            let extra = source
                .read(HTML, MAX_HTML_BYTES)
                .filter(|html| std::str::from_utf8(html).is_ok())
                .map(|html| (HTML, html));
            Some(Captured::Text { text, extra })
        }
        Capture::Text => Some(Captured::Text {
            text: read_text("text")?,
            extra: None,
        }),
    }
}

/// The live clipboard, through `wl-paste` and the compositor.
struct WlPaste;

impl ClipboardSource for WlPaste {
    fn types(&self) -> Vec<String> {
        wl_paste(&["--list-types"], 64 * 1024)
            .map(|(bytes, _)| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read(&self, mime: &str, cap: usize) -> Option<Vec<u8>> {
        wl_paste(&["--no-newline", "--type", mime], cap)
            .filter(|(_, truncated)| !truncated)
            .map(|(bytes, _)| bytes)
    }

    fn read_prefix(&self, mime: &str, cap: usize) -> Option<Vec<u8>> {
        wl_paste(&["--no-newline", "--type", mime], cap).map(|(bytes, _)| bytes)
    }

    fn focused_app(&self) -> Option<String> {
        crate::services::compositor::focused_app_id()
    }
}

/// `wl-paste args`, at most `cap` bytes of stdout: `(bytes, truncated)`.
/// A truncated run was killed at the cap, which is not a failure.
fn wl_paste(args: &[&str], cap: usize) -> Option<(Vec<u8>, bool)> {
    let mut command = std::process::Command::new("wl-paste");
    command.args(args);
    let run =
        crate::process::run_capped(&mut command, std::time::Duration::from_secs(3), cap, 4096)
            .ok()?;
    if run.stdout_truncated {
        return Some((run.stdout, true));
    }
    (run.status.success() && !run.timed_out).then_some((run.stdout, false))
}

/// Background clipboard capture (Q6.4): `wl-paste --watch` only signals a
/// change; each change is then read in the format worth keeping, and skipped
/// for passwords and `clipboard_ignore_apps`.
pub(crate) fn install_clipboard_background_watcher(launcher: &Rc<RefCell<Zeshicast>>) {
    if !launcher.borrow().clipboard_history_enabled() {
        return;
    }

    set_image_capture_allowed(launcher.borrow().clipboard_capture_images());
    set_ignored_apps(launcher.borrow().clipboard_ignored_apps());
    let (tx, rx) = std::sync::mpsc::channel::<Option<Captured>>();
    std::thread::Builder::new()
        .name("clipboard-watch".to_string())
        .spawn(move || watch_clipboard(tx))
        .ok();

    let launcher = Rc::clone(launcher);
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        // Mirror the preferences into the watcher's view, so private mode,
        // image capture and the ignore list apply without a restart.
        set_image_capture_allowed(launcher.borrow().clipboard_capture_images());
        set_ignored_apps(launcher.borrow().clipboard_ignored_apps());
        while let Ok(captured) = rx.try_recv() {
            let result = match captured {
                None => continue,
                Some(Captured::Text { text, extra }) => launcher
                    .borrow_mut()
                    .add_clipboard_entry(
                        &text,
                        extra.as_ref().map(|(mime, data)| (*mime, data.as_slice())),
                    )
                    .map(drop),
                Some(Captured::Image(bytes)) => {
                    // Saved here, after the private-mode check the watcher saw
                    // may have gone stale.
                    if !launcher.borrow().clipboard_capture_images() {
                        continue;
                    }
                    crate::services::clipboard_store::save_clipboard_image(&bytes)
                        .and_then(|path| launcher.borrow_mut().add_clipboard_image(&path).map(drop))
                }
            };
            if let Err(error) = result {
                log::warn!("failed to save clipboard history: {error}");
            }
        }
        glib::ControlFlow::Continue
    });
}

fn watch_clipboard(tx: std::sync::mpsc::Sender<Option<Captured>>) {
    use std::io::BufRead;
    loop {
        // The command only marks the change; its stdin (whatever type
        // wl-paste picked) is thrown away and read again in the right one.
        let child = crate::process::die_with_parent(&mut std::process::Command::new("wl-paste"))
            .args(["--watch", "sh", "-c", "cat >/dev/null; echo"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            // wl-clipboard not installed: the gdk monitor is the only path.
            WATCHER_RUNNING.store(false, Ordering::Relaxed);
            return;
        };
        WATCHER_RUNNING.store(true, Ordering::Relaxed);
        if let Some(stdout) = child.stdout.take() {
            for line in std::io::BufReader::new(stdout).lines() {
                if line.is_err() {
                    break;
                }
                let captured = capture_change(&WlPaste, &ignored_apps(), image_capture_allowed());
                if captured.is_some() && tx.send(captured).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        // wl-paste died (a compositor restart): reconnect shortly, unless the
        // receiver is gone (the empty probe doubles as a liveness check).
        std::thread::sleep(std::time::Duration::from_secs(1));
        if tx.send(None).is_err() {
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
    // The wl-paste watcher sees every copy with its source app; this path
    // would only re-add what is on the clipboard when the palette opens,
    // including copies from ignored apps.
    if WATCHER_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    let formats = clipboard.formats();
    if formats.contain_mime_type(crate::services::clipboard_formats::PASSWORD_HINT) {
        return;
    }
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
        let Ok(path_str) = crate::services::clipboard_store::save_clipboard_image(png.as_ref())
        else {
            return;
        };

        let value = format!(
            "{}{}",
            crate::services::clipboard_store::CLIPBOARD_IMAGE_PREFIX,
            path_str
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake {
        offers: HashMap<&'static str, Vec<u8>>,
        app: Option<&'static str>,
        app_asked: std::cell::Cell<bool>,
    }

    impl Fake {
        fn new(offers: &[(&'static str, &[u8])], app: Option<&'static str>) -> Self {
            Self {
                offers: offers
                    .iter()
                    .map(|(mime, data)| (*mime, data.to_vec()))
                    .collect(),
                app,
                app_asked: std::cell::Cell::new(false),
            }
        }
    }

    impl ClipboardSource for Fake {
        fn types(&self) -> Vec<String> {
            self.offers.keys().map(|mime| mime.to_string()).collect()
        }

        fn read(&self, mime: &str, cap: usize) -> Option<Vec<u8>> {
            self.read_prefix(mime, usize::MAX)
                .filter(|data| data.len() <= cap)
        }

        fn read_prefix(&self, mime: &str, cap: usize) -> Option<Vec<u8>> {
            // `--type text` is wl-paste's "best text type".
            let mime = if mime == "text" { "text/plain" } else { mime };
            let data = self.offers.get(mime)?;
            Some(data[..data.len().min(cap)].to_vec())
        }

        fn focused_app(&self) -> Option<String> {
            self.app_asked.set(true);
            self.app.map(str::to_string)
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    #[test]
    fn each_change_is_read_in_the_format_worth_keeping() {
        let html = Fake::new(&[("text/html", b"<b>Hi</b>"), ("text/plain", b"Hi")], None);
        assert_eq!(
            capture_change(&html, &[], true),
            Some(Captured::Text {
                text: "Hi".to_string(),
                extra: Some(("text/html", b"<b>Hi</b>".to_vec())),
            })
        );

        let files = Fake::new(
            &[
                (
                    "text/uri-list",
                    b"file:///home/me/a%20b.pdf\r\nfile:///tmp/c\r\n",
                ),
                ("text/plain", b"x"),
            ],
            None,
        );
        let Some(Captured::Text { text, extra }) = capture_change(&files, &[], true) else {
            panic!("files are kept");
        };
        assert_eq!(text, "/home/me/a b.pdf\n/tmp/c");
        assert_eq!(extra.unwrap().0, "text/uri-list");

        let image = Fake::new(&[("image/png", PNG), ("text/plain", b"x")], None);
        assert_eq!(
            capture_change(&image, &[], true),
            Some(Captured::Image(PNG.to_vec()))
        );

        let plain = Fake::new(&[("text/plain", b"just text")], None);
        assert_eq!(
            capture_change(&plain, &[], true),
            Some(Captured::Text {
                text: "just text".to_string(),
                extra: None
            })
        );
    }

    #[test]
    fn passwords_and_ignored_apps_leave_nothing() {
        let password = Fake::new(
            &[
                ("text/plain", b"hunter2"),
                ("x-kde-passwordManagerHint", b"secret"),
            ],
            Some("firefox"),
        );
        assert_eq!(capture_change(&password, &[], true), None);

        let ignored = vec!["KeePassXC".to_string()];
        let from_keepass = Fake::new(
            &[("text/plain", b"hunter2")],
            Some("org.keepassxc.KeePassXC"),
        );
        assert_eq!(capture_change(&from_keepass, &ignored, true), None);

        let from_editor = Fake::new(&[("text/plain", b"notes")], Some("dev.zed.Zed"));
        assert!(capture_change(&from_editor, &ignored, true).is_some());

        let nobody_ignored = Fake::new(&[("text/plain", b"notes")], Some("x"));
        capture_change(&nobody_ignored, &[], true);
        assert!(
            !nobody_ignored.app_asked.get(),
            "no compositor call without an ignore list"
        );
    }

    /// P3: with image capture off (private mode, images disabled) nothing is
    /// read into a PNG to be cached.
    #[test]
    fn disabled_image_capture_reads_no_image() {
        let image = Fake::new(&[("image/png", PNG)], None);
        assert_eq!(capture_change(&image, &[], false), None);
        let not_png = Fake::new(&[("image/png", b"GIF89a")], None);
        assert_eq!(capture_change(&not_png, &[], true), None);
    }

    #[test]
    fn a_huge_text_copy_is_kept_shortened() {
        let mut huge = "ж".repeat(40_000).into_bytes();
        huge.extend_from_slice(b"tail");
        let text = Fake::new(&[("text/plain", &huge)], None);
        let Some(Captured::Text { text, .. }) = capture_change(&text, &[], true) else {
            panic!("kept");
        };
        assert!(text.len() <= crate::search::clipboard::MAX_CLIPBOARD_TEXT_BYTES + 4096);
        assert!(
            text.chars().all(|c| c == 'ж'),
            "cut on a character boundary"
        );
    }

    #[test]
    fn oversized_html_keeps_the_text_only() {
        let big = vec![b'a'; MAX_HTML_BYTES + 1];
        let html = Fake::new(&[("text/html", &big), ("text/plain", b"short")], None);
        assert_eq!(
            capture_change(&html, &[], true),
            Some(Captured::Text {
                text: "short".to_string(),
                extra: None
            })
        );
    }
}
