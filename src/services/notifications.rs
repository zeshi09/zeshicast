use std::sync::{Mutex, OnceLock};

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
// external daemon (swaync/dunst) is involved.
//
// The store is process-global, not thread-local: extension failures and HTTP
// errors are reported from worker threads (`action.rs`), and a thread-local
// store would silently drop those notifications (N-6). A `Mutex` keeps every
// caller on one state no matter which thread it runs on.

#[derive(Clone)]
struct StoredNotification {
    id: u32,
    app_name: String,
    summary: String,
    body: String,
    received_at: u64,
    /// The D-Bus sender that created it, when it came from the bus. Internal
    /// pushes have no sender and may act on anything.
    sender: Option<String>,
}

#[derive(Default)]
struct NotificationState {
    entries: Vec<StoredNotification>,
    dnd: bool,
    next_id: u32,
    running: bool,
}

fn state() -> &'static Mutex<NotificationState> {
    static STATE: OnceLock<Mutex<NotificationState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(NotificationState {
            next_id: 1,
            dnd: load_persisted_dnd(),
            ..Default::default()
        })
    })
}

/// Lock the store, recovering the data if a previous holder panicked: a poisoned
/// notification store is still readable, and aborting on it is worse.
fn lock_state() -> std::sync::MutexGuard<'static, NotificationState> {
    state().lock().unwrap_or_else(|poison| poison.into_inner())
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
    crate::config::home_dir().join(".config/zeshicast/dnd")
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
    push_notification_from(app_name, summary, body, replaces_id, None)
}

/// [`push_notification`] with the D-Bus sender of the caller.
///
/// A session-bus peer must only be able to replace its *own* notification: with
/// the sender on the entry, `replaces_id` from a different client is treated as
/// a brand-new notification instead of a hijack (Low finding, notify_server).
pub fn push_notification_from(
    app_name: &str,
    summary: &str,
    body: &str,
    replaces_id: u32,
    sender: Option<&str>,
) -> u32 {
    let mut state = lock_state();
    let id = if replaces_id != 0 && can_own(&state, replaces_id, sender) {
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
            sender: sender.map(str::to_string),
        },
    );
    state.entries.truncate(MAX_HISTORY);
    id
}

/// Whether `sender` may act on the notification with `id`: only its creator, or
/// an internal caller (`None`).
fn can_own(state: &NotificationState, id: u32, sender: Option<&str>) -> bool {
    match state.entries.iter().find(|entry| entry.id == id) {
        Some(entry) => sender.is_none() || entry.sender.as_deref() == sender,
        None => true,
    }
}

pub fn close_notification(id: u32) {
    close_notification_from(id, None);
}

/// [`close_notification`] that refuses to close another sender's notification.
pub fn close_notification_from(id: u32, sender: Option<&str>) {
    let mut state = lock_state();
    if can_own(&state, id, sender) {
        state.entries.retain(|entry| entry.id != id);
    }
}

pub fn clear_notifications() {
    lock_state().entries.clear();
}

/// Flip Do-Not-Disturb; returns the new state.
pub fn toggle_dnd() -> bool {
    let mut state = lock_state();
    state.dnd = !state.dnd;
    persist_dnd(state.dnd);
    state.dnd
}

/// Check whether Do-Not-Disturb is currently enabled.
pub fn is_dnd_enabled() -> bool {
    lock_state().dnd
}

/// Marks the D-Bus server as active so the UI reports a working backend.
pub fn mark_server_active() {
    lock_state().running = true;
}

/// Marks the D-Bus server as inactive (e.g. `org.freedesktop.Notifications`
/// was lost to another daemon) so the UI stops reporting a working backend.
pub fn mark_server_inactive() {
    lock_state().running = false;
}

pub fn notification_snapshot() -> NotificationSnapshot {
    let state = lock_state();
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

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Notification state is process-global, so tests that observe counts or
    /// history must serialize and start from a clean store.
    fn isolated() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut state = lock_state();
        state.entries.clear();
        state.next_id = 1;
        state.running = false;
        drop(state);
        guard
    }

    #[test]
    fn notify_with_huge_body_is_capped() {
        let _guard = isolated();
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
        let _guard = isolated();
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
        let _guard = isolated();
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
        let _guard = isolated();
        let id = push_notification("App", "First", "", 0);
        let same = push_notification("App", "Second", "", id);
        assert_eq!(id, same);
        mark_server_active();
        assert_eq!(notification_snapshot().count, Some(1));
        assert_eq!(notification_snapshot().history[0].summary, "Second");
    }

    #[test]
    fn mark_server_inactive_clears_backend_indicator() {
        let _guard = isolated();
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
        let _guard = isolated();
        let initial = is_dnd_enabled();
        let toggled = toggle_dnd();
        assert_eq!(toggled, !initial);
        assert_eq!(is_dnd_enabled(), toggled);
        let restored = toggle_dnd();
        assert_eq!(restored, initial);
        assert_eq!(is_dnd_enabled(), initial);
    }

    /// N-6: extension failures and HTTP errors are reported from worker threads.
    /// With a thread-local store the push landed on the worker's own state and
    /// the main thread never saw it.
    #[test]
    fn a_worker_thread_push_is_visible_from_another_thread() {
        let _guard = isolated();
        mark_server_active();

        std::thread::spawn(|| push_notification("Worker", "from a thread", "body", 0))
            .join()
            .expect("worker thread");

        let snapshot = notification_snapshot();
        assert_eq!(snapshot.count, Some(1));
        assert_eq!(snapshot.history[0].summary, "from a thread");
    }

    #[test]
    fn another_sender_cannot_replace_or_close_a_notification() {
        let _guard = isolated();
        mark_server_active();

        let first = push_notification_from("A", "one", "", 0, Some(":1.1"));
        // A different client asking to replace that id gets a new notification
        // instead of overwriting someone else's (ownership check).
        let second = push_notification_from("B", "two", "", first, Some(":1.2"));
        assert_ne!(first, second);
        assert_eq!(notification_snapshot().count, Some(2));

        // And it cannot close the other client's notification either.
        close_notification_from(first, Some(":1.2"));
        assert_eq!(notification_snapshot().count, Some(2));
        // The owner still can.
        close_notification_from(second, Some(":1.2"));
        assert_eq!(notification_snapshot().count, Some(1));
    }
}
