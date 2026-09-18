#[derive(Debug, Clone, Default)]
pub struct MediaSnapshot {
    pub player: Option<String>,
    pub status: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub position_secs: Option<f64>,
    pub length_secs: Option<f64>,
}

impl MediaSnapshot {
    pub fn is_active(&self) -> bool {
        self.player.is_some() || self.title.is_some()
    }
}

/// A playback control routed to the active MPRIS player over D-Bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaControl {
    PlayPause,
    Next,
    Previous,
    Stop,
    /// Seek by a relative offset in microseconds (negative = backwards).
    SeekBy(i64),
}

pub fn media_snapshot() -> MediaSnapshot {
    #[cfg(feature = "desktop")]
    {
        mpris::snapshot().unwrap_or_default()
    }
    #[cfg(not(feature = "desktop"))]
    {
        // No gio in this build, so MPRIS is out of reach (P5.1). Silent: the
        // poller asks every second, and a log line per second is noise.
        MediaSnapshot::default()
    }
}

pub fn media_control(control: MediaControl) {
    #[cfg(feature = "desktop")]
    {
        mpris::control(control);
    }
    #[cfg(not(feature = "desktop"))]
    {
        // A control that does nothing looks like a broken player, so say why
        // instead of dropping it (P5.1).
        log::warn!("cannot send {control:?}: media control needs the `desktop` feature (or `gui`)");
    }
}

/// Direct MPRIS (org.mpris.MediaPlayer2) access over the session bus via gio —
/// no external `playerctl` dependency. Part of the `desktop` feature, not of the
/// widgets: the headless CLI controls players through this same path (P5.1).
#[cfg(feature = "desktop")]
mod mpris {
    use super::{MediaControl, MediaSnapshot};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use glib::variant::ToVariant;

    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
    const APP_IFACE: &str = "org.mpris.MediaPlayer2";
    const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
    const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

    /// Per-call budget: a player that is hung (or being stopped) answers in
    /// milliseconds or not at all, and the poller runs every second (M-2).
    const CALL_TIMEOUT_MS: i32 = 250;
    /// Consecutive timeouts after which a player is parked.
    const STUCK_AFTER: u32 = 2;
    /// How long a parked player is left alone before it is tried again.
    const STUCK_RECHECK: Duration = Duration::from_secs(30);

    #[derive(Debug, Default)]
    struct PlayerHealth {
        timeouts: u32,
        parked_until: Option<Instant>,
    }

    /// Per-player health, so one hung player cannot hold the poller hostage.
    ///
    /// `ListNames` still lists a hung player; asking it for `PlaybackStatus`
    /// would then burn the whole timeout on every tick. After `STUCK_AFTER`
    /// consecutive timeouts the player is skipped until `STUCK_RECHECK` elapses.
    #[derive(Debug, Default)]
    struct HealthBook {
        entries: HashMap<String, PlayerHealth>,
    }

    impl HealthBook {
        fn should_query(&self, player: &str, now: Instant) -> bool {
            match self
                .entries
                .get(player)
                .and_then(|entry| entry.parked_until)
            {
                Some(until) => now >= until,
                None => true,
            }
        }

        fn record(&mut self, player: &str, timed_out: bool, now: Instant) {
            let entry = self.entries.entry(player.to_string()).or_default();
            if !timed_out {
                *entry = PlayerHealth::default();
                return;
            }
            entry.timeouts += 1;
            if entry.timeouts >= STUCK_AFTER {
                entry.timeouts = 0;
                entry.parked_until = Some(now + STUCK_RECHECK);
            }
        }

        /// Forget players that are no longer on the bus.
        fn retain(&mut self, players: &[String]) {
            self.entries.retain(|name, _| players.contains(name));
        }
    }

    fn health_book() -> &'static Mutex<HealthBook> {
        static BOOK: OnceLock<Mutex<HealthBook>> = OnceLock::new();
        BOOK.get_or_init(|| Mutex::new(HealthBook::default()))
    }

    fn is_timeout(error: &glib::Error) -> bool {
        error.matches(gio::IOErrorEnum::TimedOut)
    }

    fn session_bus() -> Option<gio::DBusConnection> {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
    }

    /// All `org.mpris.MediaPlayer2.*` bus names currently present.
    fn list_players(conn: &gio::DBusConnection) -> Vec<String> {
        let Ok(reply) = conn.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        ) else {
            return Vec::new();
        };

        let names = reply.child_value(0);
        (0..names.n_children())
            .filter_map(|i| names.child_value(i).str().map(str::to_string))
            .filter(|name| name.starts_with(PREFIX))
            .collect()
    }

    /// Read one property and unbox its `v` wrapper.
    /// Read one property, unboxing its `v` wrapper. `Err` distinguishes a
    /// timeout (the player is hung) from a missing property, which the health
    /// book needs (M-2).
    fn get_prop(
        conn: &gio::DBusConnection,
        dest: &str,
        iface: &str,
        prop: &str,
    ) -> Result<Option<glib::Variant>, glib::Error> {
        let params = (iface, prop).to_variant();
        let reply = conn.call_sync(
            Some(dest),
            OBJECT_PATH,
            PROPS_IFACE,
            "Get",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
        )?;
        Ok(reply.child_value(0).as_variant())
    }

    /// MPRIS time values are spec'd as `x` (i64) but some players (Spotify) use
    /// `t` (u64) or `d` (f64) — accept any.
    fn variant_to_micros(value: &glib::Variant) -> Option<i64> {
        value
            .get::<i64>()
            .or_else(|| value.get::<u64>().map(|v| v as i64))
            .or_else(|| value.get::<f64>().map(|v| v as i64))
    }

    fn playback_status(
        conn: &gio::DBusConnection,
        dest: &str,
    ) -> Result<Option<String>, glib::Error> {
        Ok(get_prop(conn, dest, PLAYER_IFACE, "PlaybackStatus")?
            .and_then(|value| value.str().map(str::to_string)))
    }

    /// Pick the most relevant player: a Playing one first, otherwise the first
    /// that exists.
    fn pick_active(
        conn: &gio::DBusConnection,
        players: &[String],
        book: &mut HealthBook,
        now: Instant,
    ) -> Option<String> {
        let mut fallback = None;
        for dest in players {
            if !book.should_query(dest, now) {
                continue;
            }
            match playback_status(conn, dest) {
                Ok(status) => {
                    book.record(dest, false, now);
                    if status.as_deref() == Some("Playing") {
                        return Some(dest.clone());
                    }
                    if fallback.is_none() {
                        fallback = Some(dest.clone());
                    }
                }
                Err(error) => book.record(dest, is_timeout(&error), now),
            }
        }
        fallback
    }

    /// Friendly player name: the `Identity` property, else the bus-name suffix.
    fn display_name(conn: &gio::DBusConnection, dest: &str) -> String {
        if let Some(identity) = get_prop(conn, dest, APP_IFACE, "Identity")
            .ok()
            .flatten()
            .and_then(|v| {
                let s = v.str().map(str::to_string);
                s.filter(|s| !s.is_empty())
            })
        {
            return identity;
        }
        dest.strip_prefix(PREFIX).unwrap_or(dest).to_string()
    }

    pub fn snapshot() -> Option<MediaSnapshot> {
        let conn = session_bus()?;
        let players = list_players(&conn);
        let book = health_book();
        let mut book = book.lock().ok()?;
        book.retain(&players);
        let now = Instant::now();

        let dest = pick_active(&conn, &players, &mut book, now)?;

        let mut snapshot = MediaSnapshot {
            player: Some(display_name(&conn, &dest)),
            status: playback_status(&conn, &dest).ok().flatten(),
            ..Default::default()
        };

        let mut timed_out = false;
        match get_prop(&conn, &dest, PLAYER_IFACE, "Metadata") {
            Ok(Some(metadata)) => parse_metadata(&metadata, &mut snapshot),
            Ok(None) => {}
            Err(error) => timed_out |= is_timeout(&error),
        }
        match get_prop(&conn, &dest, PLAYER_IFACE, "Position") {
            Ok(position) => {
                snapshot.position_secs = position
                    .as_ref()
                    .and_then(variant_to_micros)
                    .filter(|&us| us >= 0)
                    .map(|us| us as f64 / 1_000_000.0);
            }
            Err(error) => timed_out |= is_timeout(&error),
        }
        book.record(&dest, timed_out, Instant::now());

        Some(snapshot)
    }

    /// Fill title/artist/album/art/length from an `a{sv}` MPRIS metadata dict.
    fn parse_metadata(metadata: &glib::Variant, snapshot: &mut MediaSnapshot) {
        for i in 0..metadata.n_children() {
            let entry = metadata.child_value(i);
            let key_variant = entry.child_value(0);
            let Some(key) = key_variant.str() else {
                continue;
            };
            let Some(value) = entry.child_value(1).as_variant() else {
                continue;
            };

            match key {
                "xesam:title" => snapshot.title = value.str().map(str::to_string),
                "xesam:album" => snapshot.album = value.str().map(str::to_string),
                "mpris:artUrl" => snapshot.art_url = value.str().map(str::to_string),
                "xesam:artist" | "xesam:albumArtist" if snapshot.artist.is_none() => {
                    // Array of strings — take the first non-empty.
                    for j in 0..value.n_children() {
                        if let Some(artist) = value.child_value(j).str()
                            && !artist.is_empty()
                        {
                            snapshot.artist = Some(artist.to_string());
                            break;
                        }
                    }
                }
                "mpris:length" => {
                    snapshot.length_secs = variant_to_micros(&value)
                        .filter(|&us| us > 0)
                        .map(|us| us as f64 / 1_000_000.0);
                }
                _ => {}
            }
        }
    }

    pub fn control(control: MediaControl) {
        let Some(conn) = session_bus() else { return };
        let players = list_players(&conn);
        let dest = {
            let book = health_book();
            let Ok(mut book) = book.lock() else { return };
            let now = Instant::now();
            book.retain(&players);
            let Some(dest) = pick_active(&conn, &players, &mut book, now) else {
                return;
            };
            dest
        };

        let (method, params) = control_call(control);

        let _ = conn.call_sync(
            Some(&dest),
            OBJECT_PATH,
            PLAYER_IFACE,
            method,
            params.as_ref(),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
        );
    }

    /// The D-Bus method a control maps to, and its arguments. Split out so the
    /// mapping is tested without a session bus (P5.1).
    fn control_call(control: MediaControl) -> (&'static str, Option<glib::Variant>) {
        match control {
            MediaControl::PlayPause => ("PlayPause", None),
            MediaControl::Next => ("Next", None),
            MediaControl::Previous => ("Previous", None),
            MediaControl::Stop => ("Stop", None),
            MediaControl::SeekBy(offset) => ("Seek", Some((offset,).to_variant())),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_control_maps_to_the_mpris_method_it_claims() {
            assert_eq!(control_call(MediaControl::PlayPause).0, "PlayPause");
            assert_eq!(control_call(MediaControl::Next).0, "Next");
            assert_eq!(control_call(MediaControl::Previous).0, "Previous");
            assert_eq!(control_call(MediaControl::Stop).0, "Stop");

            let (method, params) = control_call(MediaControl::SeekBy(-10_000_000));
            assert_eq!(method, "Seek");
            assert_eq!(
                params.expect("Seek carries an offset").get::<(i64,)>(),
                Some((-10_000_000,))
            );
        }

        #[test]
        fn stuck_player_is_skipped_after_two_timeouts() {
            let mut book = HealthBook::default();
            let now = Instant::now();
            let player = "org.mpris.MediaPlayer2.stuck";

            assert!(book.should_query(player, now));

            book.record(player, true, now);
            assert!(
                book.should_query(player, now),
                "a single timeout is not enough to park a player"
            );

            book.record(player, true, now);
            assert!(
                !book.should_query(player, now),
                "a player that timed out twice in a row is parked"
            );
            assert!(
                !book.should_query(player, now + STUCK_RECHECK - Duration::from_secs(1)),
                "still parked just before the recheck"
            );
            assert!(
                book.should_query(player, now + STUCK_RECHECK),
                "tried again once the park expires"
            );

            // A player that answers again starts from a clean slate.
            book.record(player, false, now);
            book.record(player, true, now);
            assert!(
                book.should_query(player, now),
                "a successful read clears the timeout history"
            );
        }

        #[test]
        fn players_that_left_the_bus_are_forgotten() {
            let mut book = HealthBook::default();
            let now = Instant::now();
            book.record("gone", true, now);
            book.retain(&[]);

            assert!(book.entries.is_empty());
        }
    }
}
