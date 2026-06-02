use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone)]
struct MediaActionEntry {
    title: &'static str,
    subtitle: &'static str,
    command: &'static str,
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
                    ActionKind::Shell(ShellCommand::new(entry.command)),
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
            subtitle: "Toggle MPRIS playback through playerctl",
            command: "playerctl play-pause",
            icon_name: "media-playback-start-symbolic",
        },
        MediaActionEntry {
            title: "Next Track",
            subtitle: "Skip to next MPRIS track through playerctl",
            command: "playerctl next",
            icon_name: "media-skip-forward-symbolic",
        },
        MediaActionEntry {
            title: "Previous Track",
            subtitle: "Return to previous MPRIS track through playerctl",
            command: "playerctl previous",
            icon_name: "media-skip-backward-symbolic",
        },
        MediaActionEntry {
            title: "Stop Playback",
            subtitle: "Stop current MPRIS player through playerctl",
            command: "playerctl stop",
            icon_name: "media-playback-stop-symbolic",
        },
    ]
}
