// The palette is the only consumer of these readers: the headless build cannot
// reach them, which is not the same as them being dead. Gating each one on
// `gui` would scatter `#[cfg]` over a dozen modules; P5.5 tracks shrinking the
// set instead.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod audio;
#[cfg(feature = "gui")]
pub mod audio_events;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod battery;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod bluetooth;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod brightness;
pub(crate) mod clipboard_formats;
pub mod clipboard_store;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod compositor;
pub(crate) mod crypto;
pub(crate) mod db_key;
pub mod extension_protocol;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod local_ai;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod media;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod network;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod notifications;
#[cfg(feature = "gui")]
pub mod poll_cache;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod power;
pub mod storage;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod system_metrics;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod system_stats;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod systemd_units;
pub mod terminal;
pub mod text_input;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod thermal;
