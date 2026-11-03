//! Power (Q7.2): the power profile from power-profiles-daemon and the
//! battery as UPower sees it, both over the system bus. Switching a profile
//! goes through `powerprofilesctl`, so it passes the execution gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerProfiles {
    pub active: String,
    /// In the daemon's order: `power-saver`, `balanced`, `performance`.
    pub available: Vec<String>,
}

/// A profile name as passed to `powerprofilesctl set`: lowercase letters
/// and dashes, starting with a letter (so never an option).
pub(crate) fn is_valid_profile(name: &str) -> bool {
    name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

pub(crate) fn profile_label(name: &str) -> String {
    match name {
        "power-saver" => "Power Saver".to_string(),
        "balanced" => "Balanced".to_string(),
        "performance" => "Performance".to_string(),
        other => other.to_string(),
    }
}

/// UPower's `DisplayDevice`: the combined battery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpowerBattery {
    pub(crate) percent: u8,
    pub(crate) status: String,
}

/// `State` from `org.freedesktop.UPower.Device`.
fn state_label(state: u32) -> Option<&'static str> {
    match state {
        1 => Some("Charging"),
        2 => Some("Discharging"),
        3 => Some("Empty"),
        4 => Some("Full"),
        5 => Some("Not charging"),
        6 => Some("Discharging"),
        _ => None,
    }
}

fn duration_label(seconds: i64) -> Option<String> {
    if seconds <= 0 {
        return None;
    }
    let minutes = seconds / 60;
    Some(if minutes >= 60 {
        format!("{} h {} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    })
}

/// The battery from `DisplayDevice` properties; `None` without one (a
/// desktop reports `IsPresent = false`).
pub(crate) fn battery_from_upower(
    present: bool,
    percentage: f64,
    state: u32,
    time_to_empty: i64,
    time_to_full: i64,
) -> Option<UpowerBattery> {
    if !present || !(0.0..=100.0).contains(&percentage) {
        return None;
    }
    let mut status = state_label(state).unwrap_or("Unknown").to_string();
    let remaining = match state {
        1 => duration_label(time_to_full).map(|time| format!("{time} to full")),
        2 | 6 => duration_label(time_to_empty).map(|time| format!("{time} left")),
        _ => None,
    };
    if let Some(remaining) = remaining {
        status.push_str(" · ");
        status.push_str(&remaining);
    }
    Some(UpowerBattery {
        percent: percentage.round() as u8,
        status,
    })
}

#[cfg(feature = "desktop")]
pub(crate) mod dbus {
    use super::{PowerProfiles, UpowerBattery, battery_from_upower};

    /// The daemon's current name first, the name it had before 0.20.
    const PROFILE_SERVICES: [(&str, &str, &str); 2] = [
        (
            "org.freedesktop.UPower.PowerProfiles",
            "/org/freedesktop/UPower/PowerProfiles",
            "org.freedesktop.UPower.PowerProfiles",
        ),
        (
            "net.hadess.PowerProfiles",
            "/net/hadess/PowerProfiles",
            "net.hadess.PowerProfiles",
        ),
    ];

    fn get_all(
        bus: &gio::DBusConnection,
        name: &str,
        path: &str,
        interface: &str,
    ) -> Result<glib::VariantDict, glib::Error> {
        use glib::variant::ToVariant;
        let reply = bus.call_sync(
            Some(name),
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            Some(&(interface,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        )?;
        Ok(glib::VariantDict::new(Some(&reply.child_value(0))))
    }

    fn system_bus() -> Result<gio::DBusConnection, String> {
        gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)
            .map_err(|error| format!("no system bus: {error}"))
    }

    pub(crate) fn power_profiles() -> Result<PowerProfiles, String> {
        let bus = system_bus()?;
        let mut last_error = String::new();
        for (name, path, interface) in PROFILE_SERVICES {
            match get_all(&bus, name, path, interface) {
                Ok(properties) => {
                    return profiles_from_properties(&properties).ok_or_else(|| {
                        "power-profiles-daemon answered without profiles".to_string()
                    });
                }
                Err(error) => last_error = error.to_string(),
            }
        }
        Err(if last_error.contains("ServiceUnknown") {
            "power-profiles-daemon is not running".to_string()
        } else {
            format!("power-profiles-daemon did not answer: {last_error}")
        })
    }

    /// `ActiveProfile` and `Profiles` (`aa{sv}`, each with a `Profile`).
    pub(crate) fn profiles_from_properties(
        properties: &glib::VariantDict,
    ) -> Option<PowerProfiles> {
        let active = properties
            .lookup_value("ActiveProfile", None)?
            .get::<String>()?;
        let list = properties.lookup_value("Profiles", None)?;
        let available = (0..list.n_children())
            .filter_map(|index| {
                glib::VariantDict::new(Some(&list.child_value(index)))
                    .lookup_value("Profile", None)?
                    .get::<String>()
            })
            .collect();
        Some(PowerProfiles { active, available })
    }

    pub(crate) fn upower_battery() -> Option<UpowerBattery> {
        let bus = system_bus().ok()?;
        let properties = get_all(
            &bus,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower/devices/DisplayDevice",
            "org.freedesktop.UPower.Device",
        )
        .ok()?;
        battery_from_properties(&properties)
    }

    pub(crate) fn battery_from_properties(properties: &glib::VariantDict) -> Option<UpowerBattery> {
        let value = |key: &str| properties.lookup_value(key, None);
        battery_from_upower(
            value("IsPresent")?.get::<bool>()?,
            value("Percentage")?.get::<f64>()?,
            value("State").and_then(|v| v.get::<u32>()).unwrap_or(0),
            value("TimeToEmpty")
                .and_then(|v| v.get::<i64>())
                .unwrap_or(0),
            value("TimeToFull")
                .and_then(|v| v.get::<i64>())
                .unwrap_or(0),
        )
    }
}

/// The power profiles, or why there are none.
pub fn power_profiles() -> Result<PowerProfiles, String> {
    #[cfg(feature = "desktop")]
    {
        // Unit tests must not see the machine's daemon.
        if cfg!(test) {
            return Err("not read in tests".to_string());
        }
        dbus::power_profiles()
    }
    #[cfg(not(feature = "desktop"))]
    {
        Err("this build has no D-Bus support".to_string())
    }
}

/// The battery from UPower, when it runs and reports one.
pub(crate) fn upower_battery() -> Option<UpowerBattery> {
    #[cfg(feature = "desktop")]
    {
        if cfg!(test) {
            return None;
        }
        dbus::upower_battery()
    }
    #[cfg(not(feature = "desktop"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upower_battery_reads_level_state_and_time() {
        assert_eq!(
            battery_from_upower(true, 41.6, 2, 5_400, 0),
            Some(UpowerBattery {
                percent: 42,
                status: "Discharging · 1 h 30 min left".to_string()
            })
        );
        assert_eq!(
            battery_from_upower(true, 80.0, 1, 0, 1_500).unwrap().status,
            "Charging · 25 min to full"
        );
        assert_eq!(
            battery_from_upower(true, 100.0, 4, 0, 0).unwrap().status,
            "Full"
        );
        assert_eq!(
            battery_from_upower(false, 0.0, 0, 0, 0),
            None,
            "a desktop has none"
        );
        assert_eq!(battery_from_upower(true, f64::NAN, 2, 0, 0), None);
    }

    #[test]
    fn only_plain_profile_names_reach_powerprofilesctl() {
        assert!(is_valid_profile("power-saver"));
        assert!(!is_valid_profile("balanced; reboot"));
        assert!(!is_valid_profile("--help"));
        assert!(!is_valid_profile("Performance"));
        assert!(!is_valid_profile(""));
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn daemon_properties_are_read_from_their_gvariant() {
        let profiles = glib::Variant::parse(
            None,
            "{'ActiveProfile': <'balanced'>, 'Profiles': <[
                {'Profile': <'power-saver'>, 'Driver': <'platform_profile'>},
                {'Profile': <'balanced'>}, {'Profile': <'performance'>}]>,
              'PerformanceDegraded': <''>}",
        )
        .unwrap();
        let read =
            dbus::profiles_from_properties(&glib::VariantDict::new(Some(&profiles))).unwrap();
        assert_eq!(read.active, "balanced");
        assert_eq!(
            read.available,
            vec!["power-saver", "balanced", "performance"]
        );

        let battery = glib::Variant::parse(
            None,
            "{'IsPresent': <true>, 'Percentage': <55.0>, 'State': <uint32 1>,
              'TimeToFull': <int64 600>, 'TimeToEmpty': <int64 0>}",
        )
        .unwrap();
        let read = dbus::battery_from_properties(&glib::VariantDict::new(Some(&battery))).unwrap();
        assert_eq!(read.percent, 55);
        assert_eq!(read.status, "Charging · 10 min to full");

        let partial = glib::Variant::parse(None, "{'Percentage': <55.0>}").unwrap();
        assert_eq!(
            dbus::battery_from_properties(&glib::VariantDict::new(Some(&partial))),
            None
        );
    }
}
