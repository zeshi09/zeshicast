use std::cell::RefCell;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub backend: Option<String>,
    pub count: Option<u32>,
    pub dnd: Option<bool>,
    pub history: Vec<NotificationEntrySnapshot>,
}

impl NotificationSnapshot {
    pub fn is_available(&self) -> bool {
        self.backend.is_some()
    }
}

/// A notification command-center action routed to our own store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAction {
    ToggleDnd,
    ClearAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEntrySnapshot {
    pub id: Option<u32>,
    pub app_name: Option<String>,
    pub summary: String,
    pub body: Option<String>,
    pub timestamp: Option<String>,
}

// ── Internal store ───────────────────────────────────────────────────────────
//
// zeshicast is the notification daemon: it owns `org.freedesktop.Notifications`
// (see ui/notify_server.rs) and records every incoming notification here. No
// external daemon (swaync/dunst) is involved. Everything lives on the GLib main
// thread, so a thread-local store is enough.

#[derive(Clone)]
struct StoredNotification {
    id: u32,
    app_name: String,
    summary: String,
    body: String,
    received_at: u64,
}

#[derive(Default)]
struct NotificationState {
    entries: Vec<StoredNotification>,
    dnd: bool,
    next_id: u32,
    running: bool,
}

thread_local! {
    static STATE: RefCell<NotificationState> = RefCell::new(NotificationState {
        next_id: 1,
        ..Default::default()
    });
}

const MAX_HISTORY: usize = 100;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Record an incoming notification. Reuses `replaces_id` when non-zero (the
/// notification spec's replacement semantics); returns the resolved id.
pub fn push_notification(app_name: &str, summary: &str, body: &str, replaces_id: u32) -> u32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            let id = state.next_id;
            state.next_id = state.next_id.checked_add(1).unwrap_or(1);
            id
        };
        state.entries.retain(|entry| entry.id != id);
        state.entries.insert(
            0,
            StoredNotification {
                id,
                app_name: app_name.to_string(),
                summary: summary.to_string(),
                body: body.to_string(),
                received_at: now_secs(),
            },
        );
        state.entries.truncate(MAX_HISTORY);
        id
    })
}

pub fn close_notification(id: u32) {
    STATE.with(|state| state.borrow_mut().entries.retain(|entry| entry.id != id));
}

pub fn clear_notifications() {
    STATE.with(|state| state.borrow_mut().entries.clear());
}

/// Flip Do-Not-Disturb; returns the new state.
pub fn toggle_dnd() -> bool {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.dnd = !state.dnd;
        state.dnd
    })
}

/// Marks the D-Bus server as active so the UI reports a working backend.
pub fn mark_server_active() {
    STATE.with(|state| state.borrow_mut().running = true);
}

pub fn notification_snapshot() -> NotificationSnapshot {
    STATE.with(|state| {
        let state = state.borrow();
        if !state.running {
            return NotificationSnapshot::default();
        }
        NotificationSnapshot {
            backend: Some("zeshicast".to_string()),
            count: Some(state.entries.len() as u32),
            dnd: Some(state.dnd),
            history: state
                .entries
                .iter()
                .map(|entry| NotificationEntrySnapshot {
                    id: Some(entry.id),
                    app_name: non_empty_string(&entry.app_name),
                    summary: entry.summary.clone(),
                    body: non_empty_string(&entry.body),
                    timestamp: Some(format_notif_time(entry.received_at)),
                })
                .collect(),
        }
    })
}

fn format_notif_time(unix_secs: u64) -> String {
    let diff = now_secs().saturating_sub(unix_secs);
    if diff < 60 {
        "now".to_string()
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86400)
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_records_newest_first_and_closes() {
        mark_server_active();
        let _first = push_notification("Mail", "New message", "Project update", 0);
        let second = push_notification("Chat", "Hi there", "", 0);

        let snapshot = notification_snapshot();
        assert_eq!(snapshot.backend.as_deref(), Some("zeshicast"));
        assert_eq!(snapshot.count, Some(2));
        assert_eq!(snapshot.history[0].summary, "Hi there");
        assert_eq!(snapshot.history[0].body, None);
        assert_eq!(snapshot.history[1].app_name.as_deref(), Some("Mail"));

        close_notification(second);
        let snapshot = notification_snapshot();
        assert_eq!(snapshot.count, Some(1));
        assert_eq!(snapshot.history[0].summary, "New message");
    }

    #[test]
    fn replaces_id_updates_in_place() {
        let id = push_notification("App", "First", "", 0);
        let same = push_notification("App", "Second", "", id);
        assert_eq!(id, same);
        mark_server_active();
        assert_eq!(notification_snapshot().count, Some(1));
        assert_eq!(notification_snapshot().history[0].summary, "Second");
    }

    #[test]
    fn dnd_toggles() {
        assert!(toggle_dnd());
        assert!(!toggle_dnd());
    }
}
