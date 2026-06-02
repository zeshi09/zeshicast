use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ScrolledWindow,
};

use crate::Action;

pub fn action_panel(
    parent: &ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> Option<ApplicationWindow> {
    let app = parent.application()?;
    let panel = ApplicationWindow::builder()
        .application(&app)
        .title(title)
        .transient_for(parent)
        .default_width(default_width)
        .default_height(default_height)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");
    Some(panel)
}

pub fn panel_root(spacing: i32, margin: i32) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, spacing);
    root.set_margin_top(margin);
    root.set_margin_bottom(margin);
    root.set_margin_start(margin);
    root.set_margin_end(margin);
    root
}

pub fn panel_title(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("action-panel-title");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}

pub fn results_list() -> ListBox {
    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);
    list
}

pub fn scrollable_list(list: &ListBox) -> ScrolledWindow {
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .propagate_natural_height(false)
        .child(list)
        .build();
    scroller.add_css_class("results-scroll");
    scroller.set_vexpand(true);
    scroller
}

pub fn action_button(icon_name: &str, tooltip: &str) -> Button {
    let button = Button::builder().icon_name(icon_name).build();
    button.add_css_class("action-button");
    button.set_tooltip_text(Some(tooltip));
    button
}

pub fn move_selection(list: &ListBox, delta: i32) {
    let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
    let mut next = (current + delta).max(0);

    while let Some(row) = list.row_at_index(next) {
        if row.is_selectable() {
            list.select_row(Some(&row));
            return;
        }
        if next == 0 && delta < 0 {
            return;
        }
        next = (next + delta.signum()).max(0);
    }
}

pub fn result_row(action: &Action) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 12);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(14);
    layout.set_margin_end(14);

    let icon = Image::from_icon_name(&action.icon_name);
    icon.add_css_class("result-icon");
    icon.set_pixel_size(28);

    let text = GtkBox::new(Orientation::Horizontal, 8);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&action.title));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(false);
    title.set_margin_top(1);
    title.set_margin_bottom(1);

    let subtitle = Label::new(Some(&action.subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);
    subtitle.set_margin_top(1);
    subtitle.set_margin_bottom(1);

    text.append(&title);
    text.append(&subtitle);

    let category = Label::new(Some(&action.category));
    category.add_css_class("category-pill");
    category.set_xalign(0.5);
    category.set_valign(gtk::Align::Center);
    category.set_margin_top(1);
    category.set_margin_bottom(1);

    layout.append(&icon);
    layout.append(&text);
    layout.append(&category);
    row.set_child(Some(&layout));
    row
}

pub fn section_header(title: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("section-header-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let label = Label::new(Some(title));
    label.add_css_class("section-header");
    label.set_xalign(0.0);
    label.set_margin_top(8);
    label.set_margin_bottom(4);
    label.set_margin_start(16);
    label.set_margin_end(16);
    row.set_child(Some(&label));
    row
}

pub fn secondary_action_row(icon_name: &str, title: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(10);
    layout.set_margin_bottom(10);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let label = Label::new(Some(title));
    label.add_css_class("result-title");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_margin_top(1);
    label.set_margin_bottom(1);

    layout.append(&icon);
    layout.append(&label);
    row.set_child(Some(&layout));
    row
}
