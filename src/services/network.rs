use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterfaceSnapshot>,
    pub wifi_networks: Vec<WifiNetworkSnapshot>,
    pub vpn_connections: Vec<VpnConnectionSnapshot>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone)]
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

static NET_SPEED_STATE: Mutex<Option<(HashMap<String, (u64, u64)>, Instant)>> = Mutex::new(None);

/// Returns (rx_mbps, tx_mbps) for the given interface, using a static to track deltas.
pub fn net_speed_mbps(iface: &str) -> (f64, f64) {
    let rx = fs::read_to_string(format!("/sys/class/net/{iface}/statistics/rx_bytes"))
        .ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    let tx = fs::read_to_string(format!("/sys/class/net/{iface}/statistics/tx_bytes"))
        .ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    let now = Instant::now();

    let mut state = NET_SPEED_STATE.lock().unwrap();
    let speeds = if let Some((prev_map, prev_time)) = state.as_ref() {
        let dt = now.duration_since(*prev_time).as_secs_f64();
        if dt > 0.2 {
            if let Some(&(prev_rx, prev_tx)) = prev_map.get(iface) {
                let rx_s = rx.saturating_sub(prev_rx) as f64 / dt / 1_000_000.0;
                let tx_s = tx.saturating_sub(prev_tx) as f64 / dt / 1_000_000.0;
                (rx_s, tx_s)
            } else { (0.0, 0.0) }
        } else { (0.0, 0.0) }
    } else { (0.0, 0.0) };

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
        return Err(io::Error::new(io::ErrorKind::Other, "command failed"));
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
    let output = command_stdout(
        "nmcli",
        &["-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"],
    )?;
    Ok(parse_nmcli_wifi_list(&output))
}

fn parse_nmcli_wifi_list(output: &str) -> Vec<WifiNetworkSnapshot> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ':');
            let ssid = parts.next()?.trim();
            if ssid.is_empty() {
                return None;
            }
            let signal_percent = parts
                .next()
                .and_then(|value| value.trim().parse::<u8>().ok())
                .filter(|value| *value <= 100);
            let security = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Some(WifiNetworkSnapshot {
                ssid: ssid.to_string(),
                signal_percent,
                security,
            })
        })
        .collect()
}

fn read_vpn_connections() -> io::Result<Vec<VpnConnectionSnapshot>> {
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
        let networks = parse_nmcli_wifi_list("Home:87:WPA2\nCafe:42:\n:10:WPA1\n");

        assert_eq!(
            networks,
            vec![
                WifiNetworkSnapshot {
                    ssid: "Home".to_string(),
                    signal_percent: Some(87),
                    security: Some("WPA2".to_string()),
                },
                WifiNetworkSnapshot {
                    ssid: "Cafe".to_string(),
                    signal_percent: Some(42),
                    security: None,
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
}
