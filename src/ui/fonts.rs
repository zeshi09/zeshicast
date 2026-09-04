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



use std::cell::RefCell;
use std::rc::Rc;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, Label, ListBox, Orientation};

#[derive(Clone)]
pub struct FontBrowserView {
    pub root: GtkBox,
    pub search: Entry,
    pub preview_entry: Entry,
    pub list: ListBox,
}

pub fn font_browser_view() -> FontBrowserView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Top bar: search + preview text input
    let top_bar = GtkBox::new(Orientation::Horizontal, 8);
    top_bar.add_css_class("search-bar");

    let search = Entry::builder()
        .placeholder_text("Search fonts…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");

    let preview_entry = Entry::builder()
        .text("The quick brown fox jumps over the lazy dog")
        .width_chars(24)
        .build();
    preview_entry.add_css_class("search-entry");

    top_bar.append(&search);
    top_bar.append(&preview_entry);
    root.append(&top_bar);

    // Font list
    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroll);

    // Fonts load off-thread: the window must appear before `fc-list`
    // finishes, so the list starts empty and is populated when the worker
    // reports back (same deferred-load pattern as the launcher file index).
    let fonts = Rc::new(RefCell::new(Vec::<String>::new()));

    // Wire up search filter
    {
        let list_c = list.clone();
        let preview_c = preview_entry.clone();
        let fonts_c = Rc::clone(&fonts);
        search.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c.borrow(), &e.text(), &preview_c.text());
        });
    }
    {
        let list_c = list.clone();
        let search_c = search.clone();
        let fonts_c = Rc::clone(&fonts);
        preview_entry.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c.borrow(), &search_c.text(), &e.text());
        });
    }

    {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(list_system_fonts());
        });
        let fonts = Rc::clone(&fonts);
        let list_c = list.clone();
        let search_c = search.clone();
        let preview_c = preview_entry.clone();
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            move || match receiver.try_recv() {
                Ok(loaded) => {
                    *fonts.borrow_mut() = loaded;
                    populate_font_list(&list_c, &fonts.borrow(), &search_c.text(), &preview_c.text());
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            },
        );
    }

    FontBrowserView {
        root,
        search,
        preview_entry,
        list,
    }
}

fn list_system_fonts() -> Vec<String> {
    let out = std::process::Command::new("fc-list")
        .args([":", "family"])
        .output();
    match out {
        Ok(o) => {
            let mut fonts: Vec<String> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .flat_map(|l| l.split(',').map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty() && !s.starts_with('.'))
                .collect();
            fonts.sort();
            fonts.dedup();
            fonts
        }
        Err(_) => vec!["Sans".into(), "Serif".into(), "Monospace".into()],
    }
}

fn populate_font_list(list: &ListBox, fonts: &[String], query: &str, preview: &str) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let q = query.trim().to_lowercase();
    let preview_text = if preview.trim().is_empty() {
        "The quick brown fox jumps over the lazy dog"
    } else {
        preview
    };

    let mut shown = 0;
    for font in fonts {
        if !q.is_empty() && !font.to_lowercase().contains(&q) {
            continue;
        }
        if shown >= 120 {
            break;
        }
        list.append(&font_row(font, preview_text));
        shown += 1;
    }
}

fn font_row(font_name: &str, preview: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    row.set_height_request(52);

    let layout = GtkBox::new(Orientation::Vertical, 2);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);
    layout.set_margin_top(6);
    layout.set_margin_bottom(6);

    // Font name label (uppercase, muted, 10px)
    let name_lbl = Label::new(Some(font_name));
    name_lbl.add_css_class("font-name-label");
    name_lbl.set_xalign(0.0);
    layout.append(&name_lbl);

    // Preview text in that font using Pango markup
    let escaped = font_name.replace('"', "");
    let safe_preview: String = preview.chars().take(60).collect();
    let markup = format!(
        "<span font_desc=\"{escaped} 16\">{}</span>",
        glib::markup_escape_text(&safe_preview)
    );
    let preview_lbl = Label::new(None);
    preview_lbl.set_markup(&markup);
    preview_lbl.set_xalign(0.0);
    preview_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    layout.append(&preview_lbl);

    row.set_child(Some(&layout));
    row
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
