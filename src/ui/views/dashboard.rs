use chrono::Local;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, Grid, Image, Label, Orientation,
    ProgressBar,
};
use crate::{AudioSnapshot, BatterySnapshot, MediaSnapshot, NetworkSnapshot, NotificationSnapshot, SystemSnapshot};
pub use super::system_monitor::{MetricGraph, metric_graph, push_metric_graph, load_fraction};

#[derive(Clone)]
pub struct DashboardView {
    pub root: GtkBox,
    pub clock: Label,
    pub date: Label,
    pub uptime: Label,
    pub battery: Label,
    pub processes: Label,
    pub load: Label,
    pub load_sub: Label,
    pub memory: Label,
    pub memory_sub: Label,
    pub disk: Label,
    pub disk_sub: Label,
    pub thermal: Label,
    pub network: Label,
    pub audio: Label,
    pub media: Label,
    pub notifications: Label,
    pub load_bar: ProgressBar,
    pub memory_bar: ProgressBar,
    pub disk_bar: ProgressBar,
    pub load_graph: MetricGraph,
    pub memory_graph: MetricGraph,
    pub disk_graph: MetricGraph,
    pub open_audio: Button,
    pub open_network: Button,
    pub open_media: Button,
    pub open_ai: Button,
    pub open_system: Button,
    pub open_notifications: Button,
    pub toggle_wifi: Button,
    pub toggle_bluetooth: Button,
    pub toggle_dnd: Button,
    pub toggle_mute: Button,
    pub lock: Button,
    pub suspend: Button,
}

pub fn dashboard_view(snapshot: &SystemSnapshot) -> DashboardView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.set_margin_top(14);
    root.set_margin_bottom(14);
    root.set_margin_start(14);
    root.set_margin_end(14);

    // ── Header ────────────────────────────────────────────────────────────────
    let header = GtkBox::new(Orientation::Horizontal, 10);
    header.set_margin_bottom(12);

    let clock = Label::new(None);
    clock.add_css_class("dashboard-clock");
    clock.set_xalign(0.0);

    let date = Label::new(None);
    date.add_css_class("dashboard-date");
    date.set_xalign(0.0);

    let clock_block = GtkBox::new(Orientation::Vertical, 0);
    clock_block.set_hexpand(true);
    clock_block.set_valign(gtk::Align::Center);
    clock_block.append(&clock);
    clock_block.append(&date);
    header.append(&clock_block);

    let stats_row = GtkBox::new(Orientation::Horizontal, 6);
    stats_row.set_valign(gtk::Align::Center);

    let uptime = dashboard_stat_chip();
    let battery = dashboard_stat_chip();
    battery.set_visible(false);
    let processes = dashboard_stat_chip();
    stats_row.append(&uptime);
    stats_row.append(&battery);
    stats_row.append(&processes);
    header.append(&stats_row);
    root.append(&header);

    // ── Metric row (4 cards side by side) ────────────────────────────────────
    let metric_row = GtkBox::new(Orientation::Horizontal, 8);
    metric_row.set_margin_bottom(8);

    let (load_card, load, load_sub, load_bar) =
        crate::ui::metric_card("CPU", "utilities-system-monitor-symbolic");
    let (memory_card, memory, memory_sub, memory_bar) =
        crate::ui::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, disk_sub, disk_bar) =
        crate::ui::metric_card("Disk", "drive-harddisk-symbolic");

    let thermal_card = GtkBox::new(Orientation::Vertical, 4);
    thermal_card.add_css_class("dashboard-card");
    thermal_card.set_hexpand(true);
    let thermal_header = GtkBox::new(Orientation::Horizontal, 6);
    let thermal_icon = crate::ui::icons::fa_icon("weather-clear-symbolic", 14);
    let thermal_title = Label::new(Some("Temp"));
    thermal_title.add_css_class("dashboard-card-title");
    thermal_title.set_hexpand(true);
    thermal_title.set_xalign(0.0);
    thermal_header.append(&thermal_icon);
    thermal_header.append(&thermal_title);
    thermal_card.append(&thermal_header);
    let thermal = Label::new(Some("—"));
    thermal.add_css_class("dashboard-metric-value");
    thermal.set_xalign(0.0);
    thermal_card.append(&thermal);

    let load_graph = metric_graph();
    let memory_graph = metric_graph();
    let disk_graph = metric_graph();
    load_card.append(&load_graph.area);
    memory_card.append(&memory_graph.area);
    disk_card.append(&disk_graph.area);

    metric_row.append(&load_card);
    metric_row.append(&memory_card);
    metric_row.append(&disk_card);
    metric_row.append(&thermal_card);
    root.append(&metric_row);

    // ── Control grid (2×2) ────────────────────────────────────────────────────
    let control_grid = dashboard_grid();

    let (network_card, network, network_row) =
        crate::ui::control_card("Network", "network-wireless-symbolic");
    let (audio_card, audio, audio_row) =
        crate::ui::control_card("Audio", "audio-volume-high-symbolic");
    let (media_card, media, media_row) =
        crate::ui::control_card("Media", "media-playback-start-symbolic");
    let (notifications_card, notifications, notify_row) =
        crate::ui::control_card("Notifications", "preferences-system-notifications-symbolic");

    let open_network = dashboard_button("Open");
    let toggle_wifi = dashboard_button("Wi-Fi");
    network_row.append(&open_network);
    network_row.append(&toggle_wifi);

    let open_audio = dashboard_button("Mixer");
    let toggle_mute = dashboard_button("Mute");
    audio_row.append(&open_audio);
    audio_row.append(&toggle_mute);

    let open_media = dashboard_button("Open");
    media_row.append(&open_media);

    let open_notifications = dashboard_button("Notify");
    let toggle_dnd = dashboard_button("DND");
    notify_row.append(&open_notifications);
    notify_row.append(&toggle_dnd);

    control_grid.attach(&network_card, 0, 0, 1, 1);
    control_grid.attach(&audio_card, 1, 0, 1, 1);
    control_grid.attach(&media_card, 0, 1, 1, 1);
    control_grid.attach(&notifications_card, 1, 1, 1, 1);
    root.append(&control_grid);

    // ── Quick actions row ─────────────────────────────────────────────────────
    let quick_row = GtkBox::new(Orientation::Horizontal, 6);
    quick_row.set_margin_top(8);
    let open_ai = dashboard_button("AI Chat");
    let open_system = dashboard_button("System Monitor");
    let lock = dashboard_button("Lock");
    let suspend = dashboard_button("Suspend");
    let toggle_bluetooth = dashboard_button("Bluetooth");
    quick_row.append(&open_ai);
    quick_row.append(&open_system);
    quick_row.append(&lock);
    quick_row.append(&suspend);
    quick_row.append(&toggle_bluetooth);
    root.append(&quick_row);

    let view = DashboardView {
        root,
        clock,
        date,
        uptime,
        battery,
        processes,
        load,
        load_sub,
        memory,
        memory_sub,
        disk,
        disk_sub,
        thermal,
        network,
        audio,
        media,
        notifications,
        load_bar,
        memory_bar,
        disk_bar,
        load_graph,
        memory_graph,
        disk_graph,
        open_audio,
        open_network,
        open_media,
        open_ai,
        open_system,
        open_notifications,
        toggle_wifi,
        toggle_bluetooth,
        toggle_dnd,
        toggle_mute,
        lock,
        suspend,
    };
    set_dashboard_snapshot(&view, snapshot);
    set_dashboard_network_snapshot(&view, &crate::network_snapshot());
    set_dashboard_battery_snapshot(&view, &crate::battery_snapshot());
    set_dashboard_audio_snapshot(&view, &crate::audio_snapshot());
    set_dashboard_media_snapshot(&view, &crate::media_snapshot());
    set_dashboard_notification_snapshot(&view, &crate::notification_snapshot());
    view
}


pub fn set_dashboard_snapshot(view: &DashboardView, snapshot: &SystemSnapshot) {
    let now = Local::now();
    view.clock.set_text(&now.format("%H:%M:%S").to_string());
    view.date.set_text(&now.format("%A, %d %B %Y").to_string());
    view.uptime.set_text(
        &snapshot
            .uptime_seconds
            .map(format_duration)
            .unwrap_or("unknown".to_string()),
    );
    view.load.set_text(
        &snapshot
            .load_average
            .map(|load| format!("{load:.2}"))
            .unwrap_or_else(|| "—".to_string()),
    );
    view.load_sub.set_text(
        &snapshot
            .cpu_count
            .map(|n| format!("{n} cores"))
            .unwrap_or_default(),
    );
    let load_fraction = snapshot.load_average.map(load_fraction).unwrap_or_default();
    view.load_bar.set_fraction(load_fraction);
    push_metric_graph(&view.load_graph, load_fraction);

    let memory_fraction = snapshot
        .memory_used_percent()
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    view.memory.set_text(
        &snapshot
            .memory_used_percent()
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_else(|| "—".to_string()),
    );
    view.memory_sub.set_text(
        &snapshot
            .memory_used_percent()
            .map(|_| {
                let used = snapshot.memory_used_kib().unwrap_or_default() / 1024;
                let total = snapshot.memory_total_kib.unwrap_or_default() / 1024;
                format!("{used} / {total} MiB")
            })
            .unwrap_or_default(),
    );
    view.memory_bar.set_fraction(memory_fraction);
    push_metric_graph(&view.memory_graph, memory_fraction);

    let disk_fraction = snapshot
        .disk_used_percent()
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    view.disk.set_text(
        &snapshot
            .disk_used_percent()
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_else(|| "—".to_string()),
    );
    view.disk_sub.set_text(
        &snapshot
            .disk_used_percent()
            .map(|_| {
                let used = snapshot.disk_used_kib.unwrap_or_default() / (1024 * 1024);
                let total = snapshot.disk_total_kib.unwrap_or_default() / (1024 * 1024);
                format!("{used} / {total} GiB")
            })
            .unwrap_or_default(),
    );
    view.disk_bar.set_fraction(disk_fraction);
    push_metric_graph(&view.disk_graph, disk_fraction);

    view.processes.set_text(
        &snapshot
            .process_count
            .map(|count| format!("{count} proc"))
            .unwrap_or_default(),
    );
}

pub fn set_dashboard_thermal(view: &DashboardView, celsius: Option<f32>) {
    if let Some(t) = celsius {
        view.thermal.set_text(&format!("{t:.0} °C"));
    } else {
        view.thermal.set_text("—");
    }
}

pub fn set_dashboard_network_snapshot(view: &DashboardView, snapshot: &NetworkSnapshot) {
    let selected = snapshot
        .interfaces
        .iter()
        .find(|interface| interface.name != "lo" && interface.state == "up")
        .or_else(|| {
            snapshot
                .interfaces
                .iter()
                .find(|interface| interface.name != "lo")
        });

    let Some(interface) = selected else {
        view.network.set_text("unknown");
        return;
    };

    let address = interface
        .ipv4_addresses
        .first()
        .or_else(|| interface.ipv6_addresses.first())
        .map(String::as_str)
        .unwrap_or("no address");
    let kind = if interface.is_wireless {
        "Wi-Fi"
    } else {
        "Interface"
    };
    view.network.set_text(&format!(
        "{kind} {}  {}  {address}",
        interface.name, interface.state
    ));
}

pub fn set_dashboard_battery_snapshot(view: &DashboardView, snapshot: &BatterySnapshot) {
    let Some(battery) = snapshot.primary() else {
        view.battery.set_visible(false);
        return;
    };
    let capacity = battery
        .capacity_percent
        .map(|value| format!("{value}%"))
        .unwrap_or_default();
    let status = battery.status.as_deref().unwrap_or("");
    view.battery.set_text(&format!("⚡ {capacity} {status}").trim().to_string());
    view.battery.set_visible(true);
}

pub fn set_dashboard_audio_snapshot(view: &DashboardView, snapshot: &AudioSnapshot) {
    let output = snapshot
        .output
        .as_ref()
        .map(|device| {
            let muted = if device.muted { " muted" } else { "" };
            format!("out {}%{muted}", device.volume_percent)
        })
        .unwrap_or("out unknown".to_string());
    let input = snapshot
        .input
        .as_ref()
        .map(|device| {
            let muted = if device.muted { " muted" } else { "" };
            format!("mic {}%{muted}", device.volume_percent)
        })
        .unwrap_or("mic unknown".to_string());
    view.audio.set_text(&format!("{output}  {input}"));
}


pub fn set_dashboard_media_snapshot(view: &DashboardView, snapshot: &MediaSnapshot) {
    if !snapshot.is_active() {
        view.media.set_text("no active player");
        return;
    }

    let status = snapshot.status.as_deref().unwrap_or("Playing");
    let title = match (&snapshot.artist, &snapshot.title) {
        (Some(artist), Some(title)) => format!("{artist} - {title}"),
        (_, Some(title)) => title.clone(),
        (Some(artist), _) => artist.clone(),
        _ => "Unknown track".to_string(),
    };
    let player = snapshot.player.as_deref().unwrap_or("MPRIS");
    view.media.set_text(&format!("{status}  {player}  {title}"));
}

pub fn set_dashboard_notification_snapshot(view: &DashboardView, snapshot: &NotificationSnapshot) {
    if !snapshot.is_available() {
        view.notifications.set_text("not detected");
        return;
    }

    let backend = snapshot.backend.as_deref().unwrap_or("notifications");
    let count = snapshot
        .count
        .map(|count| format!("{count} history"))
        .unwrap_or("unknown history".to_string());
    let dnd = snapshot
        .dnd
        .map(|enabled| if enabled { "DND on" } else { "DND off" })
        .unwrap_or("DND unknown");
    view.notifications
        .set_text(&format!("{backend}  {count}  {dnd}"));
}


pub(crate) fn dashboard_stat_chip() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-stat-chip");
    label.set_xalign(0.5);
    label.set_valign(gtk::Align::Center);
    label
}

pub(crate) fn dashboard_value_label() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-clock");
    label.set_xalign(0.0);
    label
}

pub(crate) fn dashboard_subtitle_label() -> Label {
    let label = Label::new(None);
    label.add_css_class("result-subtitle");
    label.set_xalign(0.0);
    label
}

pub(crate) fn dashboard_grid() -> Grid {
    let grid = Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);
    grid
}

pub(crate) fn dashboard_plain_card(title: &str, icon_name: &str) -> GtkBox {
    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("dashboard-card");
    card.set_hexpand(true);

    let header = GtkBox::new(Orientation::Horizontal, 8);
    header.set_hexpand(true);

    let icon = Image::from_icon_name(icon_name);
    icon.add_css_class("result-icon");
    icon.set_pixel_size(18);
    header.append(&icon);

    let title = Label::new(Some(title));
    title.add_css_class("dashboard-card-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&title);

    card.append(&header);
    card
}


pub(crate) fn dashboard_card_value() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-card-value");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_wrap(false);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

pub(crate) fn dashboard_card_actions() -> GtkBox {
    let actions = GtkBox::new(Orientation::Horizontal, 6);
    actions.add_css_class("dashboard-card-actions");
    actions
}

pub(crate) fn dashboard_button(label: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("dashboard-button");
    button
}



pub(crate) fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}
