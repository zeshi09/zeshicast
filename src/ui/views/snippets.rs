use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, ListBox, Orientation};

use crate::SnippetSummary;

#[derive(Clone)]
pub struct SnippetManagerView {
    pub root: GtkBox,
    pub list: ListBox,
}


pub fn snippet_manager_view(items: &[SnippetSummary]) -> SnippetManagerView {
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Snippets");
    root.append(&header);

    let list = crate::ui::results_list();
    set_snippet_items(&list, items);

    let scroller = crate::ui::scrollable_list(&list);
    root.append(&scroller);
    SnippetManagerView { root, list }
}


pub fn set_snippet_items(list: &ListBox, items: &[SnippetSummary]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in items {
        list.append(&snippet_row(item));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}


fn snippet_row(item: &SnippetSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name("insert-text-symbolic");
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&item.name));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle = Label::new(Some(&item.preview));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

