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
    #[cfg(feature = "gui")]
    {
        mpris::snapshot().unwrap_or_default()
    }
    #[cfg(not(feature = "gui"))]
    {
        MediaSnapshot::default()
    }
}

pub fn media_control(control: MediaControl) {
    #[cfg(feature = "gui")]
    {
        mpris::control(control);
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = control;
    }
}

/// Direct MPRIS (org.mpris.MediaPlayer2) access over the session bus via gio —
/// no external `playerctl` dependency. Only built for the GTK (`gui`) feature,
/// which is the sole consumer of media status.
#[cfg(feature = "gui")]
mod mpris {
    use super::{MediaControl, MediaSnapshot};
    use gtk::gio;
    use gtk::glib;
    use gtk::glib::variant::ToVariant;

    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
    const APP_IFACE: &str = "org.mpris.MediaPlayer2";
    const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
    const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

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
    fn get_prop(
        conn: &gio::DBusConnection,
        dest: &str,
        iface: &str,
        prop: &str,
    ) -> Option<glib::Variant> {
        let params = (iface, prop).to_variant();
        let reply = conn
            .call_sync(
                Some(dest),
                OBJECT_PATH,
                PROPS_IFACE,
                "Get",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .ok()?;
        reply.child_value(0).as_variant()
    }

    /// MPRIS time values are spec'd as `x` (i64) but some players (Spotify) use
    /// `t` (u64) or `d` (f64) — accept any.
    fn variant_to_micros(value: &glib::Variant) -> Option<i64> {
        value
            .get::<i64>()
            .or_else(|| value.get::<u64>().map(|v| v as i64))
            .or_else(|| value.get::<f64>().map(|v| v as i64))
    }

    fn playback_status(conn: &gio::DBusConnection, dest: &str) -> Option<String> {
        get_prop(conn, dest, PLAYER_IFACE, "PlaybackStatus")?
            .str()
            .map(str::to_string)
    }

    /// Pick the most relevant player: a Playing one first, otherwise the first
    /// that exists.
    fn pick_active(conn: &gio::DBusConnection) -> Option<String> {
        let players = list_players(conn);
        if let Some(playing) = players
            .iter()
            .find(|dest| playback_status(conn, dest).as_deref() == Some("Playing"))
        {
            return Some(playing.clone());
        }
        players.into_iter().next()
    }

    /// Friendly player name: the `Identity` property, else the bus-name suffix.
    fn display_name(conn: &gio::DBusConnection, dest: &str) -> String {
        if let Some(identity) = get_prop(conn, dest, APP_IFACE, "Identity").and_then(|v| {
            let s = v.str().map(str::to_string);
            s.filter(|s| !s.is_empty())
        }) {
            return identity;
        }
        dest.strip_prefix(PREFIX).unwrap_or(dest).to_string()
    }

    pub fn snapshot() -> Option<MediaSnapshot> {
        let conn = session_bus()?;
        let dest = pick_active(&conn)?;

        let mut snapshot = MediaSnapshot {
            player: Some(display_name(&conn, &dest)),
            status: playback_status(&conn, &dest),
            ..Default::default()
        };

        if let Some(metadata) = get_prop(&conn, &dest, PLAYER_IFACE, "Metadata") {
            parse_metadata(&metadata, &mut snapshot);
        }

        snapshot.position_secs = get_prop(&conn, &dest, PLAYER_IFACE, "Position")
            .as_ref()
            .and_then(variant_to_micros)
            .filter(|&us| us >= 0)
            .map(|us| us as f64 / 1_000_000.0);

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
                "xesam:artist" | "xesam:albumArtist" => {
                    // Array of strings — take the first non-empty.
                    if snapshot.artist.is_none() {
                        for j in 0..value.n_children() {
                            if let Some(artist) = value.child_value(j).str() {
                                if !artist.is_empty() {
                                    snapshot.artist = Some(artist.to_string());
                                    break;
                                }
                            }
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
        let Some(dest) = pick_active(&conn) else {
            return;
        };

        let (method, params) = match control {
            MediaControl::PlayPause => ("PlayPause", None),
            MediaControl::Next => ("Next", None),
            MediaControl::Previous => ("Previous", None),
            MediaControl::Stop => ("Stop", None),
            MediaControl::SeekBy(offset) => ("Seek", Some((offset,).to_variant())),
        };

        let _ = conn.call_sync(
            Some(&dest),
            OBJECT_PATH,
            PLAYER_IFACE,
            method,
            params.as_ref(),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        );
    }
}
