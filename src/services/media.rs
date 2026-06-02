use std::process::Command;

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
}
