//! `bt …` / `bluetooth …`: the adapter's power and the known devices, with
//! connect and disconnect (Q7.1). Typing a device's name in the root search
//! finds it too.

use crate::action::{Action, ActionKind, ProcessCommand};
use crate::search::fuzzy_score;
use crate::services::bluetooth::{BluetoothDevice, BluetoothState, is_valid_address};

const CATEGORY: &str = "Bluetooth";

/// `Some(needle)` for an explicit `bt`/`bluetooth` query.
fn explicit_needle(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let lower = trimmed.to_lowercase();
    ["bluetooth", "bt"].iter().find_map(|prefix| {
        if lower == *prefix {
            Some("")
        } else if lower.starts_with(&format!("{prefix} ")) {
            Some(trimmed[prefix.len()..].trim())
        } else {
            None
        }
    })
}

/// Rows for `query`. `state` is only read when the query can show
/// Bluetooth: an explicit prefix, or three or more characters in the root.
pub(crate) fn search_bluetooth(query: &str, state: impl FnOnce() -> BluetoothState) -> Vec<Action> {
    let explicit = explicit_needle(query);
    let needle = explicit.unwrap_or_else(|| query.trim());
    if explicit.is_none() && needle.chars().count() < 3 {
        return Vec::new();
    }
    let (adapter, devices) = match state() {
        BluetoothState::Ready { adapter, devices } => (adapter, devices),
        BluetoothState::Unavailable(reason) => {
            // Only someone who asked for Bluetooth needs to hear why not.
            return explicit
                .map(|_| {
                    vec![
                        Action::new(CATEGORY, "Bluetooth unavailable", ActionKind::None, 300)
                            .with_subtitle(reason)
                            .with_icon("bluetooth-disabled-symbolic"),
                    ]
                })
                .unwrap_or_default();
        }
    };
    let bonus = if explicit.is_some() { 260 } else { 40 };
    let score = |haystack: &str, base: i32| {
        if needle.is_empty() {
            Some(base)
        } else {
            fuzzy_score(haystack, needle).map(|score| score + bonus)
        }
    };

    let mut actions = Vec::new();
    if explicit.is_some() {
        let (title, verb) = if adapter.powered {
            ("Turn Bluetooth Off", "off")
        } else {
            ("Turn Bluetooth On", "on")
        };
        if let Some(score) = score(&format!("{title} power {}", adapter.name), 300) {
            actions.push(
                Action::new(
                    CATEGORY,
                    title,
                    ActionKind::Command(ProcessCommand::new(
                        "bluetoothctl",
                        vec!["power".to_string(), verb.to_string()],
                    )),
                    score,
                )
                .with_subtitle(adapter.name.clone())
                .with_icon(if adapter.powered {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-disabled-symbolic"
                }),
            );
        }
    }
    for (index, device) in devices.iter().enumerate() {
        let base = 290 - index as i32;
        if let Some(score) = score(&device.name, base) {
            actions.extend(device_action(device, adapter.powered, score));
        }
    }
    actions
}

fn device_action(device: &BluetoothDevice, powered: bool, score: i32) -> Option<Action> {
    if !is_valid_address(&device.address) {
        return None;
    }
    let icon = device
        .icon
        .as_deref()
        .map(|icon| format!("{icon}-symbolic"))
        .unwrap_or_else(|| "bluetooth-symbolic".to_string());
    if !powered {
        return Some(
            Action::new(
                CATEGORY,
                format!("Connect {}", device.name),
                ActionKind::None,
                score,
            )
            .with_subtitle("Bluetooth is off: turn it on first")
            .with_icon(icon),
        );
    }
    let (verb, title) = if device.connected {
        ("disconnect", format!("Disconnect {}", device.name))
    } else {
        ("connect", format!("Connect {}", device.name))
    };
    let mut subtitle = if device.connected {
        "Connected"
    } else {
        "Paired"
    }
    .to_string();
    if let Some(battery) = device.battery {
        subtitle.push_str(&format!(" · battery {battery}%"));
    }
    Some(
        Action::new(
            CATEGORY,
            title,
            ActionKind::Command(ProcessCommand::new(
                "bluetoothctl",
                vec![verb.to_string(), device.address.clone()],
            )),
            score,
        )
        .with_subtitle(subtitle)
        .with_icon(icon),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::BluetoothAdapter;

    fn ready(powered: bool) -> BluetoothState {
        BluetoothState::Ready {
            adapter: BluetoothAdapter {
                name: "laptop".to_string(),
                powered,
            },
            devices: vec![
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:FF".to_string(),
                    name: "WH-1000XM4".to_string(),
                    connected: true,
                    paired: true,
                    icon: Some("audio-headset".to_string()),
                    battery: Some(65),
                },
                BluetoothDevice {
                    address: "11:22:33:44:55:66".to_string(),
                    name: "MX Master".to_string(),
                    connected: false,
                    paired: true,
                    icon: None,
                    battery: None,
                },
            ],
        }
    }

    fn rows(actions: &[Action]) -> Vec<(String, String)> {
        actions
            .iter()
            .map(|action| (action.title.clone(), action.value()))
            .collect()
    }

    #[test]
    fn bt_lists_power_and_devices_with_their_commands() {
        let mut actions = search_bluetooth("bt", || ready(true));
        actions.sort_by_key(|action| std::cmp::Reverse(action.score));
        assert_eq!(
            rows(&actions),
            vec![
                (
                    "Turn Bluetooth Off".to_string(),
                    "bluetoothctl power off".to_string()
                ),
                (
                    "Disconnect WH-1000XM4".to_string(),
                    "bluetoothctl disconnect AA:BB:CC:DD:EE:FF".to_string()
                ),
                (
                    "Connect MX Master".to_string(),
                    "bluetoothctl connect 11:22:33:44:55:66".to_string()
                ),
            ]
        );
        assert_eq!(actions[1].subtitle, "Connected · battery 65%");
    }

    #[test]
    fn a_device_is_found_by_name_from_the_root() {
        let actions = search_bluetooth("master", || ready(true));
        assert_eq!(rows(&actions)[0].0, "Connect MX Master");
        assert!(search_bluetooth("ma", || panic!("too short to ask BlueZ")).is_empty());
    }

    #[test]
    fn without_bluez_the_query_explains_and_the_root_stays_quiet() {
        let unavailable = || BluetoothState::Unavailable("BlueZ is not running".to_string());
        let actions = search_bluetooth("bluetooth", unavailable);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].kind, ActionKind::None));
        assert_eq!(actions[0].subtitle, "BlueZ is not running");
        assert!(search_bluetooth("headphones", unavailable).is_empty());
    }

    #[test]
    fn with_the_adapter_off_devices_say_so_instead_of_failing() {
        let actions = search_bluetooth("bt", || ready(false));
        assert!(actions.iter().any(|a| a.title == "Turn Bluetooth On"));
        let device = actions
            .iter()
            .find(|a| a.title == "Connect MX Master")
            .unwrap();
        assert!(matches!(device.kind, ActionKind::None));
    }
}
