use std::collections::HashMap;
use std::process::Command;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

#[derive(Debug, Clone, Default)]
pub struct MediaSnapshot {
    pub player: Option<String>,
    pub status: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
}

impl MediaSnapshot {
    pub fn is_active(&self) -> bool {
        self.player.is_some() || self.title.is_some()
    }
}

pub fn media_snapshot() -> MediaSnapshot {
    if let Some(snapshot) = mpris_dbus_snapshot() {
        if snapshot.is_active() {
            return snapshot;
        }
    }
    playerctl_snapshot()
}

pub fn mpris_dbus_snapshot() -> Option<MediaSnapshot> {
    let connection = Connection::session().ok()?;
    let dbus_proxy = zbus::blocking::fdo::DBusProxy::new(&connection).ok()?;
    let names = dbus_proxy.list_names().ok()?;

    let mpris_name = names
        .into_iter()
        .find(|name| name.starts_with("org.mpris.MediaPlayer2."))?;
    let player_name = mpris_name
        .strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or(&mpris_name)
        .to_string();

    let player_proxy = zbus::blocking::Proxy::new(
        &connection,
        mpris_name.as_str(),
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .ok()?;

    let status: Option<String> = player_proxy.get_property("PlaybackStatus").ok();
    let metadata: Option<HashMap<String, Value>> = player_proxy.get_property("Metadata").ok();

    let mut title = None;
    let mut artist = None;

    if let Some(meta) = metadata {
        if let Some(Value::Str(t)) = meta.get("xesam:title") {
            title = Some(t.to_string());
        }
        if let Some(Value::Array(arr)) = meta.get("xesam:artist") {
            let artists: Vec<String> = arr
                .iter()
                .filter_map(|v| {
                    if let Value::Str(s) = v {
                        Some(s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if !artists.is_empty() {
                artist = Some(artists.join(", "));
            }
        } else if let Some(Value::Str(a)) = meta.get("xesam:artist") {
            artist = Some(a.to_string());
        }
    }

    Some(MediaSnapshot {
        player: Some(player_name),
        status,
        artist,
        title,
    })
}

#[allow(dead_code)]
pub fn mpris_dbus_command(command: &str) -> bool {
    let Ok(connection) = Connection::session() else {
        return false;
    };
    let Ok(dbus_proxy) = zbus::blocking::fdo::DBusProxy::new(&connection) else {
        return false;
    };
    let Ok(names) = dbus_proxy.list_names() else {
        return false;
    };

    let Some(mpris_name) = names
        .into_iter()
        .find(|name| name.starts_with("org.mpris.MediaPlayer2."))
    else {
        return false;
    };

    let Ok(player_proxy) = zbus::blocking::Proxy::new(
        &connection,
        mpris_name.as_str(),
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    ) else {
        return false;
    };

    let method_name = match command {
        "play_pause" | "PlayPause" => "PlayPause",
        "next" | "Next" => "Next",
        "previous" | "Previous" => "Previous",
        "stop" | "Stop" => "Stop",
        "play" | "Play" => "Play",
        "pause" | "Pause" => "Pause",
        _ => return false,
    };

    let res: zbus::Result<()> = player_proxy.call(method_name, &());
    res.is_ok()
}

fn playerctl_snapshot() -> MediaSnapshot {
    let Ok(output) = Command::new("playerctl")
        .args([
            "metadata",
            "--format",
            "{{playerName}}\t{{status}}\t{{artist}}\t{{title}}",
        ])
        .output()
    else {
        return MediaSnapshot::default();
    };

    if !output.status.success() {
        return MediaSnapshot::default();
    }

    parse_playerctl_metadata(&String::from_utf8_lossy(&output.stdout))
}

fn parse_playerctl_metadata(output: &str) -> MediaSnapshot {
    let mut parts = output.trim().splitn(4, '\t');
    MediaSnapshot {
        player: clean_part(parts.next()),
        status: clean_part(parts.next()),
        artist: clean_part(parts.next()),
        title: clean_part(parts.next()),
    }
}

fn clean_part(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playerctl_metadata_parser_handles_missing_fields() {
        let snapshot = parse_playerctl_metadata("spotify\tPlaying\tArtist\tTrack\n");

        assert_eq!(snapshot.player.as_deref(), Some("spotify"));
        assert_eq!(snapshot.status.as_deref(), Some("Playing"));
        assert_eq!(snapshot.artist.as_deref(), Some("Artist"));
        assert_eq!(snapshot.title.as_deref(), Some("Track"));
    }

    #[test]
    fn media_snapshot_is_active() {
        let active = MediaSnapshot {
            player: Some("mpv".into()),
            status: Some("Playing".into()),
            artist: None,
            title: Some("Song".into()),
        };
        assert!(active.is_active());

        let inactive = MediaSnapshot::default();
        assert!(!inactive.is_active());
    }
}
