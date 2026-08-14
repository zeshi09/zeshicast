use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, Label, ListBox, ListBoxRow, Orientation};

use crate::NotificationSnapshot;

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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Top bar: DND toggle + Close All
    let top_bar = GtkBox::new(Orientation::Horizontal, 8);
    top_bar.add_css_class("action-bar");

    let backend = Label::new(None);
    backend.add_css_class("result-subtitle");
    backend.set_hexpand(true);
    backend.set_xalign(0.0);
    top_bar.append(&backend);

    let dnd = Label::new(None);
    dnd.add_css_class("status-chip");
    top_bar.append(&dnd);

    let toggle_dnd = Button::with_label("DND");
    toggle_dnd.add_css_class("action-bar-more");
    let close_all = Button::with_label("Clear All");
    close_all.add_css_class("action-bar-more");
    // We're the notification daemon ourselves — there's no external panel to
    // open, so the Settings button is kept (struct compat) but hidden.
    let open_panel = Button::with_label("Settings");
    open_panel.add_css_class("action-bar-more");
    open_panel.set_visible(false);

    // DND / Clear All are wired in launcher.rs (so they can refresh the view).

    top_bar.append(&toggle_dnd);
    top_bar.append(&close_all);
    top_bar.append(&open_panel);
    root.append(&top_bar);

    // Notification list
    let history = ListBox::new();
    history.add_css_class("results-list");
    history.set_vexpand(true);
    history.set_activate_on_single_click(false);

    let scroller = crate::ui::scrollable_list(&history);
    root.append(&scroller);

    // Empty state message (shown when list is empty)
    let message = Label::new(Some("All caught up ✓"));
    message.add_css_class("result-subtitle");
    message.set_xalign(0.5);
    message.set_valign(gtk::Align::Center);
    message.set_vexpand(true);
    message.set_margin_top(40);
    root.append(&message);

    // Compat labels
    let count = Label::new(None);
    count.set_visible(false);

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
    let backend_text = snapshot.backend.as_deref().unwrap_or("No backend");
    view.backend.set_text(backend_text);

    let dnd_on = snapshot.dnd.unwrap_or(false);
    view.dnd.set_text(if dnd_on { "DND On" } else { "" });
    view.dnd.set_visible(dnd_on);
    if dnd_on {
        view.dnd.add_css_class("active");
    } else {
        view.dnd.remove_css_class("active");
    }

    set_notification_history_rows(&view.history, snapshot);

    let has_notifs = !snapshot.history.is_empty();
    view.message.set_visible(!has_notifs);
    if !has_notifs {
        if snapshot.is_available() {
            view.message.set_text("All caught up ✓");
        } else {
            view.message.set_text("No notification backend detected");
        }
    }
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
    layout.set_margin_top(10);
    layout.set_margin_bottom(10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);

    // App icon 32×32 — colored letter square
    let app_str = entry.app_name.as_deref().unwrap_or("App");
    let icon_area = crate::ui::letter_icon(app_str, 32);
    icon_area.set_valign(gtk::Align::Start);
    layout.append(&icon_area);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    // App name + timestamp row
    let meta_row = GtkBox::new(Orientation::Horizontal, 6);
    let app_name = Label::new(entry.app_name.as_deref().or(Some("App")));
    app_name.add_css_class("clipboard-time");
    app_name.set_xalign(0.0);
    app_name.set_hexpand(true);
    meta_row.append(&app_name);
    if let Some(ts) = &entry.timestamp {
        let ts_lbl = Label::new(Some(ts.as_str()));
        ts_lbl.add_css_class("notif-time");
        ts_lbl.set_xalign(1.0);
        meta_row.append(&ts_lbl);
    }
    text.append(&meta_row);

    // Summary (title)
    let title = Label::new(Some(&entry.summary));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    // Body
    if let Some(body) = &entry.body
        && !body.is_empty()
    {
        let body_lbl = Label::new(Some(body));
        body_lbl.add_css_class("result-subtitle");
        body_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        body_lbl.set_xalign(0.0);
        text.append(&body_lbl);
    }

    layout.append(&text);

    // Dismiss × button — closes the notification by id when available
    let dismiss = Button::with_label("×");
    dismiss.add_css_class("action-bar-btn");
    dismiss.add_css_class("kill-btn");
    dismiss.set_valign(gtk::Align::Start);
    if let Some(id) = entry.id {
        let row_weak = row.downgrade();
        dismiss.connect_clicked(move |_| {
            crate::close_notification(id);
            if let Some(row) = row_weak.upgrade() {
                row.set_visible(false);
            }
        });
    }
    layout.append(&dismiss);

    row.set_child(Some(&layout));
    row
}

