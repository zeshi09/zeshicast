use std::cell::RefCell;
use std::rc::Rc;
use gtk::glib;
use chrono::Local;
use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, Grid, Label, Orientation, ProgressBar};

use crate::{
    AudioSnapshot, BatterySnapshot, MediaSnapshot, NetworkSnapshot, NotificationSnapshot,
    SystemSnapshot,
};
use super::system_monitor::{MetricGraph, metric_graph, push_metric_graph, load_fraction, format_duration};

#[derive(Clone)]
pub struct DashboardView {
    pub root: GtkBox,
    pub clock: Label,
    pub date: Label,
    pub uptime: Label,
    pub battery: Label,
    pub processes: Label,
    pub workspace: Label,
    pub load: Label,
    pub load_sub: Label,
    pub memory: Label,
    pub memory_sub: Label,
    pub disk: Label,
    pub disk_sub: Label,
    pub thermal: Label,
    pub network: Label,
    pub network_sub: Label,
    pub audio: Label,
    pub audio_sub: Label,
    pub media: Label,
    pub media_sub: Label,
    pub notifications: Label,
    pub load_bar: ProgressBar,
    pub memory_bar: ProgressBar,
    pub disk_bar: ProgressBar,
    pub thermal_bar: ProgressBar,
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
    // Scrollable outer wrapper
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("dashboard-view");
    root.set_vexpand(true);
    scroll.set_child(Some(&root));

    // ── Clock & date (full width, stacked) ───────────────────────────────────
    let clock_block = GtkBox::new(Orientation::Vertical, 2);
    clock_block.set_margin_bottom(12);

    let clock = Label::new(None);
    clock.add_css_class("dashboard-clock");
    clock.set_xalign(0.0);

    let date = Label::new(None);
    date.add_css_class("dashboard-date");
    date.set_xalign(0.0);

    clock_block.append(&clock);
    clock_block.append(&date);
    root.append(&clock_block);

    // Per-second clock update with blinking colon
    {
        let clock_c = clock.clone();
        let date_c = date.clone();
        let show_colon = Rc::new(RefCell::new(true));
        glib::timeout_add_seconds_local(1, move || {
            let now = Local::now();
            // Blink the colon via alpha only — the ':' glyph always stays so the
            // digits never shift horizontally (constant width, matches mockup).
            let colon_alpha = if *show_colon.borrow() { "65%" } else { "14%" };
            clock_c.set_markup(&format!(
                "{}<span alpha='{}'>:</span>{}",
                now.format("%H"),
                colon_alpha,
                now.format("%M")
            ));
            date_c.set_text(&now.format("%A, %B %-d").to_string());
            *show_colon.borrow_mut() ^= true;
            glib::ControlFlow::Continue
        });
    }

    // ── Stat chips row ───────────────────────────────────────────────────────
    let stats_row = GtkBox::new(Orientation::Horizontal, 6);
    stats_row.set_margin_bottom(14);

    let uptime = dashboard_stat_chip();
    let battery = dashboard_stat_chip();
    battery.set_visible(false);
    let processes = dashboard_stat_chip();
    let workspace = dashboard_stat_chip();
    workspace.set_visible(false);
    stats_row.append(&uptime);
    stats_row.append(&battery);
    stats_row.append(&processes);
    stats_row.append(&workspace);
    root.append(&stats_row);

    // ── Metric 2×2 grid ──────────────────────────────────────────────────────
    let metric_grid = Grid::new();
    metric_grid.set_column_spacing(7);
    metric_grid.set_row_spacing(7);
    metric_grid.set_column_homogeneous(true);
    metric_grid.set_hexpand(true);
    metric_grid.set_margin_bottom(8);

    let (load_card, load, load_sub, load_bar) =
        crate::ui::metric_card("CPU", "utilities-system-monitor-symbolic");
    let (memory_card, memory, memory_sub, memory_bar) =
        crate::ui::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, disk_sub, disk_bar) =
        crate::ui::metric_card("Disk", "drive-harddisk-symbolic");
    // Fixed per-metric bar colors (match dashboard mockup)
    load_bar.add_css_class("metric-bar-cpu");
    memory_bar.add_css_class("metric-bar-mem");
    disk_bar.add_css_class("metric-bar-disk");

    let thermal_card = GtkBox::new(Orientation::Vertical, 6);
    thermal_card.add_css_class("metric-card");
    thermal_card.set_hexpand(true);
    // No icon in the header — matches the CPU/Memory/Disk metric cards.
    let thermal_title = Label::new(Some("Temp"));
    thermal_title.add_css_class("metric-label");
    thermal_title.set_hexpand(true);
    thermal_title.set_xalign(0.0);
    thermal_card.append(&thermal_title);

    let thermal_value_row = GtkBox::new(Orientation::Horizontal, 3);
    thermal_value_row.set_valign(gtk::Align::Baseline);
    let thermal = Label::new(Some("—"));
    thermal.add_css_class("metric-value");
    thermal.set_xalign(0.0);
    thermal_value_row.append(&thermal);
    let thermal_unit = Label::new(Some("°C"));
    thermal_unit.add_css_class("metric-unit");
    thermal_unit.set_valign(gtk::Align::End);
    thermal_unit.set_margin_bottom(2);
    thermal_value_row.append(&thermal_unit);
    thermal_card.append(&thermal_value_row);

    let thermal_bar = ProgressBar::new();
    thermal_bar.add_css_class("dashboard-metric-bar");
    thermal_bar.add_css_class("metric-bar-temp");
    thermal_card.append(&thermal_bar);

    // Sparklines are kept for data continuity but not shown on dashboard cards
    // (the mockup shows a single thin progress bar per metric).
    let load_graph = metric_graph();
    let memory_graph = metric_graph();
    let disk_graph = metric_graph();

    metric_grid.attach(&load_card, 0, 0, 1, 1);
    metric_grid.attach(&memory_card, 1, 0, 1, 1);
    metric_grid.attach(&disk_card, 0, 1, 1, 1);
    metric_grid.attach(&thermal_card, 1, 1, 1, 1);
    root.append(&metric_grid);

    // ── 3-column control cards ───────────────────────────────────────────────
    let control_row = GtkBox::new(Orientation::Horizontal, 7);
    control_row.set_hexpand(true);

    let (network_card, network, network_row) =
        crate::ui::control_card("Network", "network-wireless-symbolic");
    let (audio_card, audio, audio_row) = crate::ui::control_card("Audio", "audio-volume-high-symbolic");
    let (media_card, media, media_row) =
        crate::ui::control_card("Media", "media-playback-start-symbolic");
    // Keep notifications_card for struct compat (hidden)
    let (notifications_card, notifications, notify_row) =
        crate::ui::control_card("Notifications", "preferences-system-notifications-symbolic");
    notifications_card.set_visible(false);

    // Each card shows only a muted sub-line under the value (mockup style).
    // The action buttons are kept (hidden) for keyboard/IPC use and are
    // triggered by clicking anywhere on the card.
    let network_sub = Label::new(None);
    network_sub.add_css_class("result-subtitle");
    network_sub.set_xalign(0.0);
    network_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    network_row.append(&network_sub);

    let open_network = dashboard_button("Open");
    let toggle_wifi = dashboard_button("Wi-Fi");
    open_network.set_visible(false);
    toggle_wifi.set_visible(false);
    network_row.append(&open_network);
    network_row.append(&toggle_wifi);

    let audio_sub = Label::new(None);
    audio_sub.add_css_class("result-subtitle");
    audio_sub.set_xalign(0.0);
    audio_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    audio_row.append(&audio_sub);

    let open_audio = dashboard_button("Mixer");
    let toggle_mute = dashboard_button("Mute");
    open_audio.set_visible(false);
    toggle_mute.set_visible(false);
    audio_row.append(&open_audio);
    audio_row.append(&toggle_mute);

    let media_sub = Label::new(None);
    media_sub.add_css_class("result-subtitle");
    media_sub.set_xalign(0.0);
    media_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    media_row.append(&media_sub);

    let open_media = dashboard_button("Open");
    open_media.set_visible(false);
    media_row.append(&open_media);

    let open_notifications = dashboard_button("Notify");
    let toggle_dnd = dashboard_button("DND");
    notify_row.append(&open_notifications);
    notify_row.append(&toggle_dnd);

    // Clicking a control card triggers its (hidden) open button.
    for (card, btn) in [
        (&network_card, &open_network),
        (&audio_card, &open_audio),
        (&media_card, &open_media),
    ] {
        let gesture = gtk::GestureClick::new();
        let btn = btn.clone();
        gesture.connect_released(move |_, _, _, _| {
            btn.activate();
        });
        card.add_controller(gesture);
    }

    control_row.append(&network_card);
    control_row.append(&audio_card);
    control_row.append(&media_card);
    root.append(&control_row);

    // Quick action buttons — kept for IPC/keyboard bindings but not shown in UI
    let open_ai = dashboard_button("AI Chat");
    let open_system = dashboard_button("System Monitor");
    let lock = dashboard_button("Lock");
    let suspend = dashboard_button("Suspend");
    let toggle_bluetooth = dashboard_button("Bluetooth");
    open_ai.set_visible(false);
    open_system.set_visible(false);
    lock.set_visible(false);
    suspend.set_visible(false);
    toggle_bluetooth.set_visible(false);

    // Use scroll as the actual root widget — but the struct expects a GtkBox.
    // Wrap scroll in an outer box.
    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_vexpand(true);
    outer.append(&scroll);

    let view = DashboardView {
        root: outer,
        clock,
        date,
        uptime,
        battery,
        processes,
        workspace,
        load,
        load_sub,
        memory,
        memory_sub,
        disk,
        disk_sub,
        thermal,
        network,
        network_sub,
        audio,
        audio_sub,
        media,
        media_sub,
        notifications,
        load_bar,
        memory_bar,
        disk_bar,
        thermal_bar,
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
    set_dashboard_network_snapshot(&view, &NetworkSnapshot::default());
    set_dashboard_battery_snapshot(&view, &crate::battery_snapshot());
    set_dashboard_audio_snapshot(&view, &AudioSnapshot::default());
    set_dashboard_media_snapshot(&view, &MediaSnapshot::default());
    set_dashboard_notification_snapshot(&view, &NotificationSnapshot::default());
    view
}


pub fn set_dashboard_snapshot(view: &DashboardView, snapshot: &SystemSnapshot) {
    let now = Local::now();
    // Clock is updated by a per-second blinking timer; just set date here if not ticking yet
    if view.clock.text().is_empty() {
        view.clock.set_markup(&format!(
            "{}<span alpha='65%'>:</span>{}",
            now.format("%H"),
            now.format("%M")
        ));
        view.date.set_text(&now.format("%A, %B %-d").to_string());
    }
    // Update workspace chip
    let ws = crate::workspace_snapshot();
    // The pill already says "Workspace"; show just the index/name (mockup: "2").
    let ws_short = ws
        .active_name
        .clone()
        .unwrap_or_else(|| ws.active_idx.to_string());
    view.workspace.set_markup(&format!(
        "<span alpha='40%'>Workspace</span>  {}",
        glib::markup_escape_text(&ws_short)
    ));
    view.workspace.set_visible(true);
    let uptime_val = snapshot
        .uptime_seconds
        .map(format_duration)
        .unwrap_or_else(|| "—".to_string());
    view.uptime.set_markup(&format!(
        "<span alpha='40%'>Uptime</span>  {}",
        glib::markup_escape_text(&uptime_val)
    ));
    // CPU: show utilisation percentage + "%" unit (matches dashboard mockup)
    let load_fraction = snapshot.load_average.map(load_fraction).unwrap_or_default();
    let load_val = snapshot
        .load_average
        .map(|_| format!("{}", (load_fraction * 100.0).round() as u32))
        .unwrap_or_else(|| "—".to_string());
    view.load.set_text(&load_val);
    view.load_sub.set_text("%");
    view.load_bar.set_fraction(load_fraction);
    push_metric_graph(&view.load_graph, load_fraction);

    // Memory: show GB value + "GB" unit
    let memory_fraction = snapshot
        .memory_used_percent()
        .map(|p| (p / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    let mem_gb = snapshot
        .memory_used_kib()
        .map(|k| format!("{:.1}", k as f64 / 1024.0 / 1024.0))
        .unwrap_or_else(|| "—".to_string());
    view.memory.set_text(&mem_gb);
    view.memory_sub.set_text("GB");
    view.memory_bar.set_fraction(memory_fraction);
    push_metric_graph(&view.memory_graph, memory_fraction);

    // Disk: show percentage + "%" unit
    let disk_fraction = snapshot
        .disk_used_percent()
        .map(|p| (p / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    let disk_pct = snapshot
        .disk_used_percent()
        .map(|p| format!("{p:.0}"))
        .unwrap_or_else(|| "—".to_string());
    view.disk.set_text(&disk_pct);
    view.disk_sub.set_text("%");
    view.disk_bar.set_fraction(disk_fraction);
    push_metric_graph(&view.disk_graph, disk_fraction);

    let proc_val = snapshot
        .process_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "—".to_string());
    view.processes.set_markup(&format!(
        "<span alpha='40%'>Procs</span>  {}",
        glib::markup_escape_text(&proc_val)
    ));
}

pub fn set_dashboard_thermal(view: &DashboardView, celsius: Option<f32>) {
    if let Some(t) = celsius {
        view.thermal.set_text(&format!("{t:.0}"));
        // Map 0–100 °C onto the bar; most CPUs idle 30–60, throttle ~90.
        view.thermal_bar
            .set_fraction((t as f64 / 100.0).clamp(0.0, 1.0));
    } else {
        view.thermal.set_text("—");
        view.thermal_bar.set_fraction(0.0);
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
        view.network.set_text("Disconnected");
        view.network_sub.set_text("");
        return;
    };

    let status = if interface.state == "up" {
        "Connected"
    } else {
        &interface.state
    };
    view.network.set_text(status);

    let address = interface
        .ipv4_addresses
        .first()
        .or_else(|| interface.ipv6_addresses.first())
        .map(String::as_str)
        .unwrap_or("");
    let sub = if address.is_empty() {
        interface.name.clone()
    } else {
        format!("{}  ·  {}", interface.name, address)
    };
    view.network_sub.set_text(&sub);
}

pub fn set_dashboard_battery_snapshot(view: &DashboardView, snapshot: &BatterySnapshot) {
    let Some(battery) = snapshot.primary() else {
        view.battery.set_visible(false);
        return;
    };
    let capacity = battery
        .capacity_percent
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "—".to_string());
    view.battery.set_markup(&format!(
        "<span alpha='40%'>Battery</span>  {}",
        glib::markup_escape_text(&capacity)
    ));
    view.battery.set_visible(true);
}

pub fn set_dashboard_audio_snapshot(view: &DashboardView, snapshot: &AudioSnapshot) {
    if let Some(output) = &snapshot.output {
        let status = if output.muted {
            "Muted".to_string()
        } else {
            format!("{}%", output.volume_percent)
        };
        view.audio.set_text(&status);
        let name = output.name.as_deref().unwrap_or("Built-in Output");
        // Char-safe truncation (byte slicing panics on multi-byte UTF-8).
        let short_name = if name.chars().count() > 22 {
            name.chars().take(20).collect::<String>()
        } else {
            name.to_string()
        };
        let mic_info = snapshot
            .input
            .as_ref()
            .map(|i| format!("  ·  mic {}%", i.volume_percent))
            .unwrap_or_default();
        view.audio_sub.set_text(&format!("{short_name}{mic_info}"));
    } else {
        view.audio.set_text("No device");
        view.audio_sub.set_text("");
    }
}


pub fn set_dashboard_media_snapshot(view: &DashboardView, snapshot: &MediaSnapshot) {
    if !snapshot.is_active() {
        view.media.set_text("No player");
        view.media_sub.set_text("");
        return;
    }

    // Value = track title; sub = "artist · player" (matches mockup).
    let title = snapshot.title.as_deref().unwrap_or("Unknown track");
    view.media.set_text(title);
    let player = snapshot.player.as_deref().unwrap_or("MPRIS");
    let sub = match snapshot.artist.as_deref() {
        Some(artist) if !artist.is_empty() => format!("{artist}  ·  {player}"),
        _ => player.to_string(),
    };
    view.media_sub.set_text(&sub);
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


fn dashboard_stat_chip() -> Label {
    let label = Label::new(None);
    label.add_css_class("stat-chip");
    label.set_xalign(0.5);
    label.set_valign(gtk::Align::Center);
    label
}


fn dashboard_button(label: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("dashboard-button");
    button.add_css_class("widget-btn");
    button
}

