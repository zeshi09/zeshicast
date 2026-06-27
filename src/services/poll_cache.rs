//! Background poller for the subprocess-heavy system snapshots.
//!
//! `network_snapshot()` forks `ip` (×2) and `nmcli` (×2); `audio_snapshot()`
//! forks `wpctl` (×3). Calling them from the GTK main loop every second means
//! ~7 `fork`+`exec`s per second on the UI thread, which blocks rendering and
//! input (visible micro-stutters) and drains the battery. This worker computes
//! them on a dedicated thread and publishes the latest result to a cache the
//! main thread reads cheaply (a mutex clone, no subprocesses). The cache starts
//! empty so launcher construction never waits for these tools.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::{AudioSnapshot, NetworkSnapshot, audio_snapshot, keyboard_layout, network_snapshot};

#[derive(Default)]
struct Cache {
    network: NetworkSnapshot,
    audio: AudioSnapshot,
    keyboard_layout: Option<String>,
}

static CACHE: OnceLock<Arc<Mutex<Cache>>> = OnceLock::new();
static STARTED: OnceLock<()> = OnceLock::new();

/// Start the background poller. Idempotent — subsequent calls are no-ops.
pub fn start() {
    if STARTED.set(()).is_err() {
        return;
    }
    let cache = cache();
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        loop {
            // Audio reacts to volume keys and the keyboard layout to the switch
            // hotkey, so refresh both every tick. Network state rarely changes,
            // so refresh it less often to save power.
            let audio = audio_snapshot();
            let layout = keyboard_layout();
            let network = tick.is_multiple_of(3).then(network_snapshot);
            if let Ok(mut cache) = cache.lock() {
                cache.audio = audio;
                cache.keyboard_layout = layout;
                if let Some(network) = network {
                    cache.network = network;
                }
            }
            tick = tick.wrapping_add(1);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
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
