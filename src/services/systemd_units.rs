//! systemd user units (Q7.4): `systemctl --user list-units --output=json`,
//! read on demand and cached briefly.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserUnit {
    pub name: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

impl UserUnit {
    pub fn is_failed(&self) -> bool {
        self.active == "failed"
    }

    pub fn is_running(&self) -> bool {
        matches!(self.active.as_str(), "active" | "activating" | "reloading")
    }
}

/// A name that can only be a unit: no option (`-…`), a `.service`.
pub(crate) fn is_valid_unit_name(name: &str) -> bool {
    !name.starts_with('-')
        && name.ends_with(".service")
        && name.len() > ".service".len()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '/')
}

/// Services from the JSON listing: units that are not found are left out;
/// failed first, then running, then the rest, each by name.
pub(crate) fn parse_units(json: &str) -> Result<Vec<UserUnit>, String> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(json)
        .map_err(|error| format!("unexpected systemctl output: {error}"))?;
    let text =
        |row: &serde_json::Value, key: &str| row[key].as_str().unwrap_or_default().to_string();
    let mut units: Vec<UserUnit> = rows
        .iter()
        .filter(|row| row["load"].as_str() != Some("not-found"))
        .filter_map(|row| {
            let name = text(row, "unit");
            is_valid_unit_name(&name).then(|| UserUnit {
                name,
                active: text(row, "active"),
                sub: text(row, "sub"),
                description: text(row, "description"),
            })
        })
        .collect();
    units.sort_by(|a, b| {
        b.is_failed()
            .cmp(&a.is_failed())
            .then(b.is_running().cmp(&a.is_running()))
            .then(a.name.cmp(&b.name))
    });
    Ok(units)
}

const CACHE_TTL: Duration = Duration::from_secs(2);
type Listing = Result<Vec<UserUnit>, String>;
static CACHE: Mutex<Option<(Instant, Listing)>> = Mutex::new(None);

/// The user's services, or why they cannot be listed.
pub fn user_units() -> Listing {
    // Unit tests must not see the machine's units.
    if cfg!(test) {
        return Err("not read in tests".to_string());
    }
    let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((at, units)) = cache.as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return units.clone();
    }
    let units = read_units();
    *cache = Some((Instant::now(), units.clone()));
    units
}

fn read_units() -> Listing {
    let output = std::process::Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ])
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|error| format!("systemctl is not available: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("systemctl --user failed: {}", stderr.trim()));
    }
    parse_units(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"[
        {"unit":"app-org.telegram.desktop@autostart.service","load":"loaded","active":"inactive","sub":"dead","description":"Telegram"},
        {"unit":"zeshicast.service","load":"loaded","active":"active","sub":"running","description":"zeshicast launcher daemon"},
        {"unit":"syncthing.service","load":"loaded","active":"failed","sub":"failed","description":"Syncthing"},
        {"unit":"ghost.service","load":"not-found","active":"inactive","sub":"dead","description":"ghost"},
        {"unit":"-evil.service","load":"loaded","active":"active","sub":"running","description":"x"},
        {"unit":"dbus.socket","load":"loaded","active":"active","sub":"running"}
    ]"#;

    #[test]
    fn units_are_listed_failed_then_running() {
        let units = parse_units(LISTING).unwrap();
        let names: Vec<_> = units.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "syncthing.service",
                "zeshicast.service",
                "app-org.telegram.desktop@autostart.service"
            ]
        );
        assert!(units[0].is_failed());
        assert_eq!(units[2].description, "Telegram");
    }

    #[test]
    fn broken_output_is_an_error_not_a_panic() {
        assert!(parse_units("not json").is_err());
        assert_eq!(parse_units("[{}]").unwrap(), Vec::new());
    }

    #[test]
    fn only_service_names_reach_systemctl() {
        assert!(is_valid_unit_name(
            "app-gnome\\x2dkeyring@autostart.service"
        ));
        assert!(!is_valid_unit_name("--now.service"));
        assert!(!is_valid_unit_name("foo.socket"));
        assert!(!is_valid_unit_name(".service"));
        assert!(!is_valid_unit_name("a b.service"));
    }
}
