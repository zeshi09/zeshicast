use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, ListBox, Orientation};
use crate::NetworkSnapshot;
use super::dashboard::{dashboard_button, dashboard_card_actions, dashboard_plain_card};

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
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Network");
    root.append(&header);

    let network_card = dashboard_plain_card(
        "Interfaces, Wi-Fi, DNS and VPN",
        "network-wireless-symbolic",
    );
    network_card.set_vexpand(true);
    let list = crate::ui::results_list();
    set_network_snapshot(&list, snapshot);

    let scroller = crate::ui::scrollable_list(&list);
    network_card.append(&scroller);

    let buttons = dashboard_card_actions();
    buttons.set_halign(gtk::Align::End);
    let connect_wifi = dashboard_button("Connect");
    let disconnect = dashboard_button("Disconnect");
    let copy_ip = dashboard_button("Copy IP");
    let copy_mac = dashboard_button("Copy MAC");
    buttons.append(&connect_wifi);
    buttons.append(&disconnect);
    buttons.append(&copy_ip);
    buttons.append(&copy_mac);
    network_card.append(&buttons);
    root.append(&network_card);

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

    for interface in &snapshot.interfaces {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");

        let layout = GtkBox::new(Orientation::Horizontal, 10);
        layout.set_margin_top(8);
        layout.set_margin_bottom(8);
        layout.set_margin_start(10);
        layout.set_margin_end(10);

        let icon_name = if interface.is_wireless {
            "network-wireless-symbolic"
        } else {
            "network-wired-symbolic"
        };
        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(20);
        icon.add_css_class("result-icon");

        let text = GtkBox::new(Orientation::Vertical, 2);
        text.set_hexpand(true);

        let title = Label::new(Some(&interface.name));
        title.add_css_class("result-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);

        let kind = if interface.is_wireless {
            "Wi-Fi"
        } else {
            "Interface"
        };
        let addresses = interface
            .ipv4_addresses
            .iter()
            .chain(interface.ipv6_addresses.iter())
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let details = if addresses.is_empty() {
            interface
                .mac_address
                .as_deref()
                .unwrap_or("no address")
                .to_string()
        } else {
            addresses.join(", ")
        };
        let subtitle = Label::new(Some(&format!("{kind}  {}  {details}", interface.state)));
        subtitle.add_css_class("result-subtitle");
        subtitle.set_xalign(0.0);
        subtitle.set_hexpand(true);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);

        text.append(&title);
        text.append(&subtitle);
        layout.append(&icon);
        layout.append(&text);
        row.set_child(Some(&layout));
        list.append(&row);
    }

    if !snapshot.dns_servers.is_empty() {
        let servers = snapshot
            .dns_servers
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let title = format!("DNS  {servers}");
        list.append(&crate::ui::secondary_action_row(
            "network-server-symbolic",
            &title,
        ));
    }

    if !snapshot.wifi_networks.is_empty() {
        list.append(&crate::ui::secondary_action_row(
            "network-wireless-symbolic",
            "Available Wi-Fi",
        ));
    }

    for network in &snapshot.wifi_networks {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");

        let layout = GtkBox::new(Orientation::Horizontal, 10);
        layout.set_margin_top(8);
        layout.set_margin_bottom(8);
        layout.set_margin_start(10);
        layout.set_margin_end(10);

        let icon = gtk::Image::from_icon_name("network-wireless-symbolic");
        icon.set_pixel_size(20);
        icon.add_css_class("result-icon");

        let text = GtkBox::new(Orientation::Vertical, 2);
        text.set_hexpand(true);

        let title = Label::new(Some(&network.ssid));
        title.add_css_class("result-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let signal = network
            .signal_percent
            .map(|value| format!("{value}%"))
            .unwrap_or("unknown signal".to_string());
        let security = network.security.as_deref().unwrap_or("open");
        let subtitle = Label::new(Some(&format!("{signal}  {security}")));
        subtitle.add_css_class("result-subtitle");
        subtitle.set_xalign(0.0);
        subtitle.set_hexpand(true);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);

        text.append(&title);
        text.append(&subtitle);
        layout.append(&icon);
        layout.append(&text);
        row.set_child(Some(&layout));
        list.append(&row);
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
