use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatterySnapshot {
    pub batteries: Vec<BatteryDeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryDeviceSnapshot {
    pub name: String,
    pub capacity_percent: Option<u8>,
    pub status: Option<String>,
}

impl BatterySnapshot {
    pub fn primary(&self) -> Option<&BatteryDeviceSnapshot> {
        self.batteries.first()
    }
}

pub fn battery_snapshot() -> BatterySnapshot {
    BatterySnapshot {
        batteries: read_batteries().unwrap_or_default(),
    }
}

fn read_batteries() -> io::Result<Vec<BatteryDeviceSnapshot>> {
    let mut batteries = fs::read_dir("/sys/class/power_supply")?
        .filter_map(Result::ok)
        .filter_map(|entry| battery_device_snapshot(&entry.path()).ok())
        .collect::<Vec<_>>();
    batteries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(batteries)
}

fn battery_device_snapshot(path: &Path) -> io::Result<BatteryDeviceSnapshot> {
    let power_type = fs::read_to_string(path.join("type"))?;
    if power_type.trim() != "Battery" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a battery"));
    }

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("battery")
        .to_string();
    let capacity_percent = fs::read_to_string(path.join("capacity"))
        .ok()
        .and_then(|value| parse_capacity(value.trim()));
    let status = fs::read_to_string(path.join("status"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(BatteryDeviceSnapshot {
        name,
        capacity_percent,
        status,
    })
}

fn parse_capacity(value: &str) -> Option<u8> {
    value.parse::<u8>().ok().filter(|capacity| *capacity <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_parser_accepts_percent_range() {
        assert_eq!(parse_capacity("88"), Some(88));
        assert_eq!(parse_capacity("100"), Some(100));
        assert_eq!(parse_capacity("101"), None);
        assert_eq!(parse_capacity("full"), None);
    }

    #[test]
    fn primary_returns_first_battery() {
        let snapshot = BatterySnapshot {
            batteries: vec![BatteryDeviceSnapshot {
                name: "BAT0".to_string(),
                capacity_percent: Some(80),
                status: Some("Discharging".to_string()),
            }],
        };

        assert_eq!(snapshot.primary().unwrap().name, "BAT0");
    }
}
