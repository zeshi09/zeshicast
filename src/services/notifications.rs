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
        dnd: load_persisted_dnd(),
        ..Default::default()
    });
}

const MAX_HISTORY: usize = 100;

/// Caps for untrusted notification text (M-14): any client on the session bus
/// could otherwise make the daemon hold (and render) megabytes per notification.
const MAX_APP_NAME_BYTES: usize = 1024;
const MAX_SUMMARY_BYTES: usize = 4 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Truncate `text` to at most `max_bytes`, never splitting a UTF-8 character.
fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// DND is the one piece of notification state that must survive daemon restarts,
/// so we persist it to a tiny file next to the rest of the config.
fn dnd_state_path() -> std::path::PathBuf {
    crate::home_dir().join(".config/zeshicast/dnd")
}

fn load_persisted_dnd() -> bool {
    std::fs::read_to_string(dnd_state_path())
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

fn persist_dnd(dnd: bool) {
    let path = dnd_state_path();
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        log::warn!("could not create {}: {error}", parent.display());
    }
    // Losing this only means do-not-disturb is not remembered across restarts,
    // so it is a warning and not an error.
    if let Err(error) = std::fs::write(&path, if dnd { "1" } else { "0" }) {
        log::warn!(
            "could not persist do-not-disturb to {}: {error}",
            path.display()
        );
    }
}

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
                app_name: truncate_utf8(app_name, MAX_APP_NAME_BYTES).to_string(),
                summary: truncate_utf8(summary, MAX_SUMMARY_BYTES).to_string(),
                body: truncate_utf8(body, MAX_BODY_BYTES).to_string(),
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
        persist_dnd(state.dnd);
        state.dnd
    })
}

/// Check whether Do-Not-Disturb is currently enabled.
pub fn is_dnd_enabled() -> bool {
    STATE.with(|state| state.borrow().dnd)
}

/// Marks the D-Bus server as active so the UI reports a working backend.
pub fn mark_server_active() {
    STATE.with(|state| state.borrow_mut().running = true);
}

/// Marks the D-Bus server as inactive (e.g. `org.freedesktop.Notifications`
/// was lost to another daemon) so the UI stops reporting a working backend.
pub fn mark_server_inactive() {
    STATE.with(|state| state.borrow_mut().running = false);
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
    fn notify_with_huge_body_is_capped() {
        mark_server_active();
        let app_name = "n".repeat(MAX_APP_NAME_BYTES + 10);
        let summary = "s".repeat(MAX_SUMMARY_BYTES + 10);
        let body = "b".repeat(MAX_BODY_BYTES + 10);

        push_notification(&app_name, &summary, &body, 0);
        let snapshot = notification_snapshot();
        let entry = &snapshot.history[0];

        assert_eq!(
            entry.app_name.as_deref().map(str::len),
            Some(MAX_APP_NAME_BYTES)
        );
        assert_eq!(entry.summary.len(), MAX_SUMMARY_BYTES);
        assert_eq!(
            entry.body.as_deref().map(str::len),
            Some(MAX_BODY_BYTES),
            "an untrusted client must not store unbounded text (M-14)"
        );
    }

    #[test]
    fn truncation_never_splits_a_utf8_character() {
        mark_server_active();
        // Every 'ё' is two bytes, so the byte cap lands inside a character.
        let summary = "ё".repeat(MAX_SUMMARY_BYTES);
        let body = "🙂".repeat(MAX_BODY_BYTES);

        push_notification("app", &summary, &body, 0);
        let snapshot = notification_snapshot();
        let entry = &snapshot.history[0];

        assert!(entry.summary.len() <= MAX_SUMMARY_BYTES);
        assert!(entry.summary.chars().all(|c| c == 'ё'), "split character");
        let body = entry.body.as_deref().unwrap_or_default();
        assert!(body.len() <= MAX_BODY_BYTES);
        assert!(body.chars().all(|c| c == '🙂'), "split character");
    }

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
    fn mark_server_inactive_clears_backend_indicator() {
        mark_server_active();
        push_notification("Mail", "New message", "Project update", 0);
        assert_eq!(
            notification_snapshot().backend.as_deref(),
            Some("zeshicast")
        );

        mark_server_inactive();

        let snapshot = notification_snapshot();
        assert_eq!(snapshot, NotificationSnapshot::default());
        assert!(!snapshot.is_available());
    }

    #[test]
    fn dnd_toggles() {
        let initial = is_dnd_enabled();
        let toggled = toggle_dnd();
        assert_eq!(toggled, !initial);
        assert_eq!(is_dnd_enabled(), toggled);
        let restored = toggle_dnd();
        assert_eq!(restored, initial);
        assert_eq!(is_dnd_enabled(), initial);
    }
}
