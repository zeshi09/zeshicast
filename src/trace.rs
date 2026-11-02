//! Opt-in timing marks (Q2.1): `ZESHICAST_TRACE=1` logs how long the window
//! takes to appear, how long a keystroke takes to become results, how long
//! the indexes take to build, and the resident memory. Off by default; every
//! call is a cheap no-op then. Nothing leaves the machine: it is the log.
//!
//! The numbers feed `scripts/measure.sh` (Q2.2) and the renderer decision
//! (Q2.3); the `trace:` prefix keeps them greppable in the journal.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("ZESHICAST_TRACE")
            .is_ok_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
    })
}

fn process_start() -> &'static OnceLock<Instant> {
    static START: OnceLock<Instant> = OnceLock::new();
    &START
}

/// Call first thing in `main`: the reference point for "since start".
pub fn mark_process_start() {
    process_start().get_or_init(Instant::now);
}

/// Log `label` with the time since [`mark_process_start`].
pub fn since_start(label: &str) {
    if !enabled() {
        return;
    }
    if let Some(start) = process_start().get() {
        log::info!(
            "trace: {label} at {:.1} ms since start",
            ms(start.elapsed())
        );
    }
}

fn pending() -> &'static Mutex<HashMap<&'static str, Instant>> {
    static PENDING: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start an interval that [`end`] closes, possibly from another callback
/// (e.g. "show requested" → "first frame").
pub fn begin(name: &'static str) {
    if !enabled() {
        return;
    }
    if let Ok(mut pending) = pending().lock() {
        pending.insert(name, Instant::now());
    }
}

/// Close an interval opened by [`begin`] and log it. No-op without one.
pub fn end(name: &'static str) {
    if !enabled() {
        return;
    }
    let started = pending()
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(name));
    if let Some(started) = started {
        log::info!("trace: {name} took {:.1} ms", ms(started.elapsed()));
    }
}

/// Run `work`, logging how long it took when tracing is on.
pub fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    if !enabled() {
        return work();
    }
    let started = Instant::now();
    let result = work();
    log::info!("trace: {label} took {:.1} ms", ms(started.elapsed()));
    result
}

fn samples() -> &'static Mutex<HashMap<&'static str, Vec<Duration>>> {
    static SAMPLES: OnceLock<Mutex<HashMap<&'static str, Vec<Duration>>>> = OnceLock::new();
    SAMPLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Keep one latency sample of `series` for the [`summary`] at exit.
pub fn sample(series: &'static str, latency: Duration) {
    if !enabled() {
        return;
    }
    if let Ok(mut samples) = samples().lock() {
        samples.entry(series).or_default().push(latency);
    }
}

/// Log p50/p95/max per sampled series and the current RSS. Call on exit.
pub fn summary() {
    if !enabled() {
        return;
    }
    if let Ok(samples) = samples().lock() {
        let mut series: Vec<_> = samples.iter().collect();
        series.sort_by_key(|(name, _)| **name);
        for (name, values) in series {
            if let Some(stats) = latency_stats(values) {
                log::info!("trace: {name} {stats}");
            }
        }
    }
    log_rss("at exit");
}

/// Log the resident set size, e.g. when the palette goes idle.
pub fn log_rss(moment: &str) {
    if !enabled() {
        return;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    if let Some(kib) = vm_rss_kib(&status) {
        log::info!("trace: rss {moment} {:.1} MiB", kib as f64 / 1024.0);
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Nearest-rank percentile (`p` in 0..=100) of unsorted samples.
fn percentile(values: &[Duration], p: u32) -> Option<Duration> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let rank = (u64::from(p.min(100)) * sorted.len() as u64).div_ceil(100);
    let index = usize::try_from(rank.max(1) - 1).unwrap_or(0);
    sorted.get(index).copied()
}

/// `n=… p50=… ms p95=… ms max=… ms`, or `None` for no samples.
fn latency_stats(values: &[Duration]) -> Option<String> {
    Some(format!(
        "n={} p50={:.1} ms p95={:.1} ms max={:.1} ms",
        values.len(),
        ms(percentile(values, 50)?),
        ms(percentile(values, 95)?),
        ms(percentile(values, 100)?),
    ))
}

/// `VmRSS` from `/proc/<pid>/status`, in KiB.
fn vm_rss_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(values: &[u64]) -> Vec<Duration> {
        values
            .iter()
            .map(|value| Duration::from_millis(*value))
            .collect()
    }

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let values = millis(&[50, 10, 40, 20, 30, 60, 70, 80, 90, 100]);
        assert_eq!(percentile(&values, 50), Some(Duration::from_millis(50)));
        assert_eq!(percentile(&values, 95), Some(Duration::from_millis(100)));
        assert_eq!(percentile(&values, 100), Some(Duration::from_millis(100)));
        assert_eq!(percentile(&values, 0), Some(Duration::from_millis(10)));
        assert_eq!(
            percentile(&millis(&[7]), 95),
            Some(Duration::from_millis(7))
        );
        assert_eq!(percentile(&[], 50), None);
    }

    #[test]
    fn latency_stats_format_and_empty_series() {
        assert_eq!(
            latency_stats(&millis(&[10, 20, 30, 40])).as_deref(),
            Some("n=4 p50=20.0 ms p95=40.0 ms max=40.0 ms")
        );
        assert_eq!(latency_stats(&[]), None);
    }

    #[test]
    fn rss_is_read_from_proc_status() {
        let status =
            "Name:\tzeshicast-gtk\nVmPeak:\t  900000 kB\nVmRSS:\t   81234 kB\nThreads:\t9\n";
        assert_eq!(vm_rss_kib(status), Some(81_234));
        assert_eq!(vm_rss_kib("Name:\tx\n"), None);
        // The live file parses on Linux.
        let live = std::fs::read_to_string("/proc/self/status").expect("procfs");
        assert!(vm_rss_kib(&live).is_some_and(|kib| kib > 0));
    }

    #[test]
    fn disabled_tracing_still_runs_the_timed_work() {
        // ZESHICAST_TRACE is unset in tests: every call is a no-op except
        // that `timed` must still return the work's result.
        assert_eq!(timed("noop", || 41 + 1), 42);
        begin("never-ended");
        end("never-started");
        sample("noop", Duration::from_millis(1));
        summary();
    }
}
