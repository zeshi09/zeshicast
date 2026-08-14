use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, Label, ListBox, Orientation};

use crate::{NetworkInterfaceSnapshot, NetworkSnapshot};

#[derive(Clone)]
pub struct NetworkView {
    pub root: GtkBox,
    pub list: ListBox,
    pub connect_wifi: Button,
    pub disconnect: Button,
    pub copy_ip: Button,
    pub copy_mac: Button,
}


pub fn network_view(snapshot: &NetworkSnapshot) -> NetworkView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Section headers ("Ethernet", "Wi-Fi") live inside the list so their order
    // is data-driven (Ethernet is shown first when a wired link is present).
    let list = crate::ui::results_list();
    set_network_snapshot(&list, snapshot);

    let scroller = crate::ui::scrollable_list(&list);
    root.append(&scroller);

    // Action buttons (reachable from keyboard shortcuts)
    let connect_wifi = Button::new();
    let disconnect = Button::new();
    let copy_ip = Button::new();
    let copy_mac = Button::new();
    connect_wifi.set_visible(false);
    disconnect.set_visible(false);
    copy_ip.set_visible(false);
    copy_mac.set_visible(false);

    NetworkView {
        root,
        list,
        connect_wifi,
        disconnect,
        copy_ip,
        copy_mac,
    }
}


pub fn set_network_snapshot(list: &ListBox, snapshot: &NetworkSnapshot) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    // ── Ethernet section (shown first when a wired link exists) ───────────────
    // Not in the mockup, but requested: surface wired interfaces in the same
    // style above Wi-Fi. `snapshot.interfaces` is already filtered to physical
    // en*/eth*/wl* devices, so we just keep the non-wireless ones here.
    let wired: Vec<&NetworkInterfaceSnapshot> = snapshot
        .interfaces
        .iter()
        .filter(|iface| !iface.is_wireless)
        .collect();

    if !wired.is_empty() {
        list.append(&crate::ui::section_header("Ethernet"));
        for iface in wired {
            list.append(&ethernet_row(iface));
        }
    }

    // ── Wi-Fi section ─────────────────────────────────────────────────────────
    list.append(&crate::ui::section_header("Wi-Fi"));
    for network in &snapshot.wifi_networks {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");
        if network.active {
            row.add_css_class("network-active");
        }

        let layout = GtkBox::new(Orientation::Horizontal, 10);
        layout.set_margin_start(14);
        layout.set_margin_end(14);
        layout.set_valign(gtk::Align::Center);

        let sig = network.signal_percent.unwrap_or(0);
        layout.append(&signal_bars(sig as u32));

        let text = GtkBox::new(Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);

        let title = Label::new(Some(&network.ssid));
        title.add_css_class("result-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let subtitle = Label::new(None);
        subtitle.set_xalign(0.0);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if network.active {
            subtitle.set_text(&format!("Connected  ·  {sig}%"));
            subtitle.add_css_class("network-status-connected");
        } else {
            let security = network.security.as_deref().unwrap_or("Open");
            subtitle.set_text(&format!("{security}  ·  {sig}%"));
            subtitle.add_css_class("result-subtitle");
        }

        text.append(&title);
        text.append(&subtitle);
        layout.append(&text);

        let btn = Button::with_label(if network.active {
            "Disconnect"
        } else {
            "Connect"
        });
        btn.add_css_class(if network.active {
            "network-disconnect-btn"
        } else {
            "network-connect-btn"
        });
        btn.set_valign(gtk::Align::Center);
        layout.append(&btn);

        row.set_child(Some(&layout));
        list.append(&row);
    }

    if snapshot.wifi_networks.is_empty() {
        list.append(&crate::ui::secondary_action_row(
            "network-wireless-offline-symbolic",
            "No Wi-Fi networks found",
        ));
    }

    if !snapshot.vpn_connections.is_empty() {
        list.append(&crate::ui::secondary_action_row(
            "network-vpn-symbolic",
            "Active VPN",
        ));
    }

    for vpn in &snapshot.vpn_connections {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");

        let layout = GtkBox::new(Orientation::Horizontal, 10);
        layout.set_margin_top(8);
        layout.set_margin_bottom(8);
        layout.set_margin_start(10);
        layout.set_margin_end(10);

        let icon = gtk::Image::from_icon_name("network-vpn-symbolic");
        icon.set_pixel_size(20);
        icon.add_css_class("result-icon");

        let text = GtkBox::new(Orientation::Vertical, 2);
        text.set_hexpand(true);

        let title = Label::new(Some(&vpn.name));
        title.add_css_class("result-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let subtitle = Label::new(Some(&format!("{} active", vpn.kind)));
        subtitle.add_css_class("result-subtitle");
        subtitle.set_xalign(0.0);
        subtitle.set_hexpand(true);

        text.append(&title);
        text.append(&subtitle);
        layout.append(&icon);
        layout.append(&text);
        row.set_child(Some(&layout));
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}


fn ethernet_row(iface: &NetworkInterfaceSnapshot) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let connected = iface.state.eq_ignore_ascii_case("up") && !iface.ipv4_addresses.is_empty();
    if connected {
        row.add_css_class("network-active");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name("network-wired-symbolic");
    icon.set_pixel_size(16);
    icon.set_size_request(18, -1);
    icon.add_css_class("network-wired-icon");
    layout.append(&icon);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&iface.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let subtitle = Label::new(None);
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if connected {
        let ip = iface.ipv4_addresses[0]
            .split('/')
            .next()
            .unwrap_or(&iface.ipv4_addresses[0]);
        subtitle.set_text(&format!("Connected  ·  {ip}"));
        subtitle.add_css_class("network-status-connected");
    } else {
        subtitle.set_text("Disconnected");
        subtitle.add_css_class("result-subtitle");
    }

    text.append(&title);
    text.append(&subtitle);
    layout.append(&text);

    row.set_child(Some(&layout));
    row
}

fn signal_bars(signal_percent: u32) -> GtkBox {
    let container = GtkBox::new(Orientation::Horizontal, 2);
    container.set_valign(gtk::Align::Center);
    container.set_height_request(14);
    let filled = ((signal_percent as f64 / 100.0) * 4.0).round() as usize;
    for (i, &h) in [4i32, 7, 10, 13].iter().enumerate() {
        let bar = GtkBox::new(Orientation::Vertical, 0);
        bar.set_width_request(3);
        bar.set_height_request(h);
        bar.set_valign(gtk::Align::End);
        bar.add_css_class(if i < filled {
            "signal-bar-filled"
        } else {
            "signal-bar-empty"
        });
        container.append(&bar);
    }
    container
}
