use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::Local;
use gtk::cairo;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Box, Button, DrawingArea, DropDown, Entry, Grid, Image, Label, ListBox,
    ListBoxRow, Orientation, Paned, ProgressBar, Stack, StringList,
};

use crate::{
    Action, AudioDeviceSnapshot, AudioSnapshot, AudioStreamSnapshot, BatterySnapshot,
    ClipboardSummary, CommandSummary, MediaSnapshot, NetworkSnapshot, NotificationSnapshot,
    ProcessSummary, SnippetSummary, SystemSnapshot, ThermalSnapshot,
};

#[derive(Debug, Clone)]
pub struct ActionPanelDisplayItem {
    pub title: String,
    pub icon_name: String,
    pub is_section_header: bool,
    pub is_destructive: bool,
}

#[derive(Clone)]
pub struct ActionPanelView {
    pub root: GtkBox,
    pub title: Label,
    pub subtitle: Label,
    pub search: Entry,
    pub list: ListBox,
}

#[derive(Clone)]
pub struct AiChatView {
    pub root: GtkBox,
    pub input: Entry,
    pub output: Label,
    pub ask: Button,
    pub stop: Button,
    pub status: Label,
    pub copy: Button,
    pub use_clipboard: Button,
    pub save: Button,
}

#[derive(Clone)]
pub struct AudioView {
    pub root: GtkBox,
    pub output_name: Label,
    pub output_volume: Label,
    pub output_bar: ProgressBar,
    pub input_name: Label,
    pub input_volume: Label,
    pub input_bar: ProgressBar,
    pub streams_list: ListBox,
    pub mute_output: Button,
    pub mute_input: Button,
}

#[derive(Clone)]
pub struct MetricGraph {
    area: DrawingArea,
    values: Rc<RefCell<Vec<f64>>>,
}

#[derive(Clone)]
pub struct ScriptOutputView {
    pub root: GtkBox,
    pub title: gtk::Label,
    pub output: gtk::Label,
}

#[derive(Clone)]
pub struct ExtensionBrowserView {
    pub root: GtkBox,
    pub list: ListBox,
}

#[derive(Clone)]
pub struct ClipboardHistoryView {
    pub root: GtkBox,
    pub list: ListBox,
    pub filter: DropDown,
    pub detail_title: Label,
    pub detail_preview: Label,
    pub detail_kind: Label,
    pub detail_size: Label,
    pub detail_mime: Label,
}

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

#[derive(Clone)]
pub struct SystemMonitorView {
    pub root: GtkBox,
    pub uptime: Label,
    pub load: Label,
    pub temperature: Label,
    pub memory: Label,
    pub disk: Label,
    pub processes: Label,
    pub load_bar: ProgressBar,
    pub memory_bar: ProgressBar,
    pub disk_bar: ProgressBar,
    pub load_graph: MetricGraph,
    pub memory_graph: MetricGraph,
    pub disk_graph: MetricGraph,
    pub list: ListBox,
    pub kill: Button,
}

#[derive(Clone)]
pub struct NetworkView {
    pub root: GtkBox,
    pub list: ListBox,
    pub connect_wifi: Button,
    pub disconnect: Button,
    pub copy_ip: Button,
    pub copy_mac: Button,
}

#[derive(Clone)]
pub struct NotificationsView {
    pub root: GtkBox,
    pub backend: Label,
    pub count: Label,
    pub dnd: Label,
    pub message: Label,
    pub history: ListBox,
    pub toggle_dnd: Button,
    pub close_all: Button,
    pub open_panel: Button,
}

#[derive(Clone)]
pub struct MediaView {
    pub root: GtkBox,
    pub player: Label,
    pub status: Label,
    pub title: Label,
    pub previous: Button,
    pub play_pause: Button,
    pub next: Button,
}

#[derive(Clone)]
pub struct SnippetManagerView {
    pub root: GtkBox,
    pub list: ListBox,
}

#[derive(Clone)]
pub struct PreferencesView {
    pub root: GtkBox,
    pub search: Entry,
    pub fields: Vec<(String, Entry)>,
    pub save: Button,
    pub cancel: Button,
}

pub fn action_panel_view() -> ActionPanelView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let title = super::panel_title("");
    root.append(&title);

    let subtitle = Label::new(None);
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_xalign(0.0);
    root.append(&subtitle);

    let search = Entry::builder()
        .placeholder_text("Search actions")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    root.append(&search);

    let list = super::results_list();
    let scroller = super::scrollable_list(&list);
    root.append(&scroller);

    ActionPanelView {
        root,
        title,
        subtitle,
        search,
        list,
    }
}

pub fn ai_chat_view() -> AiChatView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let header_row = Box::new(Orientation::Horizontal, 8);
    let title = super::panel_title("AI Chat");
    title.set_hexpand(true);
    let model_chip = Label::new(Some("local model"));
    model_chip.add_css_class("ai-model-chip");
    header_row.append(&title);
    header_row.append(&model_chip);
    root.append(&header_row);

    let answer_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(false)
        .vexpand(true)
        .build();
    answer_scroll.add_css_class("results-scroll");
    let output = Label::new(Some(
        "Ask a quick question to a local Ollama-compatible model.",
    ));
    output.add_css_class("result-subtitle");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_margin_start(4);
    output.set_margin_end(4);
    output.set_margin_top(6);
    output.set_margin_bottom(6);
    output.set_selectable(true);
    answer_scroll.set_child(Some(&output));
    root.append(&answer_scroll);

    let composer = dashboard_plain_card("Composer", "document-edit-symbolic");

    let context_row = Box::new(Orientation::Horizontal, 6);
    let context_chip = Label::new(Some(""));
    context_chip.add_css_class("ai-context-chip");
    context_chip.set_visible(false);
    context_row.append(&context_chip);
    composer.append(&context_row);

    let input = Entry::builder()
        .placeholder_text("Ask local AI…")
        .hexpand(true)
        .build();
    input.add_css_class("search-entry");
    composer.append(&input);

    let status = Label::new(None);
    status.add_css_class("result-subtitle");
    status.set_xalign(0.0);
    status.set_visible(false);
    composer.append(&status);

    let buttons = dashboard_card_actions();
    buttons.set_halign(gtk::Align::End);
    let copy = dashboard_button("Copy");
    let use_clipboard = dashboard_button("Use Clipboard");
    let save = dashboard_button("Save Snippet");
    let ask = dashboard_button("Ask");
    ask.add_css_class("suggested-action");
    let stop = dashboard_button("Stop");
    stop.add_css_class("destructive-action");
    stop.set_visible(false);
    buttons.append(&copy);
    buttons.append(&use_clipboard);
    buttons.append(&save);
    buttons.append(&ask);
    buttons.append(&stop);
    composer.append(&buttons);
    root.append(&composer);

    AiChatView {
        root,
        input,
        output,
        ask,
        stop,
        status,
        copy,
        use_clipboard,
        save,
    }
}

pub fn set_action_panel_items(
    view: &ActionPanelView,
    action: &Action,
    items: &[ActionPanelDisplayItem],
) {
    view.title.set_text(&action.title);
    view.subtitle.set_text(&action.subtitle);

    set_action_panel_list(&view.list, items);
}

pub fn set_action_panel_list(list: &ListBox, items: &[ActionPanelDisplayItem]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut first_selectable: Option<gtk::ListBoxRow> = None;
    for item in items {
        if item.is_section_header {
            let row = action_section_header_row(&item.title);
            list.append(&row);
        } else {
            let row = super::secondary_action_row(&item.icon_name, &item.title);
            if item.is_destructive {
                row.add_css_class("danger");
            }
            if first_selectable.is_none() {
                first_selectable = Some(row.clone());
            }
            list.append(&row);
        }
    }

    if let Some(row) = first_selectable {
        list.select_row(Some(&row));
    }
}

fn action_section_header_row(title: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("action-section-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let label = Label::new(Some(title));
    label.add_css_class("action-section-label");
    label.set_xalign(0.0);
    label.set_margin_start(10);
    label.set_margin_end(10);
    row.set_child(Some(&label));
    row
}

pub fn script_output_view() -> ScriptOutputView {
    let root = super::panel_root(10, 12);
    root.set_vexpand(true);

    let title = super::panel_title("Script Output");
    root.append(&title);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.add_css_class("results-scroll");
    let output = Label::new(None);
    output.add_css_class("result-subtitle");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_selectable(true);
    output.set_margin_top(6);
    output.set_margin_start(4);
    scroll.set_child(Some(&output));
    root.append(&scroll);

    ScriptOutputView { root, title, output }
}

pub fn set_script_output(view: &ScriptOutputView, script_title: &str, stdout: &str) {
    view.title.set_text(&format!("Script: {script_title}"));
    view.output.set_text(stdout.trim());
}

pub fn extension_browser_view(commands: &[CommandSummary]) -> ExtensionBrowserView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Extensions");
    root.append(&header);

    let list = super::results_list();
    for command in commands {
        let row = extension_row(command);
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    let scroller = super::scrollable_list(&list);
    root.append(&scroller);
    ExtensionBrowserView { root, list }
}

pub fn audio_view(snapshot: &AudioSnapshot) -> AudioView {
    let root = super::panel_root(10, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Audio");
    root.append(&header);

    let device_grid = dashboard_grid();
    let output_card = dashboard_plain_card("Output Volume", "audio-volume-high-symbolic");
    let input_card = dashboard_plain_card("Input Volume", "audio-input-microphone-symbolic");

    let (output_name, output_volume, output_bar, mute_output) =
        audio_device_controls(&output_card, "audio-volume-muted-symbolic");
    let (input_name, input_volume, input_bar, mute_input) =
        audio_device_controls(&input_card, "microphone-sensitivity-muted-symbolic");

    device_grid.attach(&output_card, 0, 0, 1, 1);
    device_grid.attach(&input_card, 1, 0, 1, 1);
    root.append(&device_grid);

    let streams_card =
        dashboard_plain_card("Application Volumes", "multimedia-volume-control-symbolic");
    streams_card.set_vexpand(true);

    let streams_list = super::results_list();
    let streams_scroller = super::scrollable_list(&streams_list);
    streams_card.append(&streams_scroller);
    root.append(&streams_card);

    let view = AudioView {
        root,
        output_name,
        output_volume,
        output_bar,
        input_name,
        input_volume,
        input_bar,
        streams_list,
        mute_output,
        mute_input,
    };
    set_audio_snapshot(&view, snapshot);
    view
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
        super::metric_card("CPU", "utilities-system-monitor-symbolic");
    let (memory_card, memory, memory_sub, memory_bar) =
        super::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, disk_sub, disk_bar) =
        super::metric_card("Disk", "drive-harddisk-symbolic");

    let thermal_card = GtkBox::new(Orientation::Vertical, 4);
    thermal_card.add_css_class("dashboard-card");
    thermal_card.set_hexpand(true);
    let thermal_header = GtkBox::new(Orientation::Horizontal, 6);
    let thermal_icon = super::icons::fa_icon("weather-clear-symbolic", 14);
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
        super::control_card("Network", "network-wireless-symbolic");
    let (audio_card, audio, audio_row) =
        super::control_card("Audio", "audio-volume-high-symbolic");
    let (media_card, media, media_row) =
        super::control_card("Media", "media-playback-start-symbolic");
    let (notifications_card, notifications, notify_row) =
        super::control_card("Notifications", "preferences-system-notifications-symbolic");

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

pub fn system_monitor_view(
    snapshot: &SystemSnapshot,
    processes: &[ProcessSummary],
) -> SystemMonitorView {
    let root = super::panel_root(10, 12);
    root.set_vexpand(true);

    let header = super::panel_title("System Monitor");
    root.append(&header);

    let metric_grid = dashboard_grid();
    let (uptime_card, uptime, _) =
        super::control_card("Uptime", "appointment-soon-symbolic");
    let (load_card, load, _, load_bar) =
        super::metric_card("Load", "utilities-system-monitor-symbolic");
    let (memory_card, memory, _, memory_bar) =
        super::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, _, disk_bar) =
        super::metric_card("Disk", "drive-harddisk-symbolic");
    let (temperature_card, temperature, _) =
        super::control_card("Temperature", "weather-clear-symbolic");
    let (process_count_card, process_count, _) =
        super::control_card("Processes", "application-x-executable-symbolic");
    let load_graph = metric_graph();
    let memory_graph = metric_graph();
    let disk_graph = metric_graph();
    load_card.append(&load_graph.area);
    memory_card.append(&memory_graph.area);
    disk_card.append(&disk_graph.area);
    metric_grid.attach(&uptime_card, 0, 0, 1, 1);
    metric_grid.attach(&load_card, 1, 0, 1, 1);
    metric_grid.attach(&memory_card, 0, 1, 1, 1);
    metric_grid.attach(&disk_card, 1, 1, 1, 1);
    metric_grid.attach(&temperature_card, 0, 2, 1, 1);
    metric_grid.attach(&process_count_card, 1, 2, 1, 1);
    root.append(&metric_grid);

    let process_card = dashboard_plain_card("Top Processes", "view-list-symbolic");
    process_card.set_vexpand(true);

    let list = super::results_list();
    let scroller = super::scrollable_list(&list);
    process_card.append(&scroller);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let kill = Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Terminate selected process")
        .build();
    kill.add_css_class("dashboard-button");
    buttons.append(&kill);
    process_card.append(&buttons);
    root.append(&process_card);

    let view = SystemMonitorView {
        root,
        uptime,
        load,
        temperature,
        memory,
        disk,
        processes: process_count,
        load_bar,
        memory_bar,
        disk_bar,
        load_graph,
        memory_graph,
        disk_graph,
        list,
        kill,
    };
    set_system_monitor_snapshot(&view, snapshot, processes);
    view
}

pub fn network_view(snapshot: &NetworkSnapshot) -> NetworkView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Network");
    root.append(&header);

    let network_card = dashboard_plain_card(
        "Interfaces, Wi-Fi, DNS and VPN",
        "network-wireless-symbolic",
    );
    network_card.set_vexpand(true);
    let list = super::results_list();
    set_network_snapshot(&list, snapshot);

    let scroller = super::scrollable_list(&list);
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

pub fn notifications_view(snapshot: &NotificationSnapshot) -> NotificationsView {
    let root = super::panel_root(10, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Notifications");
    root.append(&header);

    let overview_grid = dashboard_grid();
    let (backend_card, backend, _) = super::control_card("Backend", "applications-system-symbolic");
    let (count_card, count, _) = super::control_card("History", "document-open-recent-symbolic");
    let (dnd_card, dnd, _) = super::control_card("DND", "notifications-disabled-symbolic");
    overview_grid.attach(&backend_card, 0, 0, 1, 1);
    overview_grid.attach(&count_card, 1, 0, 1, 1);
    overview_grid.attach(&dnd_card, 0, 1, 1, 1);

    let message = dashboard_subtitle_label();
    message.set_wrap(true);
    let controls_card =
        dashboard_plain_card("Controls", "preferences-system-notifications-symbolic");
    controls_card.append(&message);

    let buttons = dashboard_card_actions();
    let toggle_dnd = dashboard_button("DND");
    let close_all = dashboard_button("Close All");
    let open_panel = dashboard_button("Panel");
    buttons.append(&toggle_dnd);
    buttons.append(&close_all);
    buttons.append(&open_panel);
    controls_card.append(&buttons);
    overview_grid.attach(&controls_card, 1, 1, 1, 1);
    root.append(&overview_grid);

    let history_card = dashboard_plain_card("History", "view-list-symbolic");
    history_card.set_vexpand(true);
    let history = super::results_list();
    let history_scroller = super::scrollable_list(&history);
    history_card.append(&history_scroller);
    root.append(&history_card);

    let view = NotificationsView {
        root,
        backend,
        count,
        dnd,
        message,
        history,
        toggle_dnd,
        close_all,
        open_panel,
    };
    set_notification_snapshot(&view, snapshot);
    view
}

pub fn media_view(snapshot: &MediaSnapshot) -> MediaView {
    let root = super::panel_root(10, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Media");
    root.append(&header);

    let media_card = dashboard_plain_card("Now Playing", "media-playback-start-symbolic");
    let title = dashboard_value_label();
    title.add_css_class("media-title");
    let player = dashboard_subtitle_label();
    let status = dashboard_subtitle_label();
    media_card.append(&title);
    media_card.append(&player);
    media_card.append(&status);

    let buttons = dashboard_card_actions();
    buttons.set_halign(gtk::Align::End);
    let previous = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .tooltip_text("Previous")
        .build();
    previous.add_css_class("dashboard-button");
    let play_pause = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play or pause")
        .build();
    play_pause.add_css_class("dashboard-button");
    let next = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .tooltip_text("Next")
        .build();
    next.add_css_class("dashboard-button");
    buttons.append(&previous);
    buttons.append(&play_pause);
    buttons.append(&next);
    media_card.append(&buttons);
    root.append(&media_card);

    let view = MediaView {
        root,
        player,
        status,
        title,
        previous,
        play_pause,
        next,
    };
    set_media_snapshot(&view, snapshot);
    view
}

pub fn set_media_snapshot(view: &MediaView, snapshot: &MediaSnapshot) {
    if snapshot.is_active() {
        let title = match (&snapshot.artist, &snapshot.title) {
            (Some(artist), Some(title)) => format!("{artist} - {title}"),
            (_, Some(title)) => title.clone(),
            (Some(artist), _) => artist.clone(),
            _ => "Unknown track".to_string(),
        };
        view.title.set_text(&title);
        view.player.set_text(
            &snapshot
                .player
                .as_deref()
                .unwrap_or("Unknown player")
                .to_string(),
        );
        view.status
            .set_text(snapshot.status.as_deref().unwrap_or("Unknown status"));
    } else {
        view.title.set_text("No active player");
        view.player
            .set_text("Install playerctl for MPRIS media status");
        view.status.set_text("");
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
        list.append(&super::secondary_action_row(
            "network-server-symbolic",
            &title,
        ));
    }

    if !snapshot.wifi_networks.is_empty() {
        list.append(&super::secondary_action_row(
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
        list.append(&super::secondary_action_row(
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

pub fn set_notification_snapshot(view: &NotificationsView, snapshot: &NotificationSnapshot) {
    view.backend
        .set_text(snapshot.backend.as_deref().unwrap_or("not detected"));
    view.count.set_text(
        &snapshot
            .count
            .map(|count| count.to_string())
            .unwrap_or("unknown".to_string()),
    );
    view.dnd.set_text(
        &snapshot
            .dnd
            .map(|enabled| {
                if enabled {
                    "enabled".to_string()
                } else {
                    "disabled".to_string()
                }
            })
            .unwrap_or("unknown".to_string()),
    );

    if snapshot.is_available() {
        if snapshot.history.is_empty() {
            view.message
                .set_text("Notification backend detected. No readable history entries.");
        } else {
            view.message
                .set_text(&format!("{} history entries", snapshot.history.len()));
        }
    } else {
        view.message
            .set_text("No supported notification backend found. Install or enable swaync or dunst to expose notification state here.");
    }

    set_notification_history_rows(&view.history, snapshot);
}

fn set_notification_history_rows(list: &ListBox, snapshot: &NotificationSnapshot) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for entry in snapshot.history.iter().take(12) {
        list.append(&notification_history_row(entry));
    }
}

fn notification_history_row(
    entry: &crate::services::notifications::NotificationEntrySnapshot,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name("preferences-system-notifications-symbolic");
    icon.add_css_class("result-icon");
    icon.set_pixel_size(20);
    layout.append(&icon);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&entry.summary));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    let subtitle = [entry.app_name.as_deref(), entry.body.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("  ");
    let subtitle = Label::new(Some(&subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_xalign(0.0);
    text.append(&subtitle);

    layout.append(&text);
    row.set_child(Some(&layout));
    row
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

pub fn set_audio_snapshot(view: &AudioView, snapshot: &AudioSnapshot) {
    set_audio_device(
        &view.output_name,
        &view.output_volume,
        &view.output_bar,
        snapshot.output.as_ref(),
        "Output device unavailable",
    );
    set_audio_device(
        &view.input_name,
        &view.input_volume,
        &view.input_bar,
        snapshot.input.as_ref(),
        "Input device unavailable",
    );
    set_audio_stream_rows(&view.streams_list, &snapshot.streams);
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

pub fn set_system_monitor_snapshot(
    view: &SystemMonitorView,
    snapshot: &SystemSnapshot,
    processes: &[ProcessSummary],
) {
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
            .unwrap_or("unknown".to_string()),
    );
    let load_fraction = snapshot.load_average.map(load_fraction).unwrap_or_default();
    view.load_bar.set_fraction(load_fraction);
    push_metric_graph(&view.load_graph, load_fraction);
    set_system_monitor_thermal_snapshot(view, &crate::thermal_snapshot());
    view.memory.set_text(
        &snapshot
            .memory_used_percent()
            .map(|percent| {
                let used = snapshot.memory_used_kib().unwrap_or_default() / 1024;
                let total = snapshot.memory_total_kib.unwrap_or_default() / 1024;
                format!("{percent:.0}%  ({used} / {total} MiB)")
            })
            .unwrap_or("unknown".to_string()),
    );
    let memory_fraction = snapshot
        .memory_used_percent()
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    view.memory_bar.set_fraction(memory_fraction);
    push_metric_graph(&view.memory_graph, memory_fraction);
    view.disk.set_text(
        &snapshot
            .disk_used_percent()
            .map(|percent| {
                let used = snapshot.disk_used_kib.unwrap_or_default() / 1024;
                let total = snapshot.disk_total_kib.unwrap_or_default() / 1024;
                format!("{percent:.0}%  ({used} / {total} MiB)")
            })
            .unwrap_or("unknown".to_string()),
    );
    let disk_fraction = snapshot
        .disk_used_percent()
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    view.disk_bar.set_fraction(disk_fraction);
    push_metric_graph(&view.disk_graph, disk_fraction);
    view.processes.set_text(
        &snapshot
            .process_count
            .map(|count| count.to_string())
            .unwrap_or("unknown".to_string()),
    );
    set_process_rows(&view.list, processes);
}

pub fn set_system_monitor_thermal_snapshot(view: &SystemMonitorView, snapshot: &ThermalSnapshot) {
    let Some(zone) = snapshot.hottest_zone() else {
        view.temperature.set_text("unknown");
        return;
    };

    let suffix = if snapshot.zones.len() > 1 {
        format!("  ({} zones)", snapshot.zones.len())
    } else {
        String::new()
    };
    view.temperature.set_text(&format!(
        "{:.1} C  {}{}",
        zone.temperature_c, zone.name, suffix
    ));
}

pub fn snippet_manager_view(items: &[SnippetSummary]) -> SnippetManagerView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let header = super::panel_title("Snippets");
    root.append(&header);

    let list = super::results_list();
    set_snippet_items(&list, items);

    let scroller = super::scrollable_list(&list);
    root.append(&scroller);
    SnippetManagerView { root, list }
}

fn dashboard_stat_chip() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-stat-chip");
    label.set_xalign(0.5);
    label.set_valign(gtk::Align::Center);
    label
}

fn dashboard_value_label() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-clock");
    label.set_xalign(0.0);
    label
}

fn dashboard_subtitle_label() -> Label {
    let label = Label::new(None);
    label.add_css_class("result-subtitle");
    label.set_xalign(0.0);
    label
}

fn dashboard_grid() -> Grid {
    let grid = Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);
    grid
}

fn dashboard_plain_card(title: &str, icon_name: &str) -> GtkBox {
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


fn dashboard_card_value() -> Label {
    let label = Label::new(None);
    label.add_css_class("dashboard-card-value");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_wrap(false);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn dashboard_card_actions() -> GtkBox {
    let actions = GtkBox::new(Orientation::Horizontal, 6);
    actions.add_css_class("dashboard-card-actions");
    actions
}

fn dashboard_button(label: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("dashboard-button");
    button
}


fn audio_device_controls(
    card: &GtkBox,
    mute_icon_name: &str,
) -> (Label, Label, ProgressBar, Button) {
    let name = dashboard_card_value();
    card.append(&name);

    let controls = GtkBox::new(Orientation::Horizontal, 10);
    controls.set_hexpand(true);

    let bar = ProgressBar::new();
    bar.add_css_class("audio-volume-bar");
    bar.set_show_text(false);
    bar.set_hexpand(true);
    controls.append(&bar);

    let volume = Label::new(None);
    volume.add_css_class("audio-volume-value");
    controls.append(&volume);

    let mute = Button::builder()
        .icon_name(mute_icon_name)
        .tooltip_text("Toggle mute")
        .build();
    mute.add_css_class("dashboard-button");
    controls.append(&mute);

    card.append(&controls);
    (name, volume, bar, mute)
}

fn set_audio_device(
    name: &Label,
    volume: &Label,
    bar: &ProgressBar,
    device: Option<&AudioDeviceSnapshot>,
    empty: &str,
) {
    let Some(device) = device else {
        name.set_text(empty);
        volume.set_text("--");
        bar.set_fraction(0.0);
        return;
    };

    name.set_text(device.name.as_deref().unwrap_or("Default device"));
    let muted = if device.muted { " muted" } else { "" };
    volume.set_text(&format!("{}%{muted}", device.volume_percent));
    bar.set_fraction((device.volume_percent as f64 / 100.0).clamp(0.0, 1.0));
}

fn set_audio_stream_rows(list: &ListBox, streams: &[AudioStreamSnapshot]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if streams.is_empty() {
        list.append(&super::secondary_action_row(
            "dialog-information-symbolic",
            "No active application streams",
        ));
        return;
    }

    for stream in streams {
        list.append(&audio_stream_row(stream));
    }
}

fn audio_stream_row(stream: &AudioStreamSnapshot) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name("audio-volume-medium-symbolic");
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");
    layout.append(&icon);

    let text = GtkBox::new(Orientation::Vertical, 4);
    text.set_hexpand(true);

    let title = Label::new(Some(&stream.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);

    let subtitle = stream
        .id
        .map(|id| format!("stream {id}"))
        .unwrap_or("application stream".to_string());
    let subtitle = Label::new(Some(&subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    text.append(&subtitle);

    let bar = ProgressBar::new();
    bar.add_css_class("audio-volume-bar");
    bar.set_show_text(false);
    bar.set_fraction(
        stream
            .volume_percent
            .map(|value| value as f64 / 100.0)
            .unwrap_or_default()
            .clamp(0.0, 1.0),
    );
    text.append(&bar);

    let volume = stream
        .volume_percent
        .map(|value| format!("{value}%"))
        .unwrap_or("--".to_string());
    let volume = Label::new(Some(if stream.muted { "muted" } else { &volume }));
    volume.add_css_class("audio-volume-value");

    layout.append(&text);
    layout.append(&volume);
    row.set_child(Some(&layout));
    row
}

fn metric_graph() -> MetricGraph {
    let area = DrawingArea::new();
    area.add_css_class("metric-graph");
    area.set_content_height(52);
    area.set_size_request(-1, 52);
    area.set_hexpand(true);
    let values = Rc::new(RefCell::new(Vec::<f64>::new()));
    let draw_values = Rc::clone(&values);
    area.set_draw_func(move |_, cr, width, height| {
        let values = draw_values.borrow();
        let w = width as f64;
        let h = height as f64;
        if w <= 1.0 || h <= 1.0 || values.is_empty() {
            return;
        }

        let n = values.len();
        let step = w / (n.saturating_sub(1).max(1)) as f64;

        // Compute y positions
        let ys: Vec<f64> = values
            .iter()
            .map(|v| h - (v.clamp(0.0, 1.0) * (h - 2.0)) - 1.0)
            .collect();

        // ── Fill under the line ───────────────────────────────────────────────
        cr.move_to(0.0, h);
        for (i, &y) in ys.iter().enumerate() {
            cr.line_to(i as f64 * step, y);
        }
        cr.line_to((n - 1) as f64 * step, h);
        cr.close_path();

        // Gradient fill: accent color at top, transparent at bottom
        let gradient = cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
        gradient.add_color_stop_rgba(0.0, 0.54, 0.706, 0.973, 0.28);
        gradient.add_color_stop_rgba(1.0, 0.54, 0.706, 0.973, 0.02);
        cr.set_source(&gradient).ok();
        cr.fill().ok();

        // ── Line on top ───────────────────────────────────────────────────────
        cr.set_source_rgba(0.54, 0.706, 0.973, 0.88);
        cr.set_line_width(1.5);
        for (i, &y) in ys.iter().enumerate() {
            let x = i as f64 * step;
            if i == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        cr.stroke().ok();
    });

    MetricGraph { area, values }
}

fn push_metric_graph(graph: &MetricGraph, value: f64) {
    let mut values = graph.values.borrow_mut();
    // Start empty — graph fills from left as data arrives
    values.push(value.clamp(0.0, 1.0));
    if values.len() > 60 {
        values.remove(0);
    }
    graph.area.queue_draw();
}

fn load_fraction(load: f32) -> f64 {
    let cores = std::thread::available_parallelism()
        .map(|value| value.get() as f32)
        .unwrap_or(1.0)
        .max(1.0);
    (load / cores).clamp(0.0, 1.0) as f64
}

fn set_process_rows(list: &ListBox, processes: &[ProcessSummary]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if processes.is_empty() {
        list.append(&super::secondary_action_row(
            "dialog-information-symbolic",
            "No process data available",
        ));
        return;
    }

    let max_memory_kib = processes
        .iter()
        .filter_map(|process| process.memory_kib)
        .max()
        .unwrap_or(1);

    for process in processes {
        list.append(&process_row(process, max_memory_kib));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

fn process_row(process: &ProcessSummary, max_memory_kib: u64) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name("application-x-executable-symbolic");
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&process.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let memory = process
        .memory_kib
        .map(|value| format!("{} MiB RSS", value / 1024))
        .unwrap_or("unknown RSS".to_string());
    let subtitle = Label::new(Some(&format!("pid {}  {}", process.pid, memory)));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);

    text.append(&title);
    text.append(&subtitle);

    let bar = ProgressBar::new();
    bar.add_css_class("process-memory-bar");
    bar.set_show_text(false);
    bar.set_fraction(
        process
            .memory_kib
            .map(|value| value as f64 / max_memory_kib.max(1) as f64)
            .unwrap_or_default()
            .clamp(0.0, 1.0),
    );
    text.append(&bar);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

fn format_duration(seconds: u64) -> String {
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

pub fn set_snippet_items(list: &ListBox, items: &[SnippetSummary]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in items {
        list.append(&snippet_row(item));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

pub fn clipboard_history_view(items: &[ClipboardSummary]) -> ClipboardHistoryView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    let header_row = GtkBox::new(Orientation::Horizontal, 8);
    header_row.add_css_class("dashboard-header");
    header_row.set_hexpand(true);

    let header = super::panel_title("Clipboard History");
    header.set_hexpand(true);
    header_row.append(&header);

    let filters = StringList::new(&["All", "Text", "URL", "Command", "Code"]);
    let filter = DropDown::new(Some(filters), gtk::Expression::NONE);
    filter.set_selected(0);
    filter.set_width_request(190);
    filter.set_tooltip_text(Some("Filter clipboard entries by type"));
    header_row.append(&filter);
    root.append(&header_row);

    let split = GtkBox::new(Orientation::Horizontal, 8);
    split.set_vexpand(true);

    let clipboard_card = dashboard_plain_card("Recent Copies", "edit-paste-symbolic");
    clipboard_card.set_vexpand(true);
    clipboard_card.set_hexpand(true);

    let list = super::results_list();
    set_clipboard_history_items(&list, items);

    let scroller = super::scrollable_list(&list);
    clipboard_card.append(&scroller);

    let actions = dashboard_card_actions();
    let copy = dashboard_button("Enter Copy");
    let delete = dashboard_button("Delete Remove");
    let clear = dashboard_button("Ctrl+Delete Clear");
    actions.append(&copy);
    actions.append(&delete);
    actions.append(&clear);
    clipboard_card.append(&actions);

    let detail_card = dashboard_plain_card("Preview", "document-open-symbolic");
    detail_card.set_vexpand(true);
    detail_card.set_width_request(320);

    let detail_title = dashboard_card_value();
    detail_title.set_wrap(true);
    detail_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail_card.append(&detail_title);

    let detail_preview = Label::new(None);
    detail_preview.add_css_class("dashboard-card-value");
    detail_preview.set_xalign(0.0);
    detail_preview.set_yalign(0.0);
    detail_preview.set_wrap(true);
    detail_preview.set_selectable(true);
    detail_preview.set_vexpand(true);
    detail_preview.set_valign(gtk::Align::Start);
    detail_preview.set_max_width_chars(42);
    detail_card.append(&detail_preview);

    let metadata = GtkBox::new(Orientation::Vertical, 6);
    metadata.set_margin_top(6);
    let (kind_row, detail_kind) = clipboard_metadata_row("Type");
    let (size_row, detail_size) = clipboard_metadata_row("Size");
    let (mime_row, detail_mime) = clipboard_metadata_row("Mime");
    metadata.append(&kind_row);
    metadata.append(&size_row);
    metadata.append(&mime_row);
    detail_card.append(&metadata);

    split.append(&clipboard_card);
    split.append(&detail_card);
    root.append(&split);

    let view = ClipboardHistoryView {
        root,
        list,
        filter,
        detail_title,
        detail_preview,
        detail_kind,
        detail_size,
        detail_mime,
    };
    set_clipboard_detail(&view, items.first());
    view
}

fn snippet_row(item: &SnippetSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name("insert-text-symbolic");
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&item.name));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle = Label::new(Some(&item.preview));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

pub fn set_clipboard_history_items(list: &ListBox, items: &[ClipboardSummary]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in items {
        list.append(&clipboard_row(item));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

pub fn set_clipboard_detail(view: &ClipboardHistoryView, item: Option<&ClipboardSummary>) {
    let Some(item) = item else {
        view.detail_title.set_text("No clipboard item selected");
        view.detail_preview
            .set_text("Clipboard history is empty for this filter.");
        view.detail_kind.set_text("-");
        view.detail_size.set_text("-");
        view.detail_mime.set_text("-");
        return;
    };

    view.detail_title.set_text(&item.preview);
    view.detail_preview
        .set_text(&clipboard_detail_text(&item.value));
    view.detail_kind.set_text(item.kind.label());
    view.detail_size.set_text(&format_bytes(item.size_bytes));
    view.detail_mime.set_text(item.kind.mime_hint());
}

pub fn preferences_view(current: &HashMap<String, String>) -> PreferencesView {

    let outer = super::panel_root(0, 0);
    outer.set_vexpand(true);
    outer.set_hexpand(true);

    let header_box = GtkBox::new(Orientation::Horizontal, 0);
    header_box.set_margin_top(12);
    header_box.set_margin_start(14);
    header_box.set_margin_end(14);
    header_box.set_margin_bottom(8);
    let header = super::panel_title("Preferences");
    header_box.append(&header);
    outer.append(&header_box);

    let search = Entry::builder()
        .placeholder_text("Search preferences…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    let search_row = GtkBox::new(Orientation::Horizontal, 0);
    search_row.set_margin_start(14);
    search_row.set_margin_end(14);
    search_row.set_margin_bottom(6);
    search_row.append(&search);
    outer.append(&search_row);

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_vexpand(true);
    paned.set_hexpand(true);
    paned.set_position(160);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    let sidebar = super::results_list();
    sidebar.add_css_class("pref-sidebar");
    sidebar.set_vexpand(true);
    sidebar.set_activate_on_single_click(true);

    let content_stack = Stack::new();
    content_stack.set_vexpand(true);
    content_stack.set_hexpand(true);

    let mut fields = Vec::new();

    for section in super::preferences::PREFERENCE_SECTIONS {
        let sidebar_row = gtk::ListBoxRow::new();
        sidebar_row.add_css_class("pref-sidebar-row");
        let sidebar_label = Label::new(Some(section.name));
        sidebar_label.add_css_class("pref-sidebar-label");
        sidebar_label.set_xalign(0.0);
        sidebar_label.set_margin_start(12);
        sidebar_label.set_margin_end(12);
        sidebar_row.set_child(Some(&sidebar_label));
        sidebar.append(&sidebar_row);

        let fields_box = GtkBox::new(Orientation::Vertical, 6);
        fields_box.add_css_class("pref-content");
        fields_box.set_vexpand(true);

        for (key, description) in section.keys {
            let row = GtkBox::new(Orientation::Vertical, 2);
            row.add_css_class("pref-field-row");

            let label = Label::new(Some(description));
            label.add_css_class("pref-field-label");
            label.set_xalign(0.0);
            row.append(&label);

            let entry = Entry::new();
            entry.set_hexpand(true);
            if let Some(value) = current.get(*key) {
                entry.set_text(value);
            }
            entry.set_placeholder_text(Some(key));
            row.append(&entry);

            fields.push((key.to_string(), entry));
            fields_box.append(&row);
        }

        let content_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&fields_box)
            .build();
        content_scroller.set_vexpand(true);
        content_stack.add_named(&content_scroller, Some(section.name));
    }

    let sidebar_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&sidebar)
        .build();
    sidebar_scroller.add_css_class("pref-sidebar");
    sidebar_scroller.set_vexpand(true);

    let sidebar_clone = sidebar.clone();
    let stack_clone = content_stack.clone();
    sidebar.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if let Some(section) = super::preferences::PREFERENCE_SECTIONS.get(index) {
            stack_clone.set_visible_child_name(section.name);
        }
        let _ = &sidebar_clone;
    });

    {
        let sidebar2 = sidebar.clone();
        let stack2 = content_stack.clone();
        search.connect_changed(move |entry| {
            let query = entry.text().to_lowercase();
            let mut first_match: Option<usize> = None;
            for (i, section) in super::preferences::PREFERENCE_SECTIONS.iter().enumerate() {
                let visible = query.is_empty()
                    || section.name.to_lowercase().contains(&query)
                    || section.keys.iter().any(|(_, desc)| {
                        desc.to_lowercase().contains(&query)
                    });
                if let Some(row) = sidebar2.row_at_index(i as i32) {
                    row.set_visible(visible);
                }
                if visible && first_match.is_none() {
                    first_match = Some(i);
                }
            }
            if let Some(idx) = first_match {
                if let Some(row) = sidebar2.row_at_index(idx as i32) {
                    sidebar2.select_row(Some(&row));
                    if let Some(section) = super::preferences::PREFERENCE_SECTIONS.get(idx) {
                        stack2.set_visible_child_name(section.name);
                    }
                }
            }
        });
    }

    if let Some(row) = sidebar.row_at_index(0) {
        sidebar.select_row(Some(&row));
    }
    if let Some(first) = super::preferences::PREFERENCE_SECTIONS.first() {
        content_stack.set_visible_child_name(first.name);
    }

    paned.set_start_child(Some(&sidebar_scroller));
    paned.set_end_child(Some(&content_stack));
    outer.append(&paned);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    buttons.set_margin_end(14);
    buttons.set_margin_bottom(10);

    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&save);
    outer.append(&buttons);

    PreferencesView {
        root: outer,
        search,
        fields,
        save,
        cancel,
    }
}

fn clipboard_row(item: &ClipboardSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name(item.kind.icon_name());
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&item.preview));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle = Label::new(Some(&format!(
        "{} · {}",
        item.kind.label(),
        format_bytes(item.size_bytes)
    )));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);

    if let Some(ts) = item.timestamp {
        let ago = Label::new(Some(&crate::config::format_time_ago(ts)));
        ago.add_css_class("clipboard-ago");
        ago.set_xalign(1.0);
        ago.set_valign(gtk::Align::Center);
        layout.append(&ago);
    }

    row.set_child(Some(&layout));
    row
}

fn clipboard_metadata_row(label: &str) -> (GtkBox, Label) {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_hexpand(true);

    let name = Label::new(Some(label));
    name.add_css_class("result-subtitle");
    name.set_xalign(0.0);
    name.set_width_chars(6);
    row.append(&name);

    let value = Label::new(None);
    value.add_css_class("dashboard-card-value");
    value.set_xalign(1.0);
    value.set_hexpand(true);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&value);

    (row, value)
}

fn clipboard_detail_text(value: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 1400;
    if value.chars().count() <= MAX_DETAIL_CHARS {
        return value.to_string();
    }

    let mut detail = value.chars().take(MAX_DETAIL_CHARS).collect::<String>();
    detail.push_str("\n...");
    detail
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

fn extension_row(command: &CommandSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = gtk::Image::from_icon_name(&command.icon_name);
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = Label::new(Some(&command.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let subtitle_text = if !command.description.is_empty() {
        command.description.as_str()
    } else {
        command.keyword.as_deref().unwrap_or_default()
    };
    let subtitle = Label::new(Some(subtitle_text));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}
