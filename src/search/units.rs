//! `unit …` / `systemd …`: the user's systemd services (Q7.4). A failing
//! unit is listed first and restarts from here; restart and stop ask
//! first; the journal opens in the output view.

use crate::action::{Action, ActionKind, ActionRisk, ProcessCommand, ScriptMode};
use crate::search::fuzzy_score;
use crate::services::systemd_units::UserUnit;

const CATEGORY: &str = "Unit";

fn needle(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let lower = trimmed.to_lowercase();
    ["systemd", "unit"].iter().find_map(|prefix| {
        if lower == *prefix {
            Some("")
        } else if lower.starts_with(&format!("{prefix} ")) {
            Some(trimmed[prefix.len()..].trim())
        } else {
            None
        }
    })
}

fn systemctl(verb: &str, unit: &str) -> ActionKind {
    ActionKind::Command(ProcessCommand::new(
        "systemctl",
        vec!["--user".to_string(), verb.to_string(), unit.to_string()],
    ))
}

fn status(unit: &UserUnit) -> String {
    let state = if unit.sub.is_empty() || unit.sub == unit.active {
        unit.active.clone()
    } else {
        format!("{} ({})", unit.active, unit.sub)
    };
    if unit.description.is_empty() {
        state
    } else {
        format!("{state} · {}", unit.description)
    }
}

fn icon(unit: &UserUnit) -> &'static str {
    if unit.is_failed() {
        "dialog-error-symbolic"
    } else if unit.is_running() {
        "media-playback-start-symbolic"
    } else {
        "media-playback-stop-symbolic"
    }
}

/// Rows for an explicit `unit` query. Without a filter each unit has one
/// row (restart a running or failed unit, start a stopped one); a filter
/// adds stop and the journal for the units it matches.
pub(crate) fn search_units(
    query: &str,
    units: impl FnOnce() -> Result<Vec<UserUnit>, String>,
) -> Vec<Action> {
    let Some(needle) = needle(query) else {
        return Vec::new();
    };
    let units = match units() {
        Ok(units) => units,
        Err(reason) => {
            return vec![
                Action::new(CATEGORY, "User units unavailable", ActionKind::None, 300)
                    .with_subtitle(reason)
                    .with_icon("dialog-warning-symbolic"),
            ];
        }
    };
    let mut actions = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let short = unit.name.trim_end_matches(".service");
        let score = if needle.is_empty() {
            300 - index as i32
        } else {
            match fuzzy_score(&format!("{short} {}", unit.description), needle) {
                Some(score) => score + 260,
                None => continue,
            }
        };
        let main = if unit.is_failed() || unit.is_running() {
            Action::new(
                CATEGORY,
                format!("Restart {short}"),
                systemctl("restart", &unit.name),
                score,
            )
            .with_risk(ActionRisk::Destructive)
        } else {
            Action::new(
                CATEGORY,
                format!("Start {short}"),
                systemctl("start", &unit.name),
                score,
            )
        };
        actions.push(main.with_subtitle(status(unit)).with_icon(icon(unit)));
        if needle.is_empty() {
            continue;
        }
        if unit.is_running() {
            actions.push(
                Action::new(
                    CATEGORY,
                    format!("Stop {short}"),
                    systemctl("stop", &unit.name),
                    score - 1,
                )
                .with_risk(ActionRisk::Destructive)
                .with_subtitle(status(unit))
                .with_icon("media-playback-stop-symbolic"),
            );
        }
        actions.push(
            Action::new(
                CATEGORY,
                format!("Logs {short}"),
                ActionKind::Command(ProcessCommand::new(
                    "journalctl",
                    vec![
                        "--user".to_string(),
                        "-u".to_string(),
                        unit.name.clone(),
                        "-n".to_string(),
                        "200".to_string(),
                        "--no-pager".to_string(),
                    ],
                )),
                score - 2,
            )
            .with_subtitle("The last 200 lines of its journal")
            .with_icon("text-x-generic-symbolic")
            .with_script_mode(ScriptMode::FullOutput),
        );
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Result<Vec<UserUnit>, String> {
        let unit = |name: &str, active: &str, sub: &str| UserUnit {
            name: name.to_string(),
            active: active.to_string(),
            sub: sub.to_string(),
            description: String::new(),
        };
        Ok(vec![
            unit("syncthing.service", "failed", "failed"),
            unit("zeshicast.service", "active", "running"),
            unit("backup.service", "inactive", "dead"),
        ])
    }

    #[test]
    fn unit_lists_one_row_per_unit_failed_first() {
        let actions = search_units("unit", units);
        let rows: Vec<_> = actions
            .iter()
            .map(|a| (a.title.as_str(), a.value()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "Restart syncthing",
                    "systemctl --user restart syncthing.service".to_string()
                ),
                (
                    "Restart zeshicast",
                    "systemctl --user restart zeshicast.service".to_string()
                ),
                (
                    "Start backup",
                    "systemctl --user start backup.service".to_string()
                ),
            ]
        );
        assert_eq!(actions[0].subtitle, "failed");
        assert!(search_units("syncthing", || panic!("only on `unit`")).is_empty());
    }

    #[test]
    fn restart_and_stop_ask_first_start_and_logs_do_not() {
        let actions = search_units("unit zeshicast", units);
        let risk = |title: &str| actions.iter().find(|a| a.title == title).unwrap().risk;
        assert_eq!(risk("Restart zeshicast"), ActionRisk::Destructive);
        assert_eq!(risk("Stop zeshicast"), ActionRisk::Destructive);
        assert_eq!(risk("Logs zeshicast"), ActionRisk::Normal);
        let logs = actions
            .iter()
            .find(|a| a.title == "Logs zeshicast")
            .unwrap();
        assert_eq!(logs.script_mode(), Some(ScriptMode::FullOutput));
        assert!(
            logs.value()
                .starts_with("journalctl --user -u zeshicast.service")
        );

        let backup = search_units("systemd backup", units);
        assert_eq!(
            backup[0].risk,
            ActionRisk::Normal,
            "start is not destructive"
        );
        assert!(
            !backup.iter().any(|a| a.title.starts_with("Stop")),
            "nothing to stop"
        );
    }

    #[test]
    fn without_systemd_the_query_explains() {
        let actions = search_units("unit", || Err("systemctl is not available".to_string()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].kind, ActionKind::None));
    }
}
