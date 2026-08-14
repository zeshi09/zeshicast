use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, ListBox, Orientation};

use crate::CommandSummary;

#[derive(Clone)]
pub struct ExtensionBrowserView {
    pub root: GtkBox,
    pub list: ListBox,
}


pub fn extension_browser_view(commands: &[CommandSummary]) -> ExtensionBrowserView {
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let active_count = commands.iter().filter(|c| c.enabled).count();
    let header_text = format!(
        "Extensions  ·  {} of {} active",
        active_count,
        commands.len()
    );
    let header = crate::ui::panel_title(&header_text);
    root.append(&header);

    let list = crate::ui::results_list();
    let mut current_group = String::new();
    for command in commands {
        let group = command.extension_group();
        if group != current_group {
            current_group = group.clone();
            list.append(&extension_section_header(
                &group,
                &command.extension_detail(),
            ));
        }
        let row = extension_row(command);
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    let scroller = crate::ui::scrollable_list(&list);
    root.append(&scroller);
    ExtensionBrowserView { root, list }
}


fn extension_row(command: &CommandSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    if !command.enabled {
        row.add_css_class("extension-disabled");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // 32×32 icon box
    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_width_request(32);
    icon_box.set_height_request(32);
    icon_box.add_css_class("control-card-icon");
    if command.enabled {
        icon_box.add_css_class("active");
    }
    icon_box.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name(&command.icon_name);
    icon.set_pixel_size(18);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_hexpand(true);
    icon.set_vexpand(true);
    icon_box.append(&icon);
    layout.append(&icon_box);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&command.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let mut meta = vec![command.kind.clone()];
    if command.category != command.kind {
        meta.push(command.category.clone());
    }
    if let Some(keyword) = &command.keyword {
        meta.push(format!("keyword: {keyword}"));
    }
    if !command.tags.is_empty() {
        meta.push(format!("tags: {}", command.tags.join(", ")));
    }
    let permission_text = if command.permissions.is_empty() {
        "permissions: none".to_string()
    } else {
        format!("permissions: {}", command.permissions.join(", "))
    };
    meta.push(permission_text);

    let meta_text = meta.join(" - ");
    let subtitle_text = if command.description.is_empty() {
        meta_text
    } else {
        format!("{} - {meta_text}", command.description)
    };
    let subtitle = Label::new(Some(&subtitle_text));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);
    layout.append(&text);

    // Toggle switch — toggling changes row opacity visually
    let toggle = gtk::Switch::new();
    toggle.set_active(command.enabled);
    toggle.set_valign(gtk::Align::Center);
    {
        let row_ref = row.clone();
        let icon_box_ref = icon_box.clone();
        toggle.connect_active_notify(move |sw| {
            if sw.is_active() {
                row_ref.remove_css_class("extension-disabled");
                icon_box_ref.add_css_class("active");
            } else {
                row_ref.add_css_class("extension-disabled");
                icon_box_ref.remove_css_class("active");
            }
        });
    }
    layout.append(&toggle);

    row.set_child(Some(&layout));
    row
}

fn extension_section_header(title: &str, detail: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("action-section-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let layout = GtkBox::new(Orientation::Vertical, 1);
    layout.set_margin_start(10);
    layout.set_margin_end(10);
    layout.set_margin_top(6);
    layout.set_margin_bottom(4);

    let title_label = Label::new(Some(title));
    title_label.add_css_class("action-section-label");
    title_label.set_xalign(0.0);

    let detail_label = Label::new(Some(detail));
    detail_label.add_css_class("result-subtitle");
    detail_label.set_xalign(0.0);

    layout.append(&title_label);
    layout.append(&detail_label);
    row.set_child(Some(&layout));
    row
}
