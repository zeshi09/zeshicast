//! `profile …` / `power profile …`: switch the power profile (Q7.2).

use crate::action::{Action, ActionKind, ProcessCommand};
use crate::search::fuzzy_score;
use crate::services::power::{PowerProfiles, is_valid_profile, profile_label};

const CATEGORY: &str = "Power";

fn needle(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let lower = trimmed.to_lowercase();
    ["power profile", "profile"].iter().find_map(|prefix| {
        if lower == *prefix {
            Some("")
        } else if lower.starts_with(&format!("{prefix} ")) {
            Some(trimmed[prefix.len()..].trim())
        } else {
            None
        }
    })
}

/// Rows for an explicit `profile` query; nothing for any other query, so the
/// daemon is not asked on every keystroke.
pub(crate) fn search_power_profiles(
    query: &str,
    profiles: impl FnOnce() -> Result<PowerProfiles, String>,
) -> Vec<Action> {
    let Some(needle) = needle(query) else {
        return Vec::new();
    };
    let profiles = match profiles() {
        Ok(profiles) => profiles,
        Err(reason) => {
            return vec![
                Action::new(
                    CATEGORY,
                    "Power profiles unavailable",
                    ActionKind::None,
                    300,
                )
                .with_subtitle(reason)
                .with_icon("power-profile-balanced-symbolic"),
            ];
        }
    };
    profiles
        .available
        .iter()
        .enumerate()
        .filter(|(_, name)| is_valid_profile(name))
        .filter_map(|(index, name)| {
            let label = profile_label(name);
            let score = if needle.is_empty() {
                300 - index as i32
            } else {
                fuzzy_score(&format!("{label} {name}"), needle)? + 260
            };
            let icon = format!("power-profile-{name}-symbolic");
            let action = if *name == profiles.active {
                Action::new(
                    CATEGORY,
                    format!("{label} (current)"),
                    ActionKind::None,
                    score,
                )
                .with_subtitle("The active power profile")
            } else {
                Action::new(
                    CATEGORY,
                    format!("Switch to {label}"),
                    ActionKind::Command(ProcessCommand::new(
                        "powerprofilesctl",
                        vec!["set".to_string(), name.clone()],
                    )),
                    score,
                )
                .with_subtitle(format!("Now: {}", profile_label(&profiles.active)))
            };
            Some(action.with_icon(icon))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles() -> Result<PowerProfiles, String> {
        Ok(PowerProfiles {
            active: "balanced".to_string(),
            available: vec![
                "power-saver".to_string(),
                "balanced".to_string(),
                "performance".to_string(),
                "bad; rm".to_string(),
            ],
        })
    }

    #[test]
    fn profile_lists_every_profile_and_switches_to_the_others() {
        let actions = search_power_profiles("profile", profiles);
        let rows: Vec<_> = actions
            .iter()
            .map(|a| (a.title.as_str(), a.value()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "Switch to Power Saver",
                    "powerprofilesctl set power-saver".to_string()
                ),
                ("Balanced (current)", "Balanced (current)".to_string()),
                (
                    "Switch to Performance",
                    "powerprofilesctl set performance".to_string()
                ),
            ]
        );
        let perf = search_power_profiles("profile perf", profiles);
        assert_eq!(perf.len(), 1);
        assert!(search_power_profiles("performance", || panic!("not asked")).is_empty());
    }

    #[test]
    fn without_the_daemon_profile_explains() {
        let actions = search_power_profiles("power profile", || {
            Err("power-profiles-daemon is not running".to_string())
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].kind, ActionKind::None));
        assert_eq!(actions[0].subtitle, "power-profiles-daemon is not running");
    }
}
