//! Background poller for the subprocess-heavy system snapshots.
//!
//! `network_snapshot()` forks `ip` (×2) and `nmcli` (×2); `audio_snapshot()`
//! forks `wpctl` (×3). Calling them from the GTK main loop every second means
//! ~7 `fork`+`exec`s per second on the UI thread, which blocks rendering and
//! input (visible micro-stutters) and drains the battery. This worker computes
//! them on a dedicated thread and publishes the latest result to a cache the
//! main thread reads cheaply (a mutex clone, no subprocesses). The cache starts
//! empty so launcher construction never waits for these tools.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
}

static CACHE: OnceLock<Arc<Mutex<Cache>>> = OnceLock::new();
/// Cleared while the palette is hidden (P3.3): a hidden palette must not fork.
static ACTIVE: AtomicBool = AtomicBool::new(true);
static STARTED: OnceLock<()> = OnceLock::new();

/// Start the background poller. Idempotent — subsequent calls are no-ops.
pub fn start() {
    if STARTED.set(()).is_err() {
        return;
    }
    let cache = cache();
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        let target_interval = Duration::from_secs(1);
        loop {
            if !is_active() {
                // Hidden palette: keep the thread, do no work. The cached values
                // stay readable and the first visible tick refreshes them.
                std::thread::sleep(target_interval);
                continue;
            }
            let start = std::time::Instant::now();
            // Audio reacts to volume keys and the keyboard layout to the switch
            // hotkey, so refresh both every tick. Network state rarely changes,
            // so refresh it less often to save power.
            let audio = audio_snapshot();
            let layout = keyboard_layout();
            let network = tick.is_multiple_of(3).then(network_snapshot);
            // MPRIS answers within `media`'s per-call budget, and a player that
            // keeps timing out is skipped there, so this stays cheap (M-2).
            let media = media_snapshot();
            // These fork (ps/ip/thermal zones/compositor): every other tick is
            // enough for a dashboard nobody is watching continuously (M-3).
            let system = tick.is_multiple_of(2).then(system_snapshot);
            let processes = tick
                .is_multiple_of(2)
                .then(|| top_processes_by_memory(CACHED_PROCESSES));
            let workspace = tick.is_multiple_of(3).then(workspace_snapshot);
            let thermal = tick.is_multiple_of(3).then(thermal_snapshot);
            let battery = tick.is_multiple_of(3).then(battery_snapshot);
            if let Ok(mut cache) = cache.lock() {
                cache.audio = audio;
                cache.keyboard_layout = layout;
                cache.media = media;
                if let Some(network) = network {
                    cache.network = network;
                }
                if let Some(system) = system {
                    cache.system = system;
                }
                if let Some(processes) = processes {
                    cache.processes = processes;
                }
                if let Some(workspace) = workspace {
                    cache.workspace = workspace;
                }
                if let Some(thermal) = thermal {
                    cache.thermal = thermal;
                }
                if let Some(battery) = battery {
                    cache.battery = battery;
                }
            }
            tick = tick.wrapping_add(1);
            let elapsed = start.elapsed();
            if let Some(remaining) = target_interval.checked_sub(elapsed) {
                std::thread::sleep(remaining);
            }
        }
    });
}

/// Pause or resume the poller (P3.3). Idempotent.
pub fn set_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
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

    #[test]
    fn poller_can_be_paused_and_resumed() {
        assert!(is_active(), "the poller starts running");

        set_active(false);
        assert!(!is_active(), "a hidden palette pauses the poller");

        set_active(true);
        assert!(is_active(), "showing the palette resumes it");
    }
}
