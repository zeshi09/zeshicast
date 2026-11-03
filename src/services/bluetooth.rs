//! Bluetooth through BlueZ (Q7.1): the adapter and the known devices, read
//! from `org.bluez` on the system bus with one `GetManagedObjects` call.
//! Actions go through `bluetoothctl` (the `bluez` package), so they pass
//! the execution gateway like every other command and a slow connect runs
//! detached instead of blocking the palette.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothAdapter {
    pub name: String,
    pub powered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    /// BlueZ's icon hint (`audio-headset`, `input-mouse`, …).
    pub icon: Option<String>,
    /// Percent, from `org.bluez.Battery1` when the device reports it.
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothState {
    /// No BlueZ, no adapter, or no system bus: why, for the user.
    Unavailable(String),
    Ready {
        adapter: BluetoothAdapter,
        /// Paired or connected devices, connected first, then by name.
        devices: Vec<BluetoothDevice>,
    },
}

/// One `org.bluez` object as read from D-Bus, reduced to what is used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BluezObject {
    pub(crate) path: String,
    pub(crate) adapter: Option<BluetoothAdapter>,
    pub(crate) device: Option<BluetoothDevice>,
    pub(crate) battery: Option<u8>,
}

/// The state from BlueZ's objects: the first adapter, and the devices that
/// are paired or connected (discovered strangers are left out).
pub(crate) fn state_from_objects(mut objects: Vec<BluezObject>) -> BluetoothState {
    objects.sort_by(|a, b| a.path.cmp(&b.path));
    let Some(adapter) = objects.iter().find_map(|object| object.adapter.clone()) else {
        return BluetoothState::Unavailable("no Bluetooth adapter found".to_string());
    };
    let mut devices: Vec<BluetoothDevice> = objects
        .iter()
        .filter_map(|object| {
            let mut device = object.device.clone()?;
            // Battery1 lives on the device's own object path.
            device.battery = device.battery.or(object.battery);
            (device.paired || device.connected).then_some(device)
        })
        .collect();
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    BluetoothState::Ready { adapter, devices }
}

/// `XX:XX:XX:XX:XX:XX`: the only form passed to `bluetoothctl`.
pub(crate) fn is_valid_address(address: &str) -> bool {
    let parts: Vec<&str> = address.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

const CACHE_TTL: Duration = Duration::from_secs(2);

/// The current state, read at most every [`CACHE_TTL`]: the palette asks on
/// every keystroke.
static CACHE: Mutex<Option<(Instant, BluetoothState)>> = Mutex::new(None);

pub fn bluetooth_state() -> BluetoothState {
    let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((at, state)) = cache.as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return state.clone();
    }
    let state = read_state();
    *cache = Some((Instant::now(), state.clone()));
    state
}

#[cfg(feature = "desktop")]
fn read_state() -> BluetoothState {
    // Unit tests must not see the machine's real devices.
    if cfg!(test) {
        return BluetoothState::Unavailable("not read in tests".to_string());
    }
    match dbus::managed_objects() {
        Ok(objects) => state_from_objects(objects),
        Err(reason) => BluetoothState::Unavailable(reason),
    }
}

#[cfg(not(feature = "desktop"))]
fn read_state() -> BluetoothState {
    BluetoothState::Unavailable("this build has no D-Bus support".to_string())
}

#[cfg(feature = "desktop")]
pub(crate) mod dbus {
    use super::{BluetoothAdapter, BluetoothDevice, BluezObject};

    const BLUEZ: &str = "org.bluez";

    /// `GetManagedObjects` on BlueZ, reduced to [`BluezObject`]s.
    pub(crate) fn managed_objects() -> Result<Vec<BluezObject>, String> {
        let bus = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)
            .map_err(|error| format!("no system bus: {error}"))?;
        let reply = bus
            .call_sync(
                Some(BLUEZ),
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
                None,
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .map_err(|error| {
                if error.to_string().contains("ServiceUnknown") {
                    "BlueZ is not running (bluetooth.service)".to_string()
                } else {
                    format!("BlueZ did not answer: {error}")
                }
            })?;
        Ok(objects_from_variant(&reply.child_value(0)))
    }

    fn property(properties: &glib::VariantDict, name: &str) -> Option<glib::Variant> {
        properties.lookup_value(name, None)
    }

    /// `a{oa{sa{sv}}}` → objects. Unknown interfaces and properties are
    /// ignored, missing ones fall back to defaults, so a partial answer
    /// still reads.
    pub(crate) fn objects_from_variant(objects: &glib::Variant) -> Vec<BluezObject> {
        let mut out = Vec::new();
        for index in 0..objects.n_children() {
            let entry = objects.child_value(index);
            let Some(path) = entry.child_value(0).str().map(str::to_string) else {
                continue;
            };
            let interfaces = entry.child_value(1);
            let mut object = BluezObject {
                path,
                ..Default::default()
            };
            for iface_index in 0..interfaces.n_children() {
                let iface = interfaces.child_value(iface_index);
                let Some(name) = iface.child_value(0).str().map(str::to_string) else {
                    continue;
                };
                let properties = glib::VariantDict::new(Some(&iface.child_value(1)));
                let string = |key: &str| property(&properties, key).and_then(|v| v.get::<String>());
                let boolean = |key: &str| {
                    property(&properties, key)
                        .and_then(|v| v.get::<bool>())
                        .unwrap_or(false)
                };
                match name.as_str() {
                    "org.bluez.Adapter1" => {
                        object.adapter = Some(BluetoothAdapter {
                            name: string("Alias")
                                .or_else(|| string("Name"))
                                .unwrap_or_else(|| "Bluetooth".to_string()),
                            powered: boolean("Powered"),
                        });
                    }
                    "org.bluez.Device1" => {
                        let Some(address) = string("Address") else {
                            continue;
                        };
                        object.device = Some(BluetoothDevice {
                            name: string("Alias")
                                .or_else(|| string("Name"))
                                .unwrap_or_else(|| address.clone()),
                            address,
                            connected: boolean("Connected"),
                            paired: boolean("Paired"),
                            icon: string("Icon"),
                            battery: None,
                        });
                    }
                    "org.bluez.Battery1" => {
                        object.battery = property(&properties, "Percentage")
                            .and_then(|v| v.get::<u8>())
                            .filter(|percent| *percent <= 100);
                    }
                    _ => {}
                }
            }
            out.push(object);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, connected: bool, paired: bool) -> BluetoothDevice {
        BluetoothDevice {
            address: "00:11:22:33:44:55".to_string(),
            name: name.to_string(),
            connected,
            paired,
            icon: None,
            battery: None,
        }
    }

    #[test]
    fn known_devices_are_listed_connected_first() {
        let objects = vec![
            BluezObject {
                path: "/org/bluez/hci0".to_string(),
                adapter: Some(BluetoothAdapter {
                    name: "laptop".to_string(),
                    powered: true,
                }),
                ..Default::default()
            },
            BluezObject {
                path: "/org/bluez/hci0/dev_1".to_string(),
                device: Some(device("Mouse", false, true)),
                ..Default::default()
            },
            BluezObject {
                path: "/org/bluez/hci0/dev_2".to_string(),
                device: Some(device("Stranger TV", false, false)),
                ..Default::default()
            },
            BluezObject {
                path: "/org/bluez/hci0/dev_3".to_string(),
                device: Some(device("Headphones", true, true)),
                battery: Some(80),
                ..Default::default()
            },
        ];
        let BluetoothState::Ready { adapter, devices } = state_from_objects(objects) else {
            panic!("ready");
        };
        assert!(adapter.powered);
        let names: Vec<_> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["Headphones", "Mouse"]);
        assert_eq!(devices[0].battery, Some(80));
    }

    #[test]
    fn no_adapter_is_explained() {
        assert!(matches!(
            state_from_objects(Vec::new()),
            BluetoothState::Unavailable(reason) if reason.contains("adapter")
        ));
    }

    #[test]
    fn only_plain_addresses_reach_bluetoothctl() {
        assert!(is_valid_address("A0:B1:c2:D3:e4:F5"));
        assert!(!is_valid_address("A0:B1:C2:D3:E4"));
        assert!(!is_valid_address("A0:B1:C2:D3:E4:F5; rm -rf ~"));
        assert!(!is_valid_address("--help"));
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn a_bluez_answer_is_read_from_its_gvariant() {
        // GVariant text of a trimmed `GetManagedObjects` reply.
        let fixture = r#"{
            objectpath '/org/bluez/hci0': {'org.bluez.Adapter1': {'Alias': <'laptop'>, 'Powered': <true>}},
            '/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF': {
                'org.bluez.Device1': {'Address': <'AA:BB:CC:DD:EE:FF'>, 'Alias': <'WH-1000XM4'>,
                    'Paired': <true>, 'Connected': <false>, 'Icon': <'audio-headset'>, 'RSSI': <int16 -60>},
                'org.bluez.Battery1': {'Percentage': <byte 0x41>},
                'org.freedesktop.DBus.Properties': @a{sv} {}},
            '/org/bluez/hci0/dev_11_22_33_44_55_66': {'org.bluez.Device1': {'Paired': <true>}}
        }"#;
        let variant = glib::Variant::parse(None, fixture).expect("valid GVariant text");
        let state = state_from_objects(dbus::objects_from_variant(&variant));
        let BluetoothState::Ready { adapter, devices } = state else {
            panic!("ready: {state:?}");
        };
        assert_eq!(adapter.name, "laptop");
        assert_eq!(devices.len(), 1, "a device without an address is skipped");
        assert_eq!(devices[0].name, "WH-1000XM4");
        assert_eq!(devices[0].battery, Some(65));
        assert_eq!(devices[0].icon.as_deref(), Some("audio-headset"));
    }
}
