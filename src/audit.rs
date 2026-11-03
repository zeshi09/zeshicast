//! Extension permission audit log (Q5.4): what an extension asked to do, and
//! whether it was granted, denied, confirmed or cancelled.
//!
//! Rows are written from the execution gateway (`action::execute`), from the
//! confirmation panel when the user cancels, and from an extension's
//! `execute` reply whose effects are dropped. Only actions that came from an
//! extension are logged; the user's own commands are not. Private mode does
//! not turn the log off: it is a security record, not usage history.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Rows kept in the database; the oldest go first, like clipboard history.
pub(crate) const AUDIT_RETENTION: usize = 1000;

/// Longest `detail` stored; a command line or URL is cut, not dropped.
const DETAIL_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Allowed without asking (no confirmation needed for its risk).
    Granted,
    /// Refused: the extension lacks the capability.
    Denied,
    /// Allowed after the user confirmed it.
    Confirmed,
    /// The user dismissed the confirmation.
    Cancelled,
}

impl Outcome {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Confirmed => "confirmed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    pub(crate) extension: String,
    pub(crate) capability: String,
    pub(crate) detail: String,
    pub(crate) outcome: Outcome,
}

static AUDIT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where rows go. Set once when the app loads; without it nothing is written
/// (a headless helper that never ran an extension).
pub(crate) fn init(config_dir: &Path) {
    let _ = AUDIT_DIR.set(config_dir.to_path_buf());
}

pub(crate) fn record(extension: &str, capability: &str, detail: &str, outcome: Outcome) {
    let record = Record {
        extension: extension.to_string(),
        capability: capability.to_string(),
        detail: shorten(detail),
        outcome,
    };
    // Tests never call `init`, so they only see the thread-local copy.
    if let Some(dir) = AUDIT_DIR.get()
        && let Err(error) = crate::services::storage::audit_insert(
            dir,
            &record.extension,
            &record.capability,
            &record.detail,
            record.outcome.label(),
            AUDIT_RETENTION,
        )
    {
        log::warn!("could not write the extension audit log: {error}");
    }
    #[cfg(test)]
    TEST_LOG.with(|log| log.borrow_mut().push(record));
}

/// Newest rows for one extension (or all), for the Extension Browser.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub(crate) fn recent(
    extension: Option<&str>,
    limit: usize,
) -> Vec<crate::services::storage::AuditRow> {
    AUDIT_DIR
        .get()
        .map(|dir| crate::services::storage::audit_recent(dir, extension, limit))
        .unwrap_or_default()
}

/// Title and subtitle of one log row as the Extension Browser shows it.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub(crate) fn describe(row: &crate::services::storage::AuditRow, age: &str) -> (String, String) {
    let capability = if row.capability.is_empty() {
        "no capability"
    } else {
        row.capability.as_str()
    };
    let title = format!("{} · {} · {}", row.extension, capability, row.outcome);
    let subtitle = if row.detail.is_empty() {
        age.to_string()
    } else {
        format!("{age} · {}", row.detail)
    };
    (title, subtitle)
}

fn shorten(detail: &str) -> String {
    let detail = detail.replace(['\n', '\r'], " ");
    match detail.char_indices().nth(DETAIL_LIMIT) {
        Some((cut, _)) => format!("{}…", &detail[..cut]),
        None => detail,
    }
}

#[cfg(test)]
thread_local! {
    static TEST_LOG: std::cell::RefCell<Vec<Record>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Rows recorded on this thread since the last call (tests only).
#[cfg(test)]
pub(crate) fn take_test_log() -> Vec<Record> {
    TEST_LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_row_names_the_extension_capability_and_outcome() {
        let row = crate::services::storage::AuditRow {
            extension: "you.git-tools".to_string(),
            capability: "shell".to_string(),
            detail: "git push".to_string(),
            outcome: "cancelled".to_string(),
            at: 0,
        };
        let (title, subtitle) = describe(&row, "2 min ago");
        assert_eq!(title, "you.git-tools · shell · cancelled");
        assert_eq!(subtitle, "2 min ago · git push");
    }

    #[test]
    fn long_details_are_cut_to_one_line() {
        let detail = format!("a\nb{}", "x".repeat(500));
        let short = shorten(&detail);
        assert!(!short.contains('\n'));
        assert_eq!(short.chars().count(), DETAIL_LIMIT + 1);
        assert!(short.ends_with('…'));
    }
}
