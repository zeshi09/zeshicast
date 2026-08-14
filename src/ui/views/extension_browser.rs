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

    let header = crate::ui::panel_title("Extensions");
    root.append(&header);

    let list = crate::ui::results_list();
    for command in commands {
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

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name(&command.icon_name);
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&command.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle_text = if !command.description.is_empty() {
        command.description.as_str()
    } else {
        command.keyword.as_deref().unwrap_or_default()
    };
    let subtitle = Label::new(Some(subtitle_text));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}