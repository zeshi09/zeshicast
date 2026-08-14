use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation};
use crate::NotificationSnapshot;
use super::dashboard::{
    dashboard_button, dashboard_card_actions, dashboard_grid, dashboard_plain_card,
    dashboard_subtitle_label,
};

#[derive(Clone)]
pub struct NotificationsView {
    pub root: GtkBox,
    pub backend: Label,
    pub count: Label,
    pub dnd: Label,
    pub message: Label,
    pub history: ListBox,
    pub toggle_dnd: Button,
    pub close_all: Button,
    pub open_panel: Button,
}


pub fn notifications_view(snapshot: &NotificationSnapshot) -> NotificationsView {
    let root = crate::ui::panel_root(10, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Notifications");
    root.append(&header);

    let overview_grid = dashboard_grid();
    let (backend_card, backend, _) = crate::ui::control_card("Backend", "applications-system-symbolic");
    let (count_card, count, _) = crate::ui::control_card("History", "document-open-recent-symbolic");
    let (dnd_card, dnd, _) = crate::ui::control_card("DND", "notifications-disabled-symbolic");
    overview_grid.attach(&backend_card, 0, 0, 1, 1);
    overview_grid.attach(&count_card, 1, 0, 1, 1);
    overview_grid.attach(&dnd_card, 0, 1, 1, 1);

    let message = dashboard_subtitle_label();
    message.set_wrap(true);
    let controls_card =
        dashboard_plain_card("Controls", "preferences-system-notifications-symbolic");
    controls_card.append(&message);

    let buttons = dashboard_card_actions();
    let toggle_dnd = dashboard_button("DND");
    let close_all = dashboard_button("Close All");
    let open_panel = dashboard_button("Panel");
    buttons.append(&toggle_dnd);
    buttons.append(&close_all);
    buttons.append(&open_panel);
    controls_card.append(&buttons);
    overview_grid.attach(&controls_card, 1, 1, 1, 1);
    root.append(&overview_grid);

    let history_card = dashboard_plain_card("History", "view-list-symbolic");
    history_card.set_vexpand(true);
    let history = crate::ui::results_list();
    let history_scroller = crate::ui::scrollable_list(&history);
    history_card.append(&history_scroller);
    root.append(&history_card);

    let view = NotificationsView {
        root,
        backend,
        count,
        dnd,
        message,
        history,
        toggle_dnd,
        close_all,
        open_panel,
    };
    set_notification_snapshot(&view, snapshot);
    view
}


pub fn set_notification_snapshot(view: &NotificationsView, snapshot: &NotificationSnapshot) {
    view.backend
        .set_text(snapshot.backend.as_deref().unwrap_or("not detected"));
    view.count.set_text(
        &snapshot
            .count
            .map(|count| count.to_string())
            .unwrap_or("unknown".to_string()),
    );
    view.dnd.set_text(
        &snapshot
            .dnd
            .map(|enabled| {
                if enabled {
                    "enabled".to_string()
                } else {
                    "disabled".to_string()
                }
            })
            .unwrap_or("unknown".to_string()),
    );

    if snapshot.is_available() {
        if snapshot.history.is_empty() {
            view.message
                .set_text("Notification backend detected. No readable history entries.");
        } else {
            view.message
                .set_text(&format!("{} history entries", snapshot.history.len()));
        }
    } else {
        view.message
            .set_text("No supported notification backend found. Install or enable swaync or dunst to expose notification state here.");
    }

    set_notification_history_rows(&view.history, snapshot);
}

fn set_notification_history_rows(list: &ListBox, snapshot: &NotificationSnapshot) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for entry in snapshot.history.iter().take(12) {
        list.append(&notification_history_row(entry));
    }
}

fn notification_history_row(
    entry: &crate::services::notifications::NotificationEntrySnapshot,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name("preferences-system-notifications-symbolic");
    icon.add_css_class("result-icon");
    icon.set_pixel_size(20);
    layout.append(&icon);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&entry.summary));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    let subtitle = [entry.app_name.as_deref(), entry.body.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("  ");
    let subtitle = Label::new(Some(&subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_xalign(0.0);
    text.append(&subtitle);

    layout.append(&text);
    row.set_child(Some(&layout));
    row
}
