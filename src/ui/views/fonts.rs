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

    // Populate initial list
    let fonts = list_system_fonts();
    populate_font_list(
        &list,
        &fonts,
        "",
        "The quick brown fox jumps over the lazy dog",
    );

    // Wire up search filter
    {
        let list_c = list.clone();
        let preview_c = preview_entry.clone();
        let fonts_c = fonts.clone();
        search.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c, &e.text(), &preview_c.text());
        });
    }
    {
        let list_c = list.clone();
        let search_c = search.clone();
        let fonts_c = fonts.clone();
        preview_entry.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c, &search_c.text(), &e.text());
        });
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

