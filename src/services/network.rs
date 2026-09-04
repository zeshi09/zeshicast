use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterfaceSnapshot>,
    pub wifi_networks: Vec<WifiNetworkSnapshot>,
    pub vpn_connections: Vec<VpnConnectionSnapshot>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterfaceSnapshot {
    pub name: String,
    pub state: String,
    pub is_wireless: bool,
    pub mac_address: Option<String>,
    pub ipv4_addresses: Vec<String>,
    pub ipv6_addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetworkSnapshot {
    pub ssid: String,
    pub signal_percent: Option<u8>,
    pub security: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnConnectionSnapshot {
    pub name: String,
    pub kind: String,
}

pub fn network_snapshot() -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces: read_interfaces().unwrap_or_default(),
        wifi_networks: read_wifi_networks().unwrap_or_default(),
        vpn_connections: read_vpn_connections().unwrap_or_default(),
        dns_servers: read_dns_servers().unwrap_or_default(),
    }
}

type NetByteCounters = HashMap<String, (u64, u64)>;
type NetSpeedState = Option<(NetByteCounters, Instant)>;

static NET_SPEED_STATE: Mutex<NetSpeedState> = Mutex::new(None);

/// Returns (rx_mbps, tx_mbps) for the given interface, using a static to track deltas.
pub fn net_speed_mbps(iface: &str) -> (f64, f64) {
    let rx = fs::read_to_string(format!("/sys/class/net/{iface}/statistics/rx_bytes"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let tx = fs::read_to_string(format!("/sys/class/net/{iface}/statistics/tx_bytes"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let now = Instant::now();

    // Poisoning only means a panic while holding the lock; the state is
    // self-healing speed counters, so recover it instead of panicking here.
    let mut state = NET_SPEED_STATE.lock().unwrap_or_else(|p| p.into_inner());
    let speeds = if let Some((prev_map, prev_time)) = state.as_ref() {
        let dt = now.duration_since(*prev_time).as_secs_f64();
        if dt > 0.2 {
            if let Some(&(prev_rx, prev_tx)) = prev_map.get(iface) {
                let rx_s = rx.saturating_sub(prev_rx) as f64 / dt / 1_000_000.0;
                let tx_s = tx.saturating_sub(prev_tx) as f64 / dt / 1_000_000.0;
                (rx_s, tx_s)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    let mut map = state.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    map.insert(iface.to_string(), (rx, tx));
    *state = Some((map, now));
    speeds
}

fn read_interfaces() -> io::Result<Vec<NetworkInterfaceSnapshot>> {
    let addresses = read_interface_addresses().unwrap_or_default();
    let mut interfaces = fs::read_dir("/sys/class/net")?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            interface_snapshot(&path, name, &addresses).ok()
        })
        .collect::<Vec<_>>();
    interfaces.retain(|iface| {
        let n = iface.name.as_str();
        (n.starts_with("en") || n.starts_with("eth") || n.starts_with("wl")) && !n.starts_with("wg") // WireGuard is VPN, handled separately
    });
    interfaces.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.state.cmp(&right.state))
    });
    Ok(interfaces)
}

fn interface_snapshot(
    path: &Path,
    name: String,
    addresses: &HashMap<String, InterfaceAddresses>,
) -> io::Result<NetworkInterfaceSnapshot> {
    let state = fs::read_to_string(path.join("operstate"))?
        .trim()
        .to_string();
    let interface_addresses = addresses.get(&name).cloned().unwrap_or_default();
    let mac_address = fs::read_to_string(path.join("address"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "00:00:00:00:00:00");
    Ok(NetworkInterfaceSnapshot {
        name,
        state,
        is_wireless: path.join("wireless").exists(),
        mac_address,
        ipv4_addresses: interface_addresses.ipv4,
        ipv6_addresses: interface_addresses.ipv6,
    })
}

#[derive(Debug, Clone, Default)]
struct InterfaceAddresses {
    ipv4: Vec<String>,
    ipv6: Vec<String>,
}

fn read_interface_addresses() -> io::Result<HashMap<String, InterfaceAddresses>> {
    let mut addresses = HashMap::new();
    merge_ip_addr_output(
        &mut addresses,
        &command_stdout("ip", &["-o", "-4", "addr", "show"])?,
        AddressFamily::Ipv4,
    );
    if let Ok(output) = command_stdout("ip", &["-o", "-6", "addr", "show"]) {
        merge_ip_addr_output(&mut addresses, &output, AddressFamily::Ipv6);
    }
    Ok(addresses)
}

fn command_stdout(program: &str, args: &[&str]) -> io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other("command failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug, Clone, Copy)]
enum AddressFamily {
    Ipv4,
    Ipv6,
}

fn merge_ip_addr_output(
    addresses: &mut HashMap<String, InterfaceAddresses>,
    output: &str,
    family: AddressFamily,
) {
    for line in output.lines() {
        let Some((name, address)) = parse_ip_addr_line(line) else {
            continue;
        };
        let entry = addresses.entry(name).or_default();
        match family {
            AddressFamily::Ipv4 => entry.ipv4.push(address),
            AddressFamily::Ipv6 => entry.ipv6.push(address),
        }
    }
}

fn parse_ip_addr_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    parts.next()?;
    let name = parts.next()?.trim_end_matches(':').to_string();
    let address = parts.find(|part| part.contains('/'))?.to_string();
    Some((name, address))
}

fn read_wifi_networks() -> io::Result<Vec<WifiNetworkSnapshot>> {
    #[cfg(feature = "gui")]
    {
        if let Some(networks) = nm_dbus::read_wifi_networks() {
            return Ok(networks);
        }
    }
    let output = command_stdout(
        "nmcli",
        &[
            "-t",
            "-f",
            "IN-USE,SSID,SIGNAL,SECURITY",
            "dev",
            "wifi",
            "list",
        ],
    )?;
    Ok(parse_nmcli_wifi_list(&output))
}

fn parse_nmcli_wifi_list(output: &str) -> Vec<WifiNetworkSnapshot> {
    // nmcli escapes literal ':' inside fields as "\:"; split on unescaped ':'.
    fn split_fields(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    if let Some(&next) = chars.peek() {
                        current.push(next);
                        chars.next();
                    }
                }
                ':' => fields.push(std::mem::take(&mut current)),
                _ => current.push(ch),
            }
        }
        fields.push(current);
        fields
    }

    let mut networks: Vec<WifiNetworkSnapshot> = Vec::new();
    for line in output.lines() {
        let fields = split_fields(line);
        if fields.len() < 4 {
            continue;
        }
        let active = fields[0].trim() == "*";
        let ssid = fields[1].trim();
        if ssid.is_empty() {
            continue;
        }
        let signal_percent = fields[2].trim().parse::<u8>().ok().filter(|v| *v <= 100);
        let security = Some(fields[3].trim())
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        // Deduplicate by SSID, keeping the strongest / active entry.
        if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ssid) {
            existing.active |= active;
            if signal_percent.unwrap_or(0) > existing.signal_percent.unwrap_or(0) {
                existing.signal_percent = signal_percent;
            }
            if existing.security.is_none() {
                existing.security = security;
            }
            continue;
        }

        networks.push(WifiNetworkSnapshot {
            ssid: ssid.to_string(),
            signal_percent,
            security,
            active,
        });
    }

    // Active first, then by descending signal strength.
    networks.sort_by(|a, b| {
        b.active.cmp(&a.active).then(
            b.signal_percent
                .unwrap_or(0)
                .cmp(&a.signal_percent.unwrap_or(0)),
        )
    });
    networks
}

fn read_vpn_connections() -> io::Result<Vec<VpnConnectionSnapshot>> {
    #[cfg(feature = "gui")]
    {
        if let Some(connections) = nm_dbus::read_vpn_connections() {
            return Ok(connections);
        }
    }
    let output = command_stdout(
        "nmcli",
        &["-t", "-f", "NAME,TYPE", "connection", "show", "--active"],
    )?;
    Ok(parse_nmcli_active_vpn_connections(&output))
}

fn parse_nmcli_active_vpn_connections(output: &str) -> Vec<VpnConnectionSnapshot> {
    output
        .lines()
        .filter_map(|line| {
            let (name, kind) = line.split_once(':')?;
            let kind = kind.trim();
            if !matches!(kind, "vpn" | "wireguard") {
                return None;
            }
            Some(VpnConnectionSnapshot {
                name: name.trim().to_string(),
                kind: kind.to_string(),
            })
        })
        .collect()
}

fn read_dns_servers() -> io::Result<Vec<String>> {
    let contents = fs::read_to_string("/etc/resolv.conf")?;
    Ok(parse_resolv_conf_nameservers(&contents))
}

fn parse_resolv_conf_nameservers(contents: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            if parts.next()? != "nameserver" {
                return None;
            }
            parts.next().map(str::to_string)
        })
        .collect()
}

#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub fn parse_security_flags(flags: u32, wpa_flags: u32, rsn_flags: u32) -> Option<String> {
    if (rsn_flags & 0x400) != 0 {
        Some("WPA3".to_string())
    } else if rsn_flags != 0 {
        Some("WPA2".to_string())
    } else if wpa_flags != 0 {
        Some("WPA1".to_string())
    } else if (flags & 1) != 0 {
        Some("WEP".to_string())
    } else {
        None
    }
}

#[cfg(feature = "gui")]
mod nm_dbus {
    use std::collections::HashMap;

    use gtk::gio;
    use gtk::glib::{self, variant::ToVariant};

    use super::{VpnConnectionSnapshot, WifiNetworkSnapshot, parse_security_flags};

    const NM_DEST: &str = "org.freedesktop.NetworkManager";
    const NM_PATH: &str = "/org/freedesktop/NetworkManager";
    const NM_IFACE: &str = "org.freedesktop.NetworkManager";
    const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";
    const DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
    const WIRELESS_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
    const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
    const ACTIVE_CONN_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";

    fn system_bus() -> Option<gio::DBusConnection> {
        gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()
    }

    fn get_prop(
        conn: &gio::DBusConnection,
        dest: &str,
        path: &str,
        iface: &str,
        prop: &str,
    ) -> Option<glib::Variant> {
        let params = (iface, prop).to_variant();
        let reply = conn
            .call_sync(
                Some(dest),
                path,
                PROPS_IFACE,
                "Get",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .ok()?;
        reply.child_value(0).as_variant()
    }

    fn get_all_props(
        conn: &gio::DBusConnection,
        dest: &str,
        path: &str,
        iface: &str,
    ) -> Option<HashMap<String, glib::Variant>> {
        let params = (iface,).to_variant();
        let reply = conn
            .call_sync(
                Some(dest),
                path,
                PROPS_IFACE,
                "GetAll",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            )
            .ok()?;
        let dict = reply.child_value(0);
        let mut map = HashMap::new();
        for i in 0..dict.n_children() {
            let entry = dict.child_value(i);
            if let Some(key) = entry.child_value(0).str()
                && let Some(val) = entry.child_value(1).as_variant()
            {
                map.insert(key.to_string(), val);
            }
        }
        Some(map)
    }

    pub fn read_vpn_connections() -> Option<Vec<VpnConnectionSnapshot>> {
        let conn = system_bus()?;
        let active_conns_variant =
            get_prop(&conn, NM_DEST, NM_PATH, NM_IFACE, "ActiveConnections")?;
        let mut vpns = Vec::new();

        for i in 0..active_conns_variant.n_children() {
            let path_var = active_conns_variant.child_value(i);
            let Some(path) = path_var.str() else {
                continue;
            };
            let props = match get_all_props(&conn, NM_DEST, path, ACTIVE_CONN_IFACE) {
                Some(p) => p,
                None => continue,
            };
            let name = props
                .get("Id")
                .and_then(|v| v.str())
                .unwrap_or_default()
                .to_string();
            let mut kind = props
                .get("Type")
                .and_then(|v| v.str())
                .unwrap_or_default()
                .to_string();
            let is_vpn = props
                .get("Vpn")
                .and_then(|v| v.get::<bool>())
                .unwrap_or(false);

            if is_vpn || kind == "vpn" || kind == "wireguard" {
                if is_vpn && kind != "wireguard" && kind != "vpn" {
                    kind = "vpn".to_string();
                }
                vpns.push(VpnConnectionSnapshot { name, kind });
            }
        }

        Some(vpns)
    }

    pub fn read_wifi_networks() -> Option<Vec<WifiNetworkSnapshot>> {
        let conn = system_bus()?;
        let devices_variant = get_prop(&conn, NM_DEST, NM_PATH, NM_IFACE, "Devices")?;
        let mut networks = Vec::new();

        for i in 0..devices_variant.n_children() {
            let dev_var = devices_variant.child_value(i);
            let Some(dev_path) = dev_var.str() else {
                continue;
            };
            let dev_type = get_prop(&conn, NM_DEST, dev_path, DEVICE_IFACE, "DeviceType")
                .and_then(|v| v.get::<u32>());
            if dev_type != Some(2) {
                continue;
            }

            let active_ap_path =
                get_prop(&conn, NM_DEST, dev_path, WIRELESS_IFACE, "ActiveAccessPoint")
                    .and_then(|v| v.str().map(str::to_string));

            let ap_list_var = conn
                .call_sync(
                    Some(NM_DEST),
                    dev_path,
                    WIRELESS_IFACE,
                    "GetAllAccessPoints",
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    1000,
                    gio::Cancellable::NONE,
                )
                .ok()
                .map(|reply| reply.child_value(0))
                .or_else(|| get_prop(&conn, NM_DEST, dev_path, WIRELESS_IFACE, "AccessPoints"))?;

            for j in 0..ap_list_var.n_children() {
                let ap_var = ap_list_var.child_value(j);
                let Some(ap_path) = ap_var.str() else {
                    continue;
                };
                let props = match get_all_props(&conn, NM_DEST, ap_path, AP_IFACE) {
                    Some(p) => p,
                    None => continue,
                };

                let ssid = props
                    .get("Ssid")
                    .map(|val| {
                        let bytes: Vec<u8> = (0..val.n_children())
                            .filter_map(|k| val.child_value(k).get::<u8>())
                            .collect();
                        String::from_utf8_lossy(&bytes).trim().to_string()
                    })
                    .unwrap_or_default();
                if ssid.is_empty() {
                    continue;
                }

                let signal_percent = props
                    .get("Strength")
                    .and_then(|v| v.get::<u8>())
                    .filter(|&v| v <= 100);
                let flags = props.get("Flags").and_then(|v| v.get::<u32>()).unwrap_or(0);
                let wpa_flags = props
                    .get("WpaFlags")
                    .and_then(|v| v.get::<u32>())
                    .unwrap_or(0);
                let rsn_flags = props
                    .get("RsnFlags")
                    .and_then(|v| v.get::<u32>())
                    .unwrap_or(0);
                let security = parse_security_flags(flags, wpa_flags, rsn_flags);
                let active = active_ap_path.as_deref() == Some(ap_path);

                if let Some(existing) = networks.iter_mut().find(|n: &&mut WifiNetworkSnapshot| n.ssid == ssid) {
                    existing.active |= active;
                    if signal_percent.unwrap_or(0) > existing.signal_percent.unwrap_or(0) {
                        existing.signal_percent = signal_percent;
                    }
                    if existing.security.is_none() {
                        existing.security = security;
                    }
                    continue;
                }

                networks.push(WifiNetworkSnapshot {
                    ssid,
                    signal_percent,
                    security,
                    active,
                });
            }
        }

        networks.sort_by(|a, b| {
            b.active.cmp(&a.active).then(
                b.signal_percent
                    .unwrap_or(0)
                    .cmp(&a.signal_percent.unwrap_or(0)),
            )
        });
        Some(networks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_addr_parser_extracts_interface_and_address() {
        let line = "2: wlan0    inet 192.168.1.10/24 brd 192.168.1.255 scope global wlan0";
        assert_eq!(
            parse_ip_addr_line(line),
            Some(("wlan0".to_string(), "192.168.1.10/24".to_string()))
        );
    }

    #[test]
    fn ip_addr_output_merges_by_family() {
        let mut addresses = HashMap::new();
        merge_ip_addr_output(
            &mut addresses,
            "2: wlan0    inet 192.168.1.10/24 scope global wlan0",
            AddressFamily::Ipv4,
        );
        merge_ip_addr_output(
            &mut addresses,
            "2: wlan0    inet6 fe80::1/64 scope link",
            AddressFamily::Ipv6,
        );

        let wlan0 = addresses.get("wlan0").unwrap();
        assert_eq!(wlan0.ipv4, vec!["192.168.1.10/24"]);
        assert_eq!(wlan0.ipv6, vec!["fe80::1/64"]);
    }

    #[test]
    fn nmcli_wifi_parser_extracts_networks() {
        let networks =
            parse_nmcli_wifi_list("*:Home:87:WPA2\n :Cafe:42:\n :Home:55:WPA2\n ::10:WPA1\n");

        assert_eq!(
            networks,
            vec![
                // Active first, deduped (Home kept strongest signal), then by signal.
                WifiNetworkSnapshot {
                    ssid: "Home".to_string(),
                    signal_percent: Some(87),
                    security: Some("WPA2".to_string()),
                    active: true,
                },
                WifiNetworkSnapshot {
                    ssid: "Cafe".to_string(),
                    signal_percent: Some(42),
                    security: None,
                    active: false,
                },
            ]
        );
    }

    #[test]
    fn nmcli_active_connection_parser_filters_vpn() {
        let vpns = parse_nmcli_active_vpn_connections("Home:wifi\nWork VPN:vpn\nwg0:wireguard\n");

        assert_eq!(
            vpns,
            vec![
                VpnConnectionSnapshot {
                    name: "Work VPN".to_string(),
                    kind: "vpn".to_string(),
                },
                VpnConnectionSnapshot {
                    name: "wg0".to_string(),
                    kind: "wireguard".to_string(),
                },
            ]
        );
    }

    #[test]
    fn resolv_conf_parser_extracts_nameservers() {
        let servers = parse_resolv_conf_nameservers(
            "\
# Generated
nameserver 1.1.1.1
search lan
nameserver 2001:4860:4860::8888
",
        );

        assert_eq!(
            servers,
            vec!["1.1.1.1".to_string(), "2001:4860:4860::8888".to_string()]
        );
    }

    #[test]
    fn parse_security_flags_identifies_wpa_versions() {
        assert_eq!(parse_security_flags(0, 0, 0x400), Some("WPA3".to_string()));
        assert_eq!(parse_security_flags(3, 0, 392), Some("WPA2".to_string()));
        assert_eq!(parse_security_flags(0, 1, 0), Some("WPA1".to_string()));
        assert_eq!(parse_security_flags(1, 0, 0), Some("WEP".to_string()));
        assert_eq!(parse_security_flags(0, 0, 0), None);
    }

    #[test]
    fn network_snapshot_produces_consistent_results() {
        let snapshot = network_snapshot();
        for iface in &snapshot.interfaces {
            assert!(!iface.name.is_empty());
        }
    }

    #[test]
    #[cfg(feature = "gui")]
    fn nm_dbus_queries_without_panic() {
        let _ = nm_dbus::read_vpn_connections();
        let _ = nm_dbus::read_wifi_networks();
    }
}
