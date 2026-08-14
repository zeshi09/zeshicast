use gtk::prelude::*;
use gtk::{Box as GtkBox, DropDown, Label, ListBox, Orientation, StringList};
use crate::ClipboardSummary;
use super::dashboard::{dashboard_button, dashboard_card_actions, dashboard_card_value, dashboard_plain_card};

#[derive(Clone)]
pub struct ClipboardHistoryView {
    pub root: GtkBox,
    pub list: ListBox,
    pub filter: DropDown,
    pub detail_title: Label,
    pub detail_preview: Label,
    pub detail_kind: Label,
    pub detail_size: Label,
    pub detail_mime: Label,
}

pub fn clipboard_history_view(items: &[ClipboardSummary]) -> ClipboardHistoryView {
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let header_row = GtkBox::new(Orientation::Horizontal, 8);
    header_row.add_css_class("dashboard-header");
    header_row.set_hexpand(true);

    let header = crate::ui::panel_title("Clipboard History");
    header.set_hexpand(true);
    header_row.append(&header);

    let filters = StringList::new(&["All", "Text", "URL", "Command", "Code"]);
    let filter = DropDown::new(Some(filters), gtk::Expression::NONE);
    filter.set_selected(0);
    filter.set_width_request(190);
    filter.set_tooltip_text(Some("Filter clipboard entries by type"));
    header_row.append(&filter);
    root.append(&header_row);

    let split = GtkBox::new(Orientation::Horizontal, 8);
    split.set_vexpand(true);

    let clipboard_card = dashboard_plain_card("Recent Copies", "edit-paste-symbolic");
    clipboard_card.set_vexpand(true);
    clipboard_card.set_hexpand(true);

    let list = crate::ui::results_list();
    set_clipboard_history_items(&list, items);

    let scroller = crate::ui::scrollable_list(&list);
    clipboard_card.append(&scroller);

    let actions = dashboard_card_actions();
    let copy = dashboard_button("Enter Copy");
    let delete = dashboard_button("Delete Remove");
    let clear = dashboard_button("Ctrl+Delete Clear");
    actions.append(&copy);
    actions.append(&delete);
    actions.append(&clear);
    clipboard_card.append(&actions);

    let detail_card = dashboard_plain_card("Preview", "document-open-symbolic");
    detail_card.set_vexpand(true);
    detail_card.set_width_request(320);

    let detail_title = dashboard_card_value();
    detail_title.set_wrap(true);
    detail_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail_card.append(&detail_title);

    let detail_preview = Label::new(None);
    detail_preview.add_css_class("dashboard-card-value");
    detail_preview.set_xalign(0.0);
    detail_preview.set_yalign(0.0);
    detail_preview.set_wrap(true);
    detail_preview.set_selectable(true);
    detail_preview.set_vexpand(true);
    detail_preview.set_valign(gtk::Align::Start);
    detail_preview.set_max_width_chars(42);
    detail_card.append(&detail_preview);

    let metadata = GtkBox::new(Orientation::Vertical, 6);
    metadata.set_margin_top(6);
    let (kind_row, detail_kind) = clipboard_metadata_row("Type");
    let (size_row, detail_size) = clipboard_metadata_row("Size");
    let (mime_row, detail_mime) = clipboard_metadata_row("Mime");
    metadata.append(&kind_row);
    metadata.append(&size_row);
    metadata.append(&mime_row);
    detail_card.append(&metadata);

    split.append(&clipboard_card);
    split.append(&detail_card);
    root.append(&split);

    let view = ClipboardHistoryView {
        root,
        list,
        filter,
        detail_title,
        detail_preview,
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
        view.detail_title.set_text("No clipboard item selected");
        view.detail_preview
            .set_text("Clipboard history is empty for this filter.");
        view.detail_kind.set_text("-");
        view.detail_size.set_text("-");
        view.detail_mime.set_text("-");
        return;
    };

    view.detail_title.set_text(&item.preview);
    view.detail_preview
        .set_text(&clipboard_detail_text(&item.value));
    view.detail_kind.set_text(item.kind.label());
    view.detail_size.set_text(&format_bytes(item.size_bytes));
    view.detail_mime.set_text(item.kind.mime_hint());
}


fn clipboard_row(item: &ClipboardSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name(item.kind.icon_name());
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&item.preview));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle = Label::new(Some(&format!(
        "{} · {}",
        item.kind.label(),
        format_bytes(item.size_bytes)
    )));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);

    if let Some(ts) = item.timestamp {
        let ago = Label::new(Some(&crate::config::format_time_ago(ts)));
        ago.add_css_class("clipboard-ago");
        ago.set_xalign(1.0);
        ago.set_valign(gtk::Align::Center);
        layout.append(&ago);
    }

    row.set_child(Some(&layout));
    row
}

fn clipboard_metadata_row(label: &str) -> (GtkBox, Label) {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_hexpand(true);

    let name = Label::new(Some(label));
    name.add_css_class("result-subtitle");
    name.set_xalign(0.0);
    name.set_width_chars(6);
    row.append(&name);

    let value = Label::new(None);
    value.add_css_class("dashboard-card-value");
    value.set_xalign(1.0);
    value.set_hexpand(true);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&value);

    (row, value)
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

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}
