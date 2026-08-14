use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, DropDown, Label, ListBox, Orientation, StringList};

use crate::ClipboardSummary;

#[derive(Clone)]
pub struct ClipboardHistoryView {
    pub root: GtkBox,
    pub list: ListBox,
    pub filter: DropDown,
    pub detail_title: Label,
    pub detail_preview: Label,
    pub detail_image: gtk::Picture,
    pub detail_kind: Label,
    pub detail_size: Label,
    pub detail_mime: Label,
}


pub fn clipboard_history_view(items: &[ClipboardSummary]) -> ClipboardHistoryView {
    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.set_vexpand(true);

    // ── Left panel: list (216px fixed width) ─────────────────────────────────
    let left_panel = GtkBox::new(Orientation::Vertical, 0);
    left_panel.set_width_request(216);

    // filter bar at top
    let filters = StringList::new(&["All", "Text", "URL", "Command", "Code"]);
    let filter = DropDown::new(Some(filters), gtk::Expression::NONE);
    filter.set_selected(0);
    filter.set_margin_top(8);
    filter.set_margin_bottom(4);
    filter.set_margin_start(10);
    filter.set_margin_end(10);
    left_panel.append(&filter);

    // separator
    let sep = gtk::Separator::new(Orientation::Horizontal);
    left_panel.append(&sep);

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);
    set_clipboard_history_items(&list, items);

    let left_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();
    left_panel.append(&left_scroll);

    // ── Right panel: preview ──────────────────────────────────────────────────
    let right_panel = GtkBox::new(Orientation::Vertical, 0);
    right_panel.set_hexpand(true);
    right_panel.set_vexpand(true);

    // Meta bar (type badge + char count + time)
    let meta_bar = GtkBox::new(Orientation::Horizontal, 6);
    meta_bar.add_css_class("ai-model-bar");

    let detail_kind = Label::new(Some("TEXT"));
    detail_kind.add_css_class("ai-model-btn");
    detail_kind.add_css_class("active");
    detail_kind.set_valign(gtk::Align::Center);

    let meta_spacer = GtkBox::new(Orientation::Horizontal, 0);
    meta_spacer.set_hexpand(true);

    let detail_size = Label::new(None);
    detail_size.add_css_class("clipboard-time");
    detail_size.set_valign(gtk::Align::Center);

    let detail_mime = Label::new(None);
    detail_mime.add_css_class("clipboard-time");
    detail_mime.set_valign(gtk::Align::Center);

    meta_bar.append(&detail_kind);
    meta_bar.append(&meta_spacer);
    meta_bar.append(&detail_size);
    meta_bar.append(&detail_mime);
    right_panel.append(&meta_bar);

    // Content area
    let content_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    // Content area holds either the text label or an image preview.
    let content_box = GtkBox::new(Orientation::Vertical, 0);

    let detail_preview = Label::new(None);
    detail_preview.add_css_class("clipboard-text");
    detail_preview.set_xalign(0.0);
    detail_preview.set_yalign(0.0);
    detail_preview.set_wrap(true);
    detail_preview.set_selectable(true);
    detail_preview.set_valign(gtk::Align::Start);
    detail_preview.set_margin_top(10);
    detail_preview.set_margin_bottom(10);
    detail_preview.set_margin_start(12);
    detail_preview.set_margin_end(12);
    content_box.append(&detail_preview);

    // Fills the preview pane width; height follows the image aspect ratio.
    let detail_image = gtk::Picture::new();
    detail_image.set_can_shrink(true);
    detail_image.set_hexpand(true);
    detail_image.set_halign(gtk::Align::Fill);
    detail_image.set_valign(gtk::Align::Start);
    detail_image.add_css_class("clipboard-image");
    detail_image.set_visible(false);
    content_box.append(&detail_image);

    content_scroll.set_child(Some(&content_box));
    right_panel.append(&content_scroll);

    // Invisible compat label for detail_title
    let detail_title = Label::new(None);
    detail_title.set_visible(false);
    right_panel.append(&detail_title);

    // Copy button (full width, bottom)
    let copy_row = GtkBox::new(Orientation::Horizontal, 0);
    copy_row.add_css_class("ai-input-row");

    let copy = Button::with_label("Copy to clipboard");
    copy.add_css_class("ai-send-btn");
    copy.add_css_class("ready");
    copy.set_hexpand(true);
    copy_row.append(&copy);
    right_panel.append(&copy_row);

    // Vertical separator between panels
    let vsep = gtk::Separator::new(Orientation::Vertical);
    root.append(&left_panel);
    root.append(&vsep);
    root.append(&right_panel);

    let view = ClipboardHistoryView {
        root,
        list,
        filter,
        detail_title,
        detail_preview,
        detail_image,
        detail_kind,
        detail_size,
        detail_mime,
    };
    set_clipboard_detail(&view, items.first());
    view
}


pub fn set_clipboard_history_items(list: &ListBox, items: &[ClipboardSummary]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in items {
        list.append(&clipboard_row(item));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

pub fn set_clipboard_detail(view: &ClipboardHistoryView, item: Option<&ClipboardSummary>) {
    let Some(item) = item else {
        view.detail_preview.set_visible(true);
        view.detail_image.set_visible(false);
        view.detail_preview.set_text("No item selected");
        view.detail_kind.set_text("–");
        view.detail_size.set_text("");
        view.detail_mime.set_text("");
        return;
    };

    view.detail_kind.set_text(item.kind.label());
    if let Some(ts) = item.timestamp {
        view.detail_mime
            .set_text(&format!("·  {}", crate::config::format_time_ago(ts)));
    } else {
        view.detail_mime.set_text("");
    }

    // Image entry → show the picture; otherwise the text label.
    if let Some(path) = crate::clipboard_image_path(&item.value) {
        view.detail_preview.set_visible(false);
        view.detail_image.set_visible(true);
        match gtk::gdk::Texture::from_filename(path) {
            Ok(texture) => {
                view.detail_image.set_paintable(Some(&texture));
                view.detail_size
                    .set_text(&format!("{}×{}", texture.width(), texture.height()));
            }
            Err(_) => {
                view.detail_image.set_paintable(gtk::gdk::Paintable::NONE);
                view.detail_size.set_text("missing");
            }
        }
        return;
    }

    view.detail_preview.set_visible(true);
    view.detail_image.set_visible(false);
    view.detail_preview
        .set_text(&clipboard_detail_text(&item.value));
    // Character count (matches the mockup's "N ch"), not raw byte size.
    view.detail_size
        .set_text(&format!("{} ch", item.value.chars().count()));

    // Code/URL → monospace style
    let is_code = matches!(
        item.kind,
        crate::ClipboardKind::Code | crate::ClipboardKind::Command
    );
    if is_code {
        view.detail_preview.add_css_class("code");
    } else {
        view.detail_preview.remove_css_class("code");
    }
}


fn clipboard_row(item: &ClipboardSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 8);
    layout.set_margin_start(12);
    layout.set_margin_end(12);
    layout.set_valign(gtk::Align::Center);

    // Type icon (16×16; image entries use the generic image icon).
    let icon = gtk::Image::from_icon_name(item.kind.icon_name());
    icon.set_pixel_size(16);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    // First line of preview (truncated)
    let first_line = item.preview.lines().next().unwrap_or(&item.preview);
    let title = Label::new(Some(first_line));
    let is_code = matches!(
        item.kind,
        crate::ClipboardKind::Code | crate::ClipboardKind::Command
    );
    if is_code {
        title.add_css_class("clipboard-text");
        title.add_css_class("code");
    } else {
        title.add_css_class("clipboard-text");
    }
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    text.append(&title);

    // Timestamp
    if let Some(ts) = item.timestamp {
        let ago = Label::new(Some(&crate::config::format_time_ago(ts)));
        ago.add_css_class("clipboard-time");
        ago.set_xalign(0.0);
        text.append(&ago);
    }

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

fn clipboard_detail_text(value: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 1400;
    if value.chars().count() <= MAX_DETAIL_CHARS {
        return value.to_string();
    }

    let mut detail = value.chars().take(MAX_DETAIL_CHARS).collect::<String>();
    detail.push_str("\n...");
    detail
}

