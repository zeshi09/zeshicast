use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ThermalSnapshot {
    pub zones: Vec<ThermalZoneSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalZoneSnapshot {
    pub name: String,
    pub temperature_c: f32,
}

impl ThermalSnapshot {
    pub fn hottest_zone(&self) -> Option<&ThermalZoneSnapshot> {
        self.zones.iter().max_by(|left, right| {
            left.temperature_c
                .partial_cmp(&right.temperature_c)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

pub fn thermal_snapshot() -> ThermalSnapshot {
    ThermalSnapshot {
        zones: read_thermal_zones("/sys/class/thermal").unwrap_or_default(),
    }
}

fn read_thermal_zones(root: impl AsRef<Path>) -> io::Result<Vec<ThermalZoneSnapshot>> {
    let mut zones = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        if !filename.starts_with("thermal_zone") {
            continue;
        }

        let path = entry.path();
        let name = fs::read_to_string(path.join("type"))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| filename.to_string());
        let Ok(raw_temp) = fs::read_to_string(path.join("temp")) else {
            continue;
        };
        let Some(temperature_c) = parse_thermal_temp(&raw_temp) else {
            continue;
        };

        zones.push(ThermalZoneSnapshot {
            name,
            temperature_c,
        });
    }

    zones.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(zones)
}

fn parse_thermal_temp(value: &str) -> Option<f32> {
    let raw = value.trim().parse::<f32>().ok()?;
    if raw.abs() > 1_000.0 {
        Some(raw / 1_000.0)
    } else {
        Some(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_temp_parser_accepts_millicelsius() {
        assert_eq!(parse_thermal_temp("42500\n"), Some(42.5));
    }

    #[test]
    fn thermal_temp_parser_accepts_celsius() {
        assert_eq!(parse_thermal_temp("43\n"), Some(43.0));
    }

    #[test]
    fn hottest_zone_returns_highest_temperature() {
        let snapshot = ThermalSnapshot {
            zones: vec![
                ThermalZoneSnapshot {
                    name: "wifi".to_string(),
                    temperature_c: 39.0,
                },
                ThermalZoneSnapshot {
                    name: "cpu".to_string(),
                    temperature_c: 58.5,
                },
            ],
        };

        assert_eq!(snapshot.hottest_zone().unwrap().name, "cpu");
    }
}
