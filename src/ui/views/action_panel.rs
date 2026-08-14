use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, Label, ListBox};
use crate::Action;

#[derive(Debug, Clone)]
pub struct ActionPanelDisplayItem {
    pub title: String,
    pub icon_name: String,
    pub is_section_header: bool,
    pub is_destructive: bool,
}

#[derive(Clone)]
pub struct ActionPanelView {
    pub root: GtkBox,
    pub title: Label,
    pub subtitle: Label,
    pub search: Entry,
    pub list: ListBox,
}

pub fn action_panel_view() -> ActionPanelView {
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let title = crate::ui::panel_title("");
    root.append(&title);

    let subtitle = Label::new(None);
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_xalign(0.0);
    root.append(&subtitle);

    let search = Entry::builder()
        .placeholder_text("Search actions")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    root.append(&search);

    let list = crate::ui::results_list();
    let scroller = crate::ui::scrollable_list(&list);
    root.append(&scroller);

    ActionPanelView {
        root,
        title,
        subtitle,
        search,
        list,
    }
}


pub fn set_action_panel_items(
    view: &ActionPanelView,
    action: &Action,
    items: &[ActionPanelDisplayItem],
) {
    view.title.set_text(&action.title);
    view.subtitle.set_text(&action.subtitle);

    set_action_panel_list(&view.list, items);
}

pub fn set_action_panel_list(list: &ListBox, items: &[ActionPanelDisplayItem]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut first_selectable: Option<gtk::ListBoxRow> = None;
    for item in items {
        if item.is_section_header {
            let row = action_section_header_row(&item.title);
            list.append(&row);
        } else {
            let row = crate::ui::secondary_action_row(&item.icon_name, &item.title);
            if item.is_destructive {
                row.add_css_class("danger");
            }
            if first_selectable.is_none() {
                first_selectable = Some(row.clone());
            }
            list.append(&row);
        }
    }

    if let Some(row) = first_selectable {
        list.select_row(Some(&row));
    }
}

fn action_section_header_row(title: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("action-section-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let label = Label::new(Some(title));
    label.add_css_class("action-section-label");
    label.set_xalign(0.0);
    label.set_margin_start(10);
    label.set_margin_end(10);
    row.set_child(Some(&label));
    row
}
