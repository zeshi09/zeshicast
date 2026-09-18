//! Logging for the daemon (P4.4).
//!
//! The daemon runs as a systemd user service, so its diagnostics belong in the
//! journal with a level attached rather than as bare prints. The filter is read
//! from `RUST_LOG` in the usual `target=level` form, e.g.
//!
//! ```text
//! RUST_LOG=zeshicast=debug           # our code at debug, everything else quiet
//! RUST_LOG=info,zeshicast::services=trace
//! ```
//!
//! There is no `env_logger` dependency: the whole subscriber is the level
//! lookup below plus one `log::Log` impl, and the planner is a pure function
//! that can be tested without touching the global logger.
//!
//! Note the deliberate boundary: this is for *daemon* diagnostics. CLI output
//! (`zeshicast <query>` printing results, `--export` reporting a destination)
//! stays on stdout with `println!`, because that is a program's output, not a
//! log record.

use std::io::Write;

/// Level used for a target with no matching filter entry.
const DEFAULT_LEVEL: log::LevelFilter = log::LevelFilter::Warn;
/// Level used for `zeshicast`-ish targets when `RUST_LOG` says nothing about
/// them: the daemon's own progress messages are worth having in the journal by
/// default.
const DEFAULT_OWN_LEVEL: log::LevelFilter = log::LevelFilter::Info;

/// Parse a `RUST_LOG` specification into `(target prefix, level)` pairs.
///
/// Entries that name an unknown level are dropped rather than guessed at: a
/// typo should not silence or flood the journal. A bare level (no `=`) applies
/// to every target.
pub(crate) fn parse_filters(spec: &str) -> Vec<(String, log::LevelFilter)> {
    let mut filters: Vec<(String, log::LevelFilter)> = Vec::new();

    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match entry.split_once('=') {
            Some((target, level)) => {
                if let Some(level) = parse_level(level.trim()) {
                    // Target names are compared case-sensitively, so normalise
                    // what the user wrote; `RUST_LOG=ZESHICAST=debug` should work
                    // just as well as the lowercase spelling.
                    filters.push((target.trim().to_ascii_lowercase(), level));
                }
            }
            None => {
                if let Some(level) = parse_level(entry) {
                    filters.push((String::new(), level));
                }
            }
        }
    }

    filters
}

fn parse_level(name: &str) -> Option<log::LevelFilter> {
    match name.to_ascii_lowercase().as_str() {
        "off" => Some(log::LevelFilter::Off),
        "error" => Some(log::LevelFilter::Error),
        "warn" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}

/// The level that applies to `target`.
///
/// The most specific matching prefix wins, so
/// `info,zeshicast::services=trace` keeps `trace` for the services and `info`
/// elsewhere. With no filters at all, our own code logs at
/// [`DEFAULT_OWN_LEVEL`] and everything else at [`DEFAULT_LEVEL`].
pub(crate) fn level_for(filters: &[(String, log::LevelFilter)], target: &str) -> log::LevelFilter {
    let mut best: Option<(usize, log::LevelFilter)> = None;

    for (prefix, level) in filters {
        let matches = if prefix.is_empty() {
            true
        } else {
            target == prefix || target.starts_with(&format!("{prefix}::"))
        };
        if matches && best.is_none_or(|(len, _)| prefix.len() >= len) {
            best = Some((prefix.len(), *level));
        }
    }

    match best {
        Some((_, level)) => level,
        // `zeshicast-extra` is a different target, not one of ours.
        None if is_own_target(target) => DEFAULT_OWN_LEVEL,
        None => DEFAULT_LEVEL,
    }
}

/// Whether `target` belongs to this crate (`zeshicast` or a submodule of it).
fn is_own_target(target: &str) -> bool {
    target == "zeshicast" || target.starts_with("zeshicast::")
}

struct JournalLogger {
    filters: Vec<(String, log::LevelFilter)>,
}

impl log::Log for JournalLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= level_for(&self.filters, metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // stderr, because systemd captures both streams into the journal and
        // stdout may be piped by whoever started us.
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "[{}] {}: {}",
            record.level(),
            record.target(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

static LOGGER: std::sync::OnceLock<JournalLogger> = std::sync::OnceLock::new();

/// Install the logger. Safe to call more than once and from either binary.
pub fn init() {
    let filters = parse_filters(&std::env::var("RUST_LOG").unwrap_or_default());

    let logger = LOGGER.get_or_init(|| {
        let logger = JournalLogger {
            filters: filters.clone(),
        };
        // Filtering happens in `enabled`, so the global ceiling stays open.
        log::set_max_level(log::LevelFilter::Trace);
        logger
    });

    let _ = log::set_logger(logger);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_rust_log_our_code_logs_at_info_and_others_at_warn() {
        let filters = parse_filters("");

        assert_eq!(level_for(&filters, "zeshicast"), log::LevelFilter::Info);
        assert_eq!(
            level_for(&filters, "zeshicast::services::media"),
            log::LevelFilter::Info
        );
        assert_eq!(level_for(&filters, "gtk"), log::LevelFilter::Warn);
        assert_eq!(level_for(&filters, ""), log::LevelFilter::Warn);
    }

    #[test]
    fn a_target_entry_applies_to_its_submodules() {
        let filters = parse_filters("zeshicast=debug");

        assert_eq!(
            level_for(&filters, "zeshicast::ui::launcher"),
            log::LevelFilter::Debug
        );
        // A prefix is not a match: `zeshicast-extra` is a different target.
        assert_eq!(
            level_for(&filters, "zeshicast-extra"),
            log::LevelFilter::Warn
        );
    }

    #[test]
    fn the_most_specific_filter_wins() {
        let filters = parse_filters("info,zeshicast::services=trace");

        assert_eq!(level_for(&filters, "gtk"), log::LevelFilter::Info);
        assert_eq!(
            level_for(&filters, "zeshicast::search"),
            log::LevelFilter::Info
        );
        assert_eq!(
            level_for(&filters, "zeshicast::services::network"),
            log::LevelFilter::Trace
        );
    }

    #[test]
    fn levels_are_case_insensitive_and_typos_are_ignored() {
        let filters = parse_filters("ZESHICAST=DeBuG,zeshicast::ui=shouting,extra");

        assert_eq!(filters.len(), 1, "got {filters:?}");
        assert_eq!(
            level_for(&filters, "zeshicast::ui"),
            log::LevelFilter::Debug
        );
    }

    #[test]
    fn off_silences_a_target() {
        let filters = parse_filters("off,zeshicast::services=error");

        assert_eq!(level_for(&filters, "gtk"), log::LevelFilter::Off);
        assert_eq!(
            level_for(&filters, "zeshicast::services::audio"),
            log::LevelFilter::Error
        );
    }
}
