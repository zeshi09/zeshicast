use std::collections::HashMap;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, Entry, Label, Orientation,
    Paned, Stack,
};

#[derive(Clone)]
pub struct PreferencesView {
    pub root: GtkBox,
    pub search: Entry,
    pub fields: Vec<(String, Entry)>,
    pub save: Button,
    pub cancel: Button,
}

pub fn preferences_view(current: &HashMap<String, String>) -> PreferencesView {

    let outer = crate::ui::panel_root(0, 0);
    outer.set_vexpand(true);
    outer.set_hexpand(true);

    let header_box = GtkBox::new(Orientation::Horizontal, 0);
    header_box.set_margin_top(12);
    header_box.set_margin_start(14);
    header_box.set_margin_end(14);
    header_box.set_margin_bottom(8);
    let header = crate::ui::panel_title("Preferences");
    header_box.append(&header);
    outer.append(&header_box);

    let search = Entry::builder()
        .placeholder_text("Search preferences…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    let search_row = GtkBox::new(Orientation::Horizontal, 0);
    search_row.set_margin_start(14);
    search_row.set_margin_end(14);
    search_row.set_margin_bottom(6);
    search_row.append(&search);
    outer.append(&search_row);

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_vexpand(true);
    paned.set_hexpand(true);
    paned.set_position(160);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    let sidebar = crate::ui::results_list();
    sidebar.add_css_class("pref-sidebar");
    sidebar.set_vexpand(true);
    sidebar.set_activate_on_single_click(true);

    let content_stack = Stack::new();
    content_stack.set_vexpand(true);
    content_stack.set_hexpand(true);

    let mut fields = Vec::new();

    for section in crate::ui::preferences::PREFERENCE_SECTIONS {
        let sidebar_row = gtk::ListBoxRow::new();
        sidebar_row.add_css_class("pref-sidebar-row");
        let sidebar_label = Label::new(Some(section.name));
        sidebar_label.add_css_class("pref-sidebar-label");
        sidebar_label.set_xalign(0.0);
        sidebar_label.set_margin_start(12);
        sidebar_label.set_margin_end(12);
        sidebar_row.set_child(Some(&sidebar_label));
        sidebar.append(&sidebar_row);

        let fields_box = GtkBox::new(Orientation::Vertical, 6);
        fields_box.add_css_class("pref-content");
        fields_box.set_vexpand(true);

        for (key, description) in section.keys {
            let row = GtkBox::new(Orientation::Vertical, 2);
            row.add_css_class("pref-field-row");

            let label = Label::new(Some(description));
            label.add_css_class("pref-field-label");
            label.set_xalign(0.0);
            row.append(&label);

            let entry = Entry::new();
            entry.set_hexpand(true);
            if let Some(value) = current.get(*key) {
                entry.set_text(value);
            }
            entry.set_placeholder_text(Some(key));
            row.append(&entry);

            fields.push((key.to_string(), entry));
            fields_box.append(&row);
        }

        let content_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&fields_box)
            .build();
        content_scroller.set_vexpand(true);
        content_stack.add_named(&content_scroller, Some(section.name));
    }

    let sidebar_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&sidebar)
        .build();
    sidebar_scroller.add_css_class("pref-sidebar");
    sidebar_scroller.set_vexpand(true);

    let sidebar_clone = sidebar.clone();
    let stack_clone = content_stack.clone();
    sidebar.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if let Some(section) = crate::ui::preferences::PREFERENCE_SECTIONS.get(index) {
            stack_clone.set_visible_child_name(section.name);
        }
        let _ = &sidebar_clone;
    });

    {
        let sidebar2 = sidebar.clone();
        let stack2 = content_stack.clone();
        search.connect_changed(move |entry| {
            let query = entry.text().to_lowercase();
            let mut first_match: Option<usize> = None;
            for (i, section) in crate::ui::preferences::PREFERENCE_SECTIONS.iter().enumerate() {
                let visible = query.is_empty()
                    || section.name.to_lowercase().contains(&query)
                    || section.keys.iter().any(|(_, desc)| {
                        desc.to_lowercase().contains(&query)
                    });
                if let Some(row) = sidebar2.row_at_index(i as i32) {
                    row.set_visible(visible);
                }
                if visible && first_match.is_none() {
                    first_match = Some(i);
                }
            }
            if let Some(idx) = first_match {
                if let Some(row) = sidebar2.row_at_index(idx as i32) {
                    sidebar2.select_row(Some(&row));
                    if let Some(section) = crate::ui::preferences::PREFERENCE_SECTIONS.get(idx) {
                        stack2.set_visible_child_name(section.name);
                    }
                }
            }
        });
    }

    if let Some(row) = sidebar.row_at_index(0) {
        sidebar.select_row(Some(&row));
    }
    if let Some(first) = crate::ui::preferences::PREFERENCE_SECTIONS.first() {
        content_stack.set_visible_child_name(first.name);
    }

    paned.set_start_child(Some(&sidebar_scroller));
    paned.set_end_child(Some(&content_stack));
    outer.append(&paned);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    buttons.set_margin_end(14);
    buttons.set_margin_bottom(10);

    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&save);
    outer.append(&buttons);

    PreferencesView {
        root: outer,
        search,
        fields,
        save,
        cancel,
    }
}
