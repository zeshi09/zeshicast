use crate::{Action, ActionKind, MediaControl, fuzzy_score};

#[derive(Debug, Clone)]
struct MediaActionEntry {
    title: &'static str,
    subtitle: &'static str,
    control: MediaControl,
    icon_name: &'static str,
}

pub(crate) fn search_media_actions(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("media ")
        || lower.starts_with("player ")
        || lower.starts_with("mpris ")
        || lower == "media"
        || lower == "player"
        || lower == "mpris";
    let needle = if explicit {
        query
            .split_once(' ')
            .map(|(_, value)| value.trim())
            .unwrap_or_default()
    } else {
        query.trim()
    };

    media_action_entries()
        .into_iter()
        .filter_map(|entry| {
            let haystack = format!("{} {}", entry.title, entry.subtitle);
            let score = if needle.is_empty() {
                explicit.then_some(0)?
            } else {
                fuzzy_score(&haystack, needle)?
            };
            Some(
                Action::new(
                    "Media",
                    entry.title,
                    ActionKind::Media(entry.control),
                    score + if explicit { 260 } else { 30 },
                )
                .with_subtitle(entry.subtitle)
                .with_icon(entry.icon_name),
            )
        })
        .collect()
}

fn media_action_entries() -> Vec<MediaActionEntry> {
    vec![
        MediaActionEntry {
            title: "Play/Pause",
            subtitle: "Toggle playback on the active MPRIS player",
            control: MediaControl::PlayPause,
            icon_name: "media-playback-start-symbolic",
        },
        MediaActionEntry {
            title: "Next Track",
            subtitle: "Skip to the next MPRIS track",
            control: MediaControl::Next,
            icon_name: "media-skip-forward-symbolic",
        },
        MediaActionEntry {
            title: "Previous Track",
            subtitle: "Return to the previous MPRIS track",
            control: MediaControl::Previous,
            icon_name: "media-skip-backward-symbolic",
        },
        MediaActionEntry {
            title: "Stop Playback",
            subtitle: "Stop the active MPRIS player",
            control: MediaControl::Stop,
            icon_name: "media-playback-stop-symbolic",
        },
    ]
}
