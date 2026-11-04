//! Background poller for the subprocess-heavy system snapshots.
//!
//! `network_snapshot()` forks `ip` (×2) and `nmcli` (×2); `audio_snapshot()`
//! forks `wpctl` (×3). Calling them from the GTK main loop every second means
//! ~7 `fork`+`exec`s per second on the UI thread, which blocks rendering and
//! input (visible micro-stutters) and drains the battery. This worker computes
//! them on a dedicated thread and publishes the latest result to a cache the
//! main thread reads cheaply (a mutex clone, no subprocesses). The cache starts
//! empty so launcher construction never waits for these tools.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use crate::services::audio::{AudioSnapshot, audio_snapshot};
use crate::services::battery::{BatterySnapshot, battery_snapshot};
use crate::services::compositor::{WorkspaceSnapshot, keyboard_layout, workspace_snapshot};
use crate::services::media::{MediaSnapshot, media_snapshot};
use crate::services::network::{NetworkSnapshot, network_snapshot};
use crate::services::system_stats::{
    ProcessSummary, SystemSnapshot, system_snapshot, top_processes_by_memory,
};
use crate::services::thermal::{ThermalSnapshot, thermal_snapshot};

/// How many processes the cached snapshot keeps; the views ask for 8.
const CACHED_PROCESSES: usize = 8;

#[derive(Default)]
struct Cache {
    network: NetworkSnapshot,
    audio: AudioSnapshot,
    keyboard_layout: Option<String>,
    media: MediaSnapshot,
    system: SystemSnapshot,
    processes: Vec<ProcessSummary>,
    workspace: WorkspaceSnapshot,
    thermal: ThermalSnapshot,
    battery: BatterySnapshot,
    metrics: crate::services::system_metrics::MetricsSnapshot,
}

static CACHE: OnceLock<Arc<Mutex<Cache>>> = OnceLock::new();
/// Cleared while the palette is hidden (P3.3): a hidden palette must not fork.
static ACTIVE: AtomicBool = AtomicBool::new(true);
static STARTED: OnceLock<()> = OnceLock::new();
/// `gpu_nvidia_smi`: run `nvidia-smi` for GPU load (Q7.3). Off by default:
/// it is slow to start and wakes a sleeping discrete GPU.
static NVIDIA_SMI: AtomicBool = AtomicBool::new(false);

pub fn set_nvidia_smi(enabled: bool) {
    NVIDIA_SMI.store(enabled, Ordering::Relaxed);
}

/// PSI, disk rates and GPUs; `previous` holds the last disk counters.
fn metrics_snapshot(
    previous: &mut Option<(
        std::time::Instant,
        Vec<crate::services::system_metrics::DiskCounters>,
    )>,
) -> crate::services::system_metrics::MetricsSnapshot {
    use crate::services::system_metrics as metrics;
    let now = std::time::Instant::now();
    let counters = metrics::read_diskstats(std::path::Path::new("/proc/diskstats"));
    let disks = previous
        .as_ref()
        .map(|(at, before)| metrics::disk_rates(before, &counters, now - *at))
        .unwrap_or_default();
    *previous = Some((now, counters));
    let mut gpus = metrics::read_gpus(std::path::Path::new("/sys/class/drm"));
    if NVIDIA_SMI.load(Ordering::Relaxed) {
        gpus.extend(metrics::nvidia_smi_gpus());
    }
    metrics::MetricsSnapshot {
        pressure: metrics::read_pressure(std::path::Path::new("/proc/pressure")),
        disks,
        gpus,
    }
}

/// Which slow sources a tick refreshes. Audio, layout and media every tick;
/// the forking ones less often (M-3). The first tick after the palette
/// shows refreshes everything, so nothing on screen is older than that tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickPlan {
    pub(crate) network: bool,
    pub(crate) system: bool,
    pub(crate) slow: bool,
    pub(crate) battery: bool,
}

pub(crate) fn tick_plan(tick: u64, just_shown: bool) -> TickPlan {
    TickPlan {
        network: just_shown || tick.is_multiple_of(3),
        system: just_shown || tick.is_multiple_of(2),
        slow: just_shown || tick.is_multiple_of(3),
        battery: just_shown || tick.is_multiple_of(3),
    }
}

/// A source a D-Bus signal said changed (L4): its next tick reads it even
/// when its turn has not come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dirty {
    Network,
    Battery,
}

static DIRTY_NETWORK: AtomicBool = AtomicBool::new(false);
static DIRTY_BATTERY: AtomicBool = AtomicBool::new(false);

/// Mark `source` changed and run a tick now. While the palette is hidden
/// the tick does nothing; showing it refreshes everything anyway (L2).
pub fn mark_dirty(source: Dirty) {
    match source {
        Dirty::Network => DIRTY_NETWORK.store(true, Ordering::Relaxed),
        Dirty::Battery => DIRTY_BATTERY.store(true, Ordering::Relaxed),
    }
    refresh_now();
}

/// NetworkManager signals worth a refresh: anything but access points,
/// which report signal strength every few seconds while scanning.
pub(crate) fn network_signal_matters(object_path: &str) -> bool {
    !object_path.starts_with("/org/freedesktop/NetworkManager/AccessPoint")
}

/// Subscribe to the D-Bus signals that say the strip is stale (L4):
/// NetworkManager and UPower on the system bus, MPRIS players on the
/// session bus. The connections live as long as the daemon.
pub fn subscribe_dbus_changes() {
    use gio::DBusSignalFlags;
    thread_local! {
        /// Dropping a subscription unsubscribes: keep them for the daemon's life.
        static SUBSCRIPTIONS: std::cell::RefCell<Vec<gio::SignalSubscription>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    let mut subscriptions = Vec::new();
    if let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        subscriptions.push(system.subscribe_to_signal(
            Some("org.freedesktop.NetworkManager"),
            None,
            None,
            None,
            None,
            DBusSignalFlags::NONE,
            |signal| {
                if network_signal_matters(signal.object_path) {
                    mark_dirty(Dirty::Network);
                }
            },
        ));
        subscriptions.push(system.subscribe_to_signal(
            Some("org.freedesktop.UPower"),
            None,
            None,
            None,
            None,
            DBusSignalFlags::NONE,
            |_| mark_dirty(Dirty::Battery),
        ));
        // The power profile rides with the battery (both the dashboard's).
        for name in [
            "org.freedesktop.UPower.PowerProfiles",
            "net.hadess.PowerProfiles",
        ] {
            subscriptions.push(system.subscribe_to_signal(
                Some(name),
                None,
                None,
                None,
                None,
                DBusSignalFlags::NONE,
                |_| mark_dirty(Dirty::Battery),
            ));
        }
    }
    if let Ok(session) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        // Media is read every tick; a player's change only needs the tick now.
        subscriptions.push(session.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some("/org/mpris/MediaPlayer2"),
            Some("org.mpris.MediaPlayer2.Player"),
            DBusSignalFlags::NONE,
            |_| refresh_now(),
        ));
    }
    SUBSCRIPTIONS.with(|kept| kept.borrow_mut().extend(subscriptions));
}

/// `plan` with the sources marked dirty since the last tick, clearing them.
fn with_dirty(mut plan: TickPlan) -> TickPlan {
    plan.network |= DIRTY_NETWORK.swap(false, Ordering::Relaxed);
    plan.battery |= DIRTY_BATTERY.swap(false, Ordering::Relaxed);
    plan
}

/// Wakes the poller when the palette shows (L2), instead of letting it
/// finish a sleep of up to a second first.
fn wake() -> &'static (Mutex<bool>, Condvar) {
    static WAKE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
    WAKE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// Sleep up to `timeout`, or less when [`set_active`] wakes the poller.
fn wait(timeout: Duration) {
    let (lock, condvar) = wake();
    let Ok(mut woken) = lock.lock() else {
        std::thread::sleep(timeout);
        return;
    };
    if !*woken {
        woken = match condvar.wait_timeout(woken, timeout) {
            Ok((woken, _)) => woken,
            Err(_) => return,
        };
    }
    *woken = false;
}

/// Bumped every time a value in the cache changes; the UI redraws when it
/// moves instead of on a fixed second (L2).
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Store one value as soon as it is read, so the cheap ones reach the
/// screen without waiting for the slow ones in the same tick.
fn publish(cache: &Mutex<Cache>, update: impl FnOnce(&mut Cache)) {
    if let Ok(mut cache) = cache.lock() {
        update(&mut cache);
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

/// Start the background poller. Idempotent — subsequent calls are no-ops.
pub fn start() {
    if STARTED.set(()).is_err() {
        return;
    }
    let cache = cache();
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        let target_interval = Duration::from_secs(1);
        let mut disk_counters = None;
        let mut was_active = false;
        loop {
            if !is_active() {
                // Hidden palette: keep the thread, do no work. The cached values
                // stay readable and showing the palette wakes this at once.
                was_active = false;
                wait(target_interval);
                continue;
            }
            let start = std::time::Instant::now();
            let plan = with_dirty(tick_plan(tick, !was_active));
            was_active = true;
            // Cheapest and most visible first: the strip shows them.
            let layout = keyboard_layout();
            publish(&cache, |c| c.keyboard_layout = layout);
            let audio = audio_snapshot();
            publish(&cache, |c| c.audio = audio);
            // MPRIS answers within `media`'s per-call budget, and a player that
            // keeps timing out is skipped there, so this stays cheap (M-2).
            let media = media_snapshot();
            publish(&cache, |c| c.media = media);
            if plan.battery {
                let battery = battery_snapshot();
                publish(&cache, |c| c.battery = battery);
            }
            if plan.network {
                let network = network_snapshot();
                publish(&cache, |c| c.network = network);
            }
            if plan.system {
                let system = system_snapshot();
                let processes = top_processes_by_memory(CACHED_PROCESSES);
                let metrics = metrics_snapshot(&mut disk_counters);
                publish(&cache, |c| {
                    c.system = system;
                    c.processes = processes;
                    c.metrics = metrics;
                });
            }
            if plan.slow {
                let workspace = workspace_snapshot();
                let thermal = thermal_snapshot();
                publish(&cache, |c| {
                    c.workspace = workspace;
                    c.thermal = thermal;
                });
            }
            tick = tick.wrapping_add(1);
            if let Some(remaining) = target_interval.checked_sub(start.elapsed()) {
                wait(remaining);
            }
        }
    });
}

/// New default levels from the PipeWire watcher (L3): the strip and views
/// show them on their next check, without waiting for a poll.
pub fn set_audio_levels(levels: &crate::services::audio_events::AudioLevels) {
    publish(&cache(), |cache| {
        let apply =
            |slot: &mut Option<crate::services::audio::AudioDeviceSnapshot>,
             level: &Option<crate::services::audio::AudioDeviceSnapshot>| {
                if let Some(level) = level {
                    match slot {
                        Some(current) => {
                            current.volume_percent = level.volume_percent;
                            current.muted = level.muted;
                        }
                        None => *slot = Some(level.clone()),
                    }
                }
            };
        apply(&mut cache.audio.output, &levels.output);
        apply(&mut cache.audio.input, &levels.input);
    });
}

/// Run a tick now (a volume key changed what the strip shows).
pub fn refresh_now() {
    let (lock, condvar) = wake();
    if let Ok(mut woken) = lock.lock() {
        *woken = true;
        condvar.notify_one();
    }
}

/// Pause or resume the poller (P3.3). Idempotent; resuming wakes it now.
pub fn set_active(active: bool) {
    let was = ACTIVE.swap(active, Ordering::Relaxed);
    if active && !was {
        let (lock, condvar) = wake();
        if let Ok(mut woken) = lock.lock() {
            *woken = true;
            condvar.notify_one();
        }
    }
}

/// Whether the poller is currently running its loop body.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

fn cache() -> Arc<Mutex<Cache>> {
    CACHE
        .get_or_init(|| Arc::new(Mutex::new(Cache::default())))
        .clone()
}

/// Latest PSI, disk and GPU metrics (Q7.3).
pub fn cached_metrics() -> crate::services::system_metrics::MetricsSnapshot {
    cache()
        .lock()
        .map(|cache| cache.metrics.clone())
        .unwrap_or_default()
}

/// Latest cached network snapshot (never blocks on a subprocess).
pub fn cached_network_snapshot() -> NetworkSnapshot {
    cache()
        .lock()
        .map(|c| c.network.clone())
        .unwrap_or_default()
}

/// Latest cached audio snapshot (never blocks on a subprocess).
pub fn cached_audio_snapshot() -> AudioSnapshot {
    cache().lock().map(|c| c.audio.clone()).unwrap_or_default()
}

/// Latest cached keyboard-layout code (e.g. "en"/"ru"), or `None` if unknown.
pub fn cached_keyboard_layout() -> Option<String> {
    cache().lock().ok().and_then(|c| c.keyboard_layout.clone())
}

/// Latest cached media snapshot (never blocks on a D-Bus call).
pub fn cached_media_snapshot() -> MediaSnapshot {
    cache().lock().map(|c| c.media.clone()).unwrap_or_default()
}

/// Latest cached CPU/memory/load snapshot (never forks `ps`).
pub fn cached_system_snapshot() -> SystemSnapshot {
    cache().lock().map(|c| c.system.clone()).unwrap_or_default()
}

/// Latest cached process list, by memory (never forks `ps`).
pub fn cached_top_processes() -> Vec<ProcessSummary> {
    cache()
        .lock()
        .map(|c| c.processes.clone())
        .unwrap_or_default()
}

/// Latest cached compositor workspace snapshot (never forks the compositor IPC).
pub fn cached_workspace_snapshot() -> WorkspaceSnapshot {
    cache()
        .lock()
        .map(|c| c.workspace.clone())
        .unwrap_or_default()
}

/// Latest cached thermal snapshot (never reads the thermal zones on the UI thread).
pub fn cached_thermal_snapshot() -> ThermalSnapshot {
    cache()
        .lock()
        .map(|c| c.thermal.clone())
        .unwrap_or_default()
}

/// Latest cached battery snapshot (never reads /sys on the UI thread).
pub fn cached_battery_snapshot() -> BatterySnapshot {
    cache()
        .lock()
        .map(|c| c.battery.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ACTIVE` is process-wide: tests that flip it take turns.
    static ACTIVE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn poller_can_be_paused_and_resumed() {
        let _turn = ACTIVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        assert!(is_active(), "the poller starts running");

        set_active(false);
        assert!(!is_active(), "a hidden palette pauses the poller");

        set_active(true);
        assert!(is_active(), "showing the palette resumes it");
    }

    #[test]
    fn the_first_tick_after_showing_refreshes_everything() {
        let all = TickPlan {
            network: true,
            system: true,
            slow: true,
            battery: true,
        };
        assert_eq!(tick_plan(7, true), all);
        assert_eq!(tick_plan(0, false), all);
        assert_eq!(
            tick_plan(1, false),
            TickPlan {
                network: false,
                system: false,
                slow: false,
                battery: false,
            }
        );
        assert_eq!(
            tick_plan(2, false),
            TickPlan {
                network: false,
                system: true,
                slow: false,
                battery: false,
            }
        );
    }

    #[test]
    fn access_point_chatter_does_not_refresh_the_network() {
        assert!(network_signal_matters("/org/freedesktop/NetworkManager"));
        assert!(network_signal_matters(
            "/org/freedesktop/NetworkManager/Devices/3"
        ));
        assert!(network_signal_matters(
            "/org/freedesktop/NetworkManager/ActiveConnection/7"
        ));
        assert!(!network_signal_matters(
            "/org/freedesktop/NetworkManager/AccessPoint/1204"
        ));
    }

    #[test]
    fn a_dirty_source_is_read_on_the_next_tick_once() {
        let _turn = ACTIVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        DIRTY_NETWORK.store(true, Ordering::Relaxed);
        let plan = with_dirty(tick_plan(1, false));
        assert!(plan.network && !plan.battery);
        assert!(
            !with_dirty(tick_plan(1, false)).network,
            "cleared after one tick"
        );
    }

    #[test]
    fn showing_the_palette_wakes_a_sleeping_poller() {
        let _turn = ACTIVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_active(false);
        let waiter = std::thread::spawn(|| {
            let start = std::time::Instant::now();
            wait(Duration::from_secs(5));
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        set_active(true);
        let slept = waiter.join().unwrap();
        assert!(slept < Duration::from_secs(1), "woke after {slept:?}");
    }
}
