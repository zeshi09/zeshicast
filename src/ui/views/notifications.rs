use crate::services::notifications::NotificationSnapshot;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation};

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
    history.connect_row_activated(|_, row| {
        dismiss_notification_row(row);
    });

    // The empty state is the list's placeholder: centred in the list area,
    // instead of a second expanding widget pushed below an empty list.
    let message = Label::new(Some("No notifications"));
    message.add_css_class("result-subtitle");
    message.set_xalign(0.5);
    message.set_valign(gtk::Align::Center);
    message.set_vexpand(true);
    history.set_placeholder(Some(&message));

    let scroller = super::scrollable_list(&history);
    root.append(&scroller);

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
    // The backend is zeshicast itself; what the user wants here is how many.
    let count = snapshot.history.len();
    view.backend.set_text(&match count {
        0 => String::new(),
        1 => "1 notification".to_string(),
        n => format!("{n} notifications"),
    });
    view.close_all.set_sensitive(count > 0);

    let dnd_on = snapshot.dnd.unwrap_or(false);
    view.dnd.set_text(if dnd_on { "DND On" } else { "" });
    view.dnd.set_visible(dnd_on);
    if dnd_on {
        view.dnd.add_css_class("active");
    } else {
        view.dnd.remove_css_class("active");
    }

    set_notification_history_rows(&view.history, snapshot);

    if snapshot.history.is_empty() {
        view.message.set_text(if snapshot.is_available() {
            "No notifications"
        } else {
            "Notifications go to another daemon, so there is no history here"
        });
    }
}

fn set_notification_history_rows(list: &ListBox, snapshot: &NotificationSnapshot) {
    // Rows only: removing every child also took the empty-state placeholder.
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
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
    let icon_area = super::letter_icon(app_str, 32);
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
    dismiss.set_tooltip_text(Some("Dismiss notification"));
    if let Some(id) = entry.id {
        row.set_widget_name(&format!("notif:{id}"));
        let row_weak = row.downgrade();
        dismiss.connect_clicked(move |_| {
            if let Some(row) = row_weak.upgrade() {
                dismiss_notification_row(&row);
            }
        });
    }
    layout.append(&dismiss);

    row.set_child(Some(&layout));
    row
}

pub fn dismiss_notification_row(row: &ListBoxRow) {
    if let Some(id_str) = row.widget_name().strip_prefix("notif:")
        && let Ok(id) = id_str.parse::<u32>()
    {
        crate::services::notifications::close_notification(id);
        row.set_visible(false);
    }
}
