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
    pub network_sub: Label,
    pub audio: Label,
    pub audio_sub: Label,
    pub media: Label,
    pub media_sub: Label,
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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Header: "ACTIONS" label + filter input
    let header = GtkBox::new(Orientation::Horizontal, 8);
    header.add_css_class("action-panel-header");
    header.set_valign(gtk::Align::Center);

    let actions_label = Label::new(Some("Actions"));
    actions_label.add_css_class("action-panel-label");
    actions_label.set_valign(gtk::Align::Center);

    // Item title (selected action name)
    let title = Label::new(None);
    title.add_css_class("result-subtitle");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_valign(gtk::Align::Center);

    let subtitle = Label::new(None);
    subtitle.set_visible(false);

    let search = Entry::builder()
        .placeholder_text("Filter…")
        .hexpand(false)
        .build();
    search.add_css_class("action-panel-filter");
    search.set_width_chars(12);
    search.set_valign(gtk::Align::Center);

    header.append(&actions_label);
    header.append(&title);
    header.append(&search);
    root.append(&header);

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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // ── Model selector bar ───────────────────────────────────────────────────
    let model_bar = GtkBox::new(Orientation::Horizontal, 6);
    model_bar.add_css_class("ai-model-bar");
    model_bar.set_margin_top(0);

    let model_label = Label::new(Some("Model"));
    model_label.add_css_class("action-panel-label");
    model_label.set_valign(gtk::Align::Center);
    model_bar.append(&model_label);

    for model in &["llama3.2:3b", "mistral:7b", "phi3:mini"] {
        let btn = Button::with_label(model);
        btn.add_css_class("ai-model-btn");
        if *model == "llama3.2:3b" {
            btn.add_css_class("active");
        }
        model_bar.append(&btn);
    }
    root.append(&model_bar);

    // ── Messages scroll area ─────────────────────────────────────────────────
    let answer_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    answer_scroll.add_css_class("results-scroll");

    let output = Label::new(Some(
        "Hi! Running on Ollama. Ask me anything.",
    ));
    output.add_css_class("ai-message-assistant");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_margin_start(14);
    output.set_margin_end(14);
    output.set_margin_top(10);
    output.set_margin_bottom(6);
    output.set_selectable(true);
    answer_scroll.set_child(Some(&output));
    root.append(&answer_scroll);

    let status = Label::new(None);
    status.add_css_class("result-subtitle");
    status.set_xalign(0.0);
    status.set_margin_start(14);
    status.set_visible(false);
    root.append(&status);

    // ── Input row ────────────────────────────────────────────────────────────
    let input_row = GtkBox::new(Orientation::Horizontal, 8);
    input_row.add_css_class("ai-input-row");
    input_row.set_valign(gtk::Align::Center);

    let input = Entry::builder()
        .placeholder_text("Ask anything…")
        .hexpand(true)
        .build();
    input.add_css_class("search-entry");
    input_row.append(&input);

    let ask = Button::with_label("↑");
    ask.add_css_class("ai-send-btn");
    ask.set_valign(gtk::Align::Center);
    input_row.append(&ask);

    let stop = Button::with_label("■");
    stop.add_css_class("dashboard-button");
    stop.set_visible(false);
    input_row.append(&stop);

    let copy = Button::with_label("Copy");
    copy.add_css_class("action-bar-more");

    let use_clipboard = Button::with_label("Use Clipboard");
    use_clipboard.add_css_class("action-bar-more");

    let save = Button::with_label("Save");
    save.add_css_class("action-bar-more");

    // Secondary actions row (copy / clipboard / save)
    let sec_row = GtkBox::new(Orientation::Horizontal, 4);
    sec_row.add_css_class("action-bar");
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    sec_row.append(&spacer);
    sec_row.append(&use_clipboard);
    sec_row.append(&copy);
    sec_row.append(&save);

    root.append(&input_row);
    root.append(&sec_row);

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

    let active_count = commands.iter().filter(|c| c.enabled).count();
    let header_text = format!("Built-in  ·  {} of {} active", active_count, commands.len());
    let header = super::panel_title(&header_text);
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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // ── Output section ───────────────────────────────────────────────────────
    root.append(&super::section_header("Output"));

    let output_devices = ListBox::new();
    output_devices.add_css_class("results-list");
    output_devices.set_activate_on_single_click(true);
    for name in &["Built-in Speakers", "HDMI Output", "Headphones"] {
        let row = audio_device_row(name, *name == "Built-in Speakers");
        output_devices.append(&row);
    }
    root.append(&output_devices);

    let output_name = Label::new(Some("Built-in Speakers"));
    output_name.add_css_class("result-subtitle");
    output_name.set_visible(false); // used for data binding

    // Volume row: mute btn + GtkScale + value
    let vol_row = GtkBox::new(Orientation::Horizontal, 10);
    vol_row.set_margin_start(14);
    vol_row.set_margin_end(14);
    vol_row.set_margin_top(4);
    vol_row.set_margin_bottom(8);

    let mute_output = Button::with_label("🔊");
    mute_output.add_css_class("action-bar-btn");
    mute_output.set_tooltip_text(Some("Toggle mute"));
    mute_output.set_valign(gtk::Align::Center);

    let output_bar_scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    output_bar_scale.set_value(65.0);
    output_bar_scale.set_draw_value(false);
    output_bar_scale.set_hexpand(true);
    output_bar_scale.add_css_class("audio-volume-bar");

    let output_volume = Label::new(Some("65%"));
    output_volume.add_css_class("audio-volume-value");
    output_volume.set_width_chars(5);
    output_volume.set_xalign(1.0);

    vol_row.append(&mute_output);
    vol_row.append(&output_bar_scale);
    vol_row.append(&output_volume);
    root.append(&vol_row);

    // ── Input section ────────────────────────────────────────────────────────
    root.append(&super::section_header("Input"));

    let input_devices = ListBox::new();
    input_devices.add_css_class("results-list");
    input_devices.set_activate_on_single_click(true);
    for name in &["Built-in Microphone", "USB Microphone"] {
        let row = audio_device_row(name, *name == "Built-in Microphone");
        input_devices.append(&row);
    }
    root.append(&input_devices);

    let input_name = Label::new(Some("Built-in Microphone"));
    input_name.add_css_class("result-subtitle");
    input_name.set_visible(false);

    let in_vol_row = GtkBox::new(Orientation::Horizontal, 10);
    in_vol_row.set_margin_start(14);
    in_vol_row.set_margin_end(14);
    in_vol_row.set_margin_top(4);

    let mute_input = Button::with_label("🎙");
    mute_input.add_css_class("action-bar-btn");
    mute_input.set_valign(gtk::Align::Center);

    let input_bar_scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    input_bar_scale.set_value(80.0);
    input_bar_scale.set_draw_value(false);
    input_bar_scale.set_hexpand(true);
    input_bar_scale.add_css_class("audio-volume-bar");

    let input_volume = Label::new(Some("80%"));
    input_volume.add_css_class("audio-volume-value");
    input_volume.set_width_chars(5);
    input_volume.set_xalign(1.0);

    in_vol_row.append(&mute_input);
    in_vol_row.append(&input_bar_scale);
    in_vol_row.append(&input_volume);
    root.append(&in_vol_row);

    // ── App streams ─────────────────────────────────────────────────────────
    root.append(&super::section_header("Applications"));
    let streams_list = super::results_list();
    streams_list.set_vexpand(true);
    let streams_scroller = super::scrollable_list(&streams_list);
    root.append(&streams_scroller);

    // Compat fields: output_bar / input_bar as ProgressBar for snapshot update
    let output_bar = ProgressBar::new();
    output_bar.set_visible(false);
    let input_bar = ProgressBar::new();
    input_bar.set_visible(false);

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

    // ── Stat chips row ───────────────────────────────────────────────────────
    let stats_row = GtkBox::new(Orientation::Horizontal, 6);
    stats_row.set_margin_bottom(14);

    let uptime = dashboard_stat_chip();
    let battery = dashboard_stat_chip();
    battery.set_visible(false);
    let processes = dashboard_stat_chip();
    stats_row.append(&uptime);
    stats_row.append(&battery);
    stats_row.append(&processes);
    root.append(&stats_row);

    // ── Metric 2×2 grid ──────────────────────────────────────────────────────
    let metric_grid = Grid::new();
    metric_grid.set_column_spacing(7);
    metric_grid.set_row_spacing(7);
    metric_grid.set_column_homogeneous(true);
    metric_grid.set_hexpand(true);
    metric_grid.set_margin_bottom(8);

    let (load_card, load, load_sub, load_bar) =
        super::metric_card("CPU", "utilities-system-monitor-symbolic");
    let (memory_card, memory, memory_sub, memory_bar) =
        super::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, disk_sub, disk_bar) =
        super::metric_card("Disk", "drive-harddisk-symbolic");

    let thermal_card = GtkBox::new(Orientation::Vertical, 4);
    thermal_card.add_css_class("metric-card");
    thermal_card.set_hexpand(true);
    let thermal_header = GtkBox::new(Orientation::Horizontal, 6);
    let thermal_icon = super::icons::fa_icon("weather-clear-symbolic", 14);
    let thermal_title = Label::new(Some("Temp"));
    thermal_title.add_css_class("metric-label");
    thermal_title.set_hexpand(true);
    thermal_title.set_xalign(0.0);
    thermal_header.append(&thermal_icon);
    thermal_header.append(&thermal_title);
    thermal_card.append(&thermal_header);
    let thermal = Label::new(Some("—"));
    thermal.add_css_class("metric-value");
    thermal.set_xalign(0.0);
    thermal_card.append(&thermal);

    let load_graph = metric_graph();
    let memory_graph = metric_graph();
    let disk_graph = metric_graph();
    load_card.append(&load_graph.area);
    memory_card.append(&memory_graph.area);
    disk_card.append(&disk_graph.area);

    metric_grid.attach(&load_card,    0, 0, 1, 1);
    metric_grid.attach(&memory_card,  1, 0, 1, 1);
    metric_grid.attach(&disk_card,    0, 1, 1, 1);
    metric_grid.attach(&thermal_card, 1, 1, 1, 1);
    root.append(&metric_grid);

    // ── 3-column control cards ───────────────────────────────────────────────
    let control_row = GtkBox::new(Orientation::Horizontal, 7);
    control_row.set_hexpand(true);

    let (network_card, network, network_row) =
        super::control_card("Network", "network-wireless-symbolic");
    let (audio_card, audio, audio_row) =
        super::control_card("Audio", "audio-volume-high-symbolic");
    let (media_card, media, media_row) =
        super::control_card("Media", "media-playback-start-symbolic");
    // Keep notifications_card for struct compat (hidden)
    let (notifications_card, notifications, notify_row) =
        super::control_card("Notifications", "preferences-system-notifications-symbolic");
    notifications_card.set_visible(false);

    // Sub-text labels inserted before action buttons
    let network_sub = Label::new(None);
    network_sub.add_css_class("result-subtitle");
    network_sub.set_xalign(0.0);
    network_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    network_row.append(&network_sub);

    let open_network = dashboard_button("Open");
    let toggle_wifi = dashboard_button("Wi-Fi");
    network_row.append(&open_network);
    network_row.append(&toggle_wifi);

    let audio_sub = Label::new(None);
    audio_sub.add_css_class("result-subtitle");
    audio_sub.set_xalign(0.0);
    audio_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    audio_row.append(&audio_sub);

    let open_audio = dashboard_button("Mixer");
    let toggle_mute = dashboard_button("Mute");
    audio_row.append(&open_audio);
    audio_row.append(&toggle_mute);

    let media_sub = Label::new(None);
    media_sub.add_css_class("result-subtitle");
    media_sub.set_xalign(0.0);
    media_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    media_row.append(&media_sub);

    let open_media = dashboard_button("Open");
    media_row.append(&open_media);

    let open_notifications = dashboard_button("Notify");
    let toggle_dnd = dashboard_button("DND");
    notify_row.append(&open_notifications);
    notify_row.append(&toggle_dnd);

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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // ── Resource overview panel ──────────────────────────────────────────────
    let overview = GtkBox::new(Orientation::Vertical, 6);
    overview.set_margin_top(10);
    overview.set_margin_bottom(6);
    overview.set_margin_start(14);
    overview.set_margin_end(14);

    // CPU row
    let cpu_row = GtkBox::new(Orientation::Horizontal, 10);
    let cpu_label = Label::new(Some("CPU"));
    cpu_label.add_css_class("metric-label");
    cpu_label.set_width_chars(4);
    cpu_label.set_xalign(0.0);
    let load = Label::new(Some("—"));
    load.add_css_class("metric-value");
    load.set_width_chars(6);
    load.set_xalign(0.0);
    // 8 mini core bars drawn via Cairo
    let core_values: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(vec![0.0; 8]));
    let core_area = DrawingArea::new();
    core_area.set_content_width(8 * 8); // 8 bars × 8px each
    core_area.set_content_height(20);
    core_area.set_valign(gtk::Align::Center);
    {
        let vals = Rc::clone(&core_values);
        core_area.set_draw_func(move |_, cr, w, h| {
            let data = vals.borrow();
            let bar_w = w as f64 / data.len() as f64;
            for (i, &v) in data.iter().enumerate() {
                let x = i as f64 * bar_w + 1.0;
                let bar_h = (v * h as f64).max(2.0);
                let y = h as f64 - bar_h;
                // bg
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.10);
                cr.rectangle(x, 0.0, bar_w - 2.0, h as f64);
                let _ = cr.fill();
                // fill (accent color ≈ #8B7CF8)
                let col = if v > 0.8 { (1.0, 0.42, 0.37, 1.0) }
                          else if v > 0.6 { (0.96, 0.65, 0.14, 1.0) }
                          else { (0.545, 0.486, 0.973, 1.0) };
                cr.set_source_rgba(col.0, col.1, col.2, col.3);
                cr.rectangle(x, y, bar_w - 2.0, bar_h);
                let _ = cr.fill();
            }
        });
    }

    let load_graph = metric_graph();
    load_graph.area.set_hexpand(true);
    cpu_row.append(&cpu_label);
    cpu_row.append(&load);
    cpu_row.append(&core_area);
    cpu_row.append(&load_graph.area);
    overview.append(&cpu_row);

    // RAM row
    let ram_row = GtkBox::new(Orientation::Horizontal, 10);
    let ram_label = Label::new(Some("RAM"));
    ram_label.add_css_class("metric-label");
    ram_label.set_width_chars(4);
    ram_label.set_xalign(0.0);
    let memory = Label::new(Some("—"));
    memory.add_css_class("metric-value");
    memory.set_width_chars(10);
    memory.set_xalign(0.0);
    let memory_bar = ProgressBar::new();
    memory_bar.add_css_class("dashboard-metric-bar");
    memory_bar.set_hexpand(true);
    let memory_graph = metric_graph();
    memory_graph.area.set_hexpand(false);
    memory_graph.area.set_width_request(60);
    ram_row.append(&ram_label);
    ram_row.append(&memory);
    ram_row.append(&memory_bar);
    ram_row.append(&memory_graph.area);
    overview.append(&ram_row);

    // Disk row
    let disk_row = GtkBox::new(Orientation::Horizontal, 10);
    let disk_label = Label::new(Some("DISK"));
    disk_label.add_css_class("metric-label");
    disk_label.set_width_chars(4);
    disk_label.set_xalign(0.0);
    let disk = Label::new(Some("—"));
    disk.add_css_class("metric-value");
    disk.set_width_chars(10);
    disk.set_xalign(0.0);
    let disk_bar = ProgressBar::new();
    disk_bar.add_css_class("dashboard-metric-bar");
    disk_bar.set_hexpand(true);
    let disk_graph = metric_graph();
    disk_graph.area.set_hexpand(false);
    disk_graph.area.set_width_request(60);
    disk_row.append(&disk_label);
    disk_row.append(&disk);
    disk_row.append(&disk_bar);
    disk_row.append(&disk_graph.area);
    overview.append(&disk_row);

    let sep_line = gtk::Separator::new(Orientation::Horizontal);
    sep_line.set_margin_top(4);
    overview.append(&sep_line);
    root.append(&overview);

    // Compat fields
    let load_bar = ProgressBar::new();
    load_bar.set_visible(false);
    let uptime = Label::new(None);
    uptime.set_visible(false);
    let temperature = Label::new(None);
    temperature.set_visible(false);
    let processes_label = Label::new(None);
    processes_label.set_visible(false);

    // ── Process table ────────────────────────────────────────────────────────
    // Table header: filter + sort buttons
    let table_header = GtkBox::new(Orientation::Horizontal, 8);
    table_header.set_margin_start(14);
    table_header.set_margin_end(14);
    table_header.set_margin_bottom(4);

    let filter_entry = gtk::Entry::builder()
        .placeholder_text("filter processes…")
        .hexpand(true)
        .build();
    filter_entry.add_css_class("search-entry");
    table_header.append(&filter_entry);

    let sort_cpu = Button::with_label("CPU ↓");
    sort_cpu.add_css_class("action-bar-more");
    let sort_mem = Button::with_label("MEM");
    sort_mem.add_css_class("action-bar-more");
    table_header.append(&sort_cpu);
    table_header.append(&sort_mem);
    root.append(&table_header);

    let list = super::results_list();
    list.set_vexpand(true);
    let scroller = super::scrollable_list(&list);
    root.append(&scroller);

    let kill = Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Terminate selected process")
        .build();
    kill.add_css_class("dashboard-button");
    kill.set_visible(false);

    let view = SystemMonitorView {
        root,
        uptime,
        load,
        temperature,
        memory,
        disk,
        processes: processes_label,
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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Section header
    let hdr = super::section_header("Wi-Fi");
    root.append(&hdr);

    let list = super::results_list();
    set_network_snapshot(&list, snapshot);

    let scroller = super::scrollable_list(&list);
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

pub fn notifications_view(snapshot: &NotificationSnapshot) -> NotificationsView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Top bar: DND toggle + Close All
    let top_bar = GtkBox::new(Orientation::Horizontal, 8);
    top_bar.add_css_class("action-bar");

    let backend = Label::new(None);
    backend.add_css_class("result-subtitle");
    backend.set_hexpand(true);
    backend.set_xalign(0.0);
    top_bar.append(&backend);

    let dnd = Label::new(None);
    dnd.add_css_class("status-chip");
    top_bar.append(&dnd);

    let toggle_dnd = Button::with_label("DND");
    toggle_dnd.add_css_class("action-bar-more");
    let close_all = Button::with_label("Clear All");
    close_all.add_css_class("action-bar-more");
    let open_panel = Button::with_label("Settings");
    open_panel.add_css_class("action-bar-more");
    top_bar.append(&toggle_dnd);
    top_bar.append(&close_all);
    top_bar.append(&open_panel);
    root.append(&top_bar);

    // Notification list
    let history = ListBox::new();
    history.add_css_class("results-list");
    history.set_vexpand(true);
    history.set_activate_on_single_click(false);

    let scroller = super::scrollable_list(&history);
    root.append(&scroller);

    // Empty state message (shown when list is empty)
    let message = Label::new(Some("All caught up ✓"));
    message.add_css_class("result-subtitle");
    message.set_xalign(0.5);
    message.set_valign(gtk::Align::Center);
    message.set_vexpand(true);
    message.set_margin_top(40);
    root.append(&message);

    // Compat labels
    let count = Label::new(None);
    count.set_visible(false);

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
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.set_margin_top(20);
    root.set_margin_bottom(18);
    root.set_margin_start(14);
    root.set_margin_end(14);

    // ── Album art + track info ───────────────────────────────────────────────
    let info_row = GtkBox::new(Orientation::Horizontal, 14);
    info_row.set_margin_bottom(18);

    let art = GtkBox::new(Orientation::Vertical, 0);
    art.set_width_request(80);
    art.set_height_request(80);
    art.add_css_class("control-card");
    art.set_valign(gtk::Align::End);
    let art_icon = Label::new(Some("♪"));
    art_icon.set_vexpand(true);
    art_icon.set_hexpand(true);
    art_icon.set_valign(gtk::Align::Center);
    art_icon.set_halign(gtk::Align::Center);
    art_icon.add_css_class("metric-value");
    art.append(&art_icon);
    info_row.append(&art);

    let track_info = GtkBox::new(Orientation::Vertical, 4);
    track_info.set_valign(gtk::Align::End);
    track_info.set_hexpand(true);

    let title = Label::new(Some("No active player"));
    title.add_css_class("media-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let player = Label::new(None);
    player.add_css_class("result-subtitle");
    player.set_xalign(0.0);
    player.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let status = Label::new(None);
    status.add_css_class("result-subtitle");
    status.set_xalign(0.0);

    track_info.append(&title);
    track_info.append(&player);
    track_info.append(&status);
    info_row.append(&track_info);
    root.append(&info_row);

    // ── Scrubber (GtkScale) ──────────────────────────────────────────────────
    let scrubber = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    scrubber.set_draw_value(false);
    scrubber.set_hexpand(true);
    scrubber.set_margin_bottom(4);
    root.append(&scrubber);

    // Time labels
    let time_row = GtkBox::new(Orientation::Horizontal, 0);
    let time_pos = Label::new(Some("0:00"));
    time_pos.add_css_class("clipboard-time");
    time_pos.set_xalign(0.0);
    let time_spacer = GtkBox::new(Orientation::Horizontal, 0);
    time_spacer.set_hexpand(true);
    let time_total = Label::new(Some("0:00"));
    time_total.add_css_class("clipboard-time");
    time_total.set_xalign(1.0);
    time_row.append(&time_pos);
    time_row.append(&time_spacer);
    time_row.append(&time_total);
    root.append(&time_row);

    // ── Playback controls ────────────────────────────────────────────────────
    let controls = GtkBox::new(Orientation::Horizontal, 10);
    controls.set_halign(gtk::Align::Center);
    controls.set_margin_top(14);

    let previous = media_ctrl_btn("⏮", "Previous");
    let seek_back = media_ctrl_btn("⏪", "Seek back 10s");
    let play_pause = media_play_btn("media-playback-start-symbolic");
    let seek_fwd = media_ctrl_btn("⏩", "Seek forward 10s");
    let next = media_ctrl_btn("⏭", "Next");

    controls.append(&previous);
    controls.append(&seek_back);
    controls.append(&play_pause);
    controls.append(&seek_fwd);
    controls.append(&next);
    root.append(&controls);

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

fn media_ctrl_btn(label: &str, tooltip: &str) -> Button {
    let btn = Button::with_label(label);
    btn.add_css_class("action-bar-btn");
    btn.set_tooltip_text(Some(tooltip));
    btn.set_width_request(32);
    btn.set_height_request(32);
    btn
}

fn media_play_btn(icon: &str) -> Button {
    let btn = Button::builder().icon_name(icon).build();
    btn.add_css_class("ai-send-btn");
    btn.add_css_class("ready");
    btn.set_width_request(42);
    btn.set_height_request(42);
    btn
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

    // Connected interfaces first
    for interface in &snapshot.interfaces {
        if interface.name == "lo" { continue; }
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");
        if interface.state == "up" {
            row.add_css_class("selected");
        }

        let layout = GtkBox::new(Orientation::Horizontal, 10);
        layout.set_margin_top(0);
        layout.set_margin_bottom(0);
        layout.set_margin_start(14);
        layout.set_margin_end(14);

        // Signal bars or wired icon
        let sig_widget = if interface.is_wireless {
            signal_bars(75) // default decent signal for connected
        } else {
            let b = GtkBox::new(Orientation::Horizontal, 0);
            let ico = gtk::Image::from_icon_name("network-wired-symbolic");
            ico.set_pixel_size(16);
            b.append(&ico);
            b
        };
        layout.append(&sig_widget);

        let text = GtkBox::new(Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);

        let title = Label::new(Some(&interface.name));
        title.add_css_class("result-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);

        let addresses = interface
            .ipv4_addresses.iter().chain(interface.ipv6_addresses.iter())
            .take(1).cloned().collect::<Vec<_>>();
        let detail = addresses.first().map(String::as_str)
            .or(interface.mac_address.as_deref())
            .unwrap_or("");
        let sub_text = if interface.state == "up" {
            format!("Connected  {detail}")
        } else {
            format!("{}  {detail}", interface.state)
        };
        let subtitle = Label::new(Some(&sub_text));
        subtitle.add_css_class("result-subtitle");
        subtitle.set_xalign(0.0);
        subtitle.set_hexpand(true);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);

        text.append(&title);
        text.append(&subtitle);
        layout.append(&text);

        let btn_label = if interface.state == "up" { "Disconnect" } else { "Connect" };
        let btn = Button::with_label(btn_label);
        btn.add_css_class("action-bar-more");
        btn.set_valign(gtk::Align::Center);
        layout.append(&btn);

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
        list.append(&super::section_header("Available Wi-Fi"));
    }

    for network in &snapshot.wifi_networks {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("result-row");

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

        let security = network.security.as_deref().unwrap_or("Open");
        let sub = format!("{}  {}%", security, sig);
        let subtitle = Label::new(Some(&sub));
        subtitle.add_css_class("result-subtitle");
        subtitle.set_xalign(0.0);

        text.append(&title);
        text.append(&subtitle);
        layout.append(&text);

        let btn = Button::with_label("Connect");
        btn.add_css_class("action-bar-more");
        btn.set_valign(gtk::Align::Center);
        layout.append(&btn);

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
    let backend_text = snapshot.backend.as_deref().unwrap_or("No backend");
    view.backend.set_text(backend_text);

    let dnd_on = snapshot.dnd.unwrap_or(false);
    view.dnd.set_text(if dnd_on { "DND On" } else { "" });
    view.dnd.set_visible(dnd_on);
    if dnd_on {
        view.dnd.add_css_class("active");
    } else {
        view.dnd.remove_css_class("active");
    }

    set_notification_history_rows(&view.history, snapshot);

    let has_notifs = !snapshot.history.is_empty();
    view.message.set_visible(!has_notifs);
    if !has_notifs {
        if snapshot.is_available() {
            view.message.set_text("All caught up ✓");
        } else {
            view.message.set_text("No notification backend detected");
        }
    }
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
    layout.set_margin_top(10);
    layout.set_margin_bottom(10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);

    // App icon 32×32
    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_width_request(32);
    icon_box.set_height_request(32);
    icon_box.add_css_class("control-card");
    icon_box.set_valign(gtk::Align::Start);
    let first_char = entry.app_name.as_deref()
        .and_then(|n| n.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "◎".to_string());
    let icon_lbl = Label::new(Some(first_char.as_str()));
    icon_lbl.set_valign(gtk::Align::Center);
    icon_lbl.set_halign(gtk::Align::Center);
    icon_lbl.set_vexpand(true);
    icon_box.append(&icon_lbl);
    layout.append(&icon_box);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    // App name + timestamp row
    let meta_row = GtkBox::new(Orientation::Horizontal, 6);
    let app_name = Label::new(entry.app_name.as_deref().or(Some("App")));
    app_name.add_css_class("clipboard-time");
    app_name.set_xalign(0.0);
    meta_row.append(&app_name);
    text.append(&meta_row);

    // Summary (title)
    let title = Label::new(Some(&entry.summary));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    // Body
    if let Some(body) = &entry.body {
        if !body.is_empty() {
            let body_lbl = Label::new(Some(body));
            body_lbl.add_css_class("result-subtitle");
            body_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            body_lbl.set_xalign(0.0);
            text.append(&body_lbl);
        }
    }

    layout.append(&text);

    // Dismiss × button
    let dismiss = Button::with_label("×");
    dismiss.add_css_class("action-bar-btn");
    dismiss.set_valign(gtk::Align::Start);
    layout.append(&dismiss);

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
    // CPU: show load avg as value, cores as unit
    let load_val = snapshot.load_average.map(|l| format!("{l:.1}")).unwrap_or_else(|| "—".to_string());
    view.load.set_text(&load_val);
    let cores = snapshot.cpu_count.map(|n| format!("{n}c")).unwrap_or_default();
    view.load_sub.set_text(&cores);
    let load_fraction = snapshot.load_average.map(load_fraction).unwrap_or_default();
    view.load_bar.set_fraction(load_fraction);
    push_metric_graph(&view.load_graph, load_fraction);

    // Memory: show GB value + "GB" unit
    let memory_fraction = snapshot
        .memory_used_percent()
        .map(|p| (p / 100.0).clamp(0.0, 1.0) as f64)
        .unwrap_or_default();
    let mem_gb = snapshot.memory_used_kib()
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
    let disk_pct = snapshot.disk_used_percent()
        .map(|p| format!("{p:.0}"))
        .unwrap_or_else(|| "—".to_string());
    view.disk.set_text(&disk_pct);
    view.disk_sub.set_text("%");
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
        view.thermal.set_text(&format!("{t:.0}°C"));
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
        view.network.set_text("Disconnected");
        view.network_sub.set_text("");
        return;
    };

    let status = if interface.state == "up" { "Connected" } else { &interface.state };
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
        .unwrap_or_default();
    let status = battery.status.as_deref().unwrap_or("");
    view.battery.set_text(&format!("⚡ {capacity} {status}").trim().to_string());
    view.battery.set_visible(true);
}

pub fn set_dashboard_audio_snapshot(view: &DashboardView, snapshot: &AudioSnapshot) {
    if let Some(output) = &snapshot.output {
        let status = if output.muted { "Muted".to_string() } else { format!("{}%", output.volume_percent) };
        view.audio.set_text(&status);
        let name = output.name.as_deref().unwrap_or("Built-in Output");
        let short_name = if name.len() > 22 { &name[..20] } else { name };
        let mic_info = snapshot.input.as_ref()
            .map(|i| format!("  ·  mic {}%", i.volume_percent))
            .unwrap_or_default();
        view.audio_sub.set_text(&format!("{short_name}{mic_info}"));
    } else {
        view.audio.set_text("No device");
        view.audio_sub.set_text("");
    }
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
        view.media.set_text("No player");
        view.media_sub.set_text("");
        return;
    }

    let playing = snapshot.status.as_deref() == Some("Playing");
    view.media.set_text(if playing { "Playing" } else { "Paused" });

    let title = snapshot.title.as_deref().unwrap_or("Unknown");
    let short_title = if title.len() > 20 { &title[..18] } else { title };
    let player = snapshot.player.as_deref().unwrap_or("MPRIS");
    let artist_part = snapshot.artist.as_deref()
        .map(|a| format!("{a}  ·  "))
        .unwrap_or_default();
    view.media_sub.set_text(&format!("{artist_part}{short_title}  ·  {player}"));
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
    label.add_css_class("stat-chip");
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


fn audio_device_row(name: &str, active: bool) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // Radio dot
    let dot = GtkBox::new(Orientation::Vertical, 0);
    dot.set_width_request(7);
    dot.set_height_request(7);
    dot.set_valign(gtk::Align::Center);
    if active {
        dot.add_css_class("stat-chip");
    } else {
        dot.add_css_class("section-header-row");
    }
    layout.append(&dot);

    let label = Label::new(Some(name));
    label.add_css_class(if active { "result-title" } else { "result-subtitle" });
    label.set_xalign(0.0);
    label.set_hexpand(true);
    layout.append(&label);

    if active {
        let default_lbl = Label::new(Some("default"));
        default_lbl.add_css_class("clipboard-time");
        layout.append(&default_lbl);
    }

    row.set_child(Some(&layout));
    row
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

    let layout = GtkBox::new(Orientation::Horizontal, 7);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // Process name (monospace, flex-1)
    let title = Label::new(Some(&process.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    layout.append(&title);

    // Mini CPU progress bar (36×3px)
    // Mini memory bar (36×3px, relative to max)
    let mem_frac = process.memory_kib
        .map(|v| v as f64 / max_memory_kib.max(1) as f64)
        .unwrap_or(0.0);
    let cpu_bar = ProgressBar::new();
    cpu_bar.add_css_class("process-memory-bar");
    cpu_bar.set_fraction(mem_frac.clamp(0.0, 1.0));
    cpu_bar.set_show_text(false);
    cpu_bar.set_width_request(36);
    layout.append(&cpu_bar);

    // MEM
    let mem_text = process.memory_kib
        .map(|v| if v >= 1024*1024 { format!("{:.1}G", v as f64 / 1024.0 / 1024.0) }
                 else { format!("{}M", v / 1024) })
        .unwrap_or_else(|| "—".to_string());
    let mem_lbl = Label::new(Some(&mem_text));
    mem_lbl.add_css_class("clipboard-time");
    mem_lbl.set_width_chars(6);
    mem_lbl.set_xalign(1.0);
    layout.append(&mem_lbl);

    // Kill ×
    let kill_btn = Button::with_label("×");
    kill_btn.add_css_class("action-bar-btn");
    kill_btn.set_valign(gtk::Align::Center);
    kill_btn.set_tooltip_text(Some("Kill process"));
    layout.append(&kill_btn);

    row.set_child(Some(&layout));
    let _ = max_memory_kib;
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
    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.set_vexpand(true);

    // ── Left panel: list (216px fixed width) ─────────────────────────────────
    let left_panel = GtkBox::new(Orientation::Vertical, 0);
    left_panel.set_width_request(216);

    // filter bar at top
    let filters = StringList::new(&["All", "Text", "URL", "Command", "Code"]);
    let filter = DropDown::new(Some(filters), gtk::Expression::NONE);
    filter.set_selected(0);
    filter.set_margin_top(8);
    filter.set_margin_bottom(4);
    filter.set_margin_start(10);
    filter.set_margin_end(10);
    left_panel.append(&filter);

    // separator
    let sep = gtk::Separator::new(Orientation::Horizontal);
    left_panel.append(&sep);

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);
    set_clipboard_history_items(&list, items);

    let left_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();
    left_panel.append(&left_scroll);

    // ── Right panel: preview ──────────────────────────────────────────────────
    let right_panel = GtkBox::new(Orientation::Vertical, 0);
    right_panel.set_hexpand(true);
    right_panel.set_vexpand(true);

    // Meta bar (type badge + char count + time)
    let meta_bar = GtkBox::new(Orientation::Horizontal, 6);
    meta_bar.add_css_class("ai-model-bar");

    let detail_kind = Label::new(Some("TEXT"));
    detail_kind.add_css_class("ai-model-btn");
    detail_kind.add_css_class("active");
    detail_kind.set_valign(gtk::Align::Center);

    let meta_spacer = GtkBox::new(Orientation::Horizontal, 0);
    meta_spacer.set_hexpand(true);

    let detail_size = Label::new(None);
    detail_size.add_css_class("clipboard-time");
    detail_size.set_valign(gtk::Align::Center);

    let detail_mime = Label::new(None);
    detail_mime.add_css_class("clipboard-time");
    detail_mime.set_valign(gtk::Align::Center);

    meta_bar.append(&detail_kind);
    meta_bar.append(&meta_spacer);
    meta_bar.append(&detail_size);
    meta_bar.append(&detail_mime);
    right_panel.append(&meta_bar);

    // Content area
    let content_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let detail_preview = Label::new(None);
    detail_preview.add_css_class("clipboard-text");
    detail_preview.set_xalign(0.0);
    detail_preview.set_yalign(0.0);
    detail_preview.set_wrap(true);
    detail_preview.set_selectable(true);
    detail_preview.set_valign(gtk::Align::Start);
    detail_preview.set_margin_top(10);
    detail_preview.set_margin_bottom(10);
    detail_preview.set_margin_start(12);
    detail_preview.set_margin_end(12);
    content_scroll.set_child(Some(&detail_preview));
    right_panel.append(&content_scroll);

    // Invisible compat label for detail_title
    let detail_title = Label::new(None);
    detail_title.set_visible(false);
    right_panel.append(&detail_title);

    // Copy button (full width, bottom)
    let copy_row = GtkBox::new(Orientation::Horizontal, 0);
    copy_row.set_margin_top(0);
    copy_row.add_css_class("ai-input-row");

    let copy = Button::with_label("Copy to clipboard");
    copy.add_css_class("ai-send-btn");
    copy.add_css_class("ready");
    copy.set_hexpand(true);
    copy_row.append(&copy);
    right_panel.append(&copy_row);

    // Vertical separator between panels
    let vsep = gtk::Separator::new(Orientation::Vertical);
    root.append(&left_panel);
    root.append(&vsep);
    root.append(&right_panel);

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
        view.detail_preview.set_text("No item selected");
        view.detail_kind.set_text("–");
        view.detail_size.set_text("");
        view.detail_mime.set_text("");
        return;
    };

    view.detail_preview.set_text(&clipboard_detail_text(&item.value));
    view.detail_kind.set_text(item.kind.label());
    view.detail_size.set_text(&format_bytes(item.size_bytes));
    if let Some(ts) = item.timestamp {
        view.detail_mime.set_text(&crate::config::format_time_ago(ts));
    } else {
        view.detail_mime.set_text("");
    }

    // Code/URL → monospace style
    let is_code = matches!(
        item.kind,
        crate::ClipboardKind::Code | crate::ClipboardKind::Command
    );
    if is_code {
        view.detail_preview.add_css_class("code");
    } else {
        view.detail_preview.remove_css_class("code");
    }
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
    paned.set_position(138);
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
            let row = GtkBox::new(Orientation::Horizontal, 10);
            row.add_css_class("pref-field-row");

            let label = Label::new(Some(description));
            label.add_css_class("pref-field-label");
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_valign(gtk::Align::Center);
            row.append(&label);

            // Determine control type from description content
            let is_bool = description.contains("true/false");
            let is_numeric = key.contains("_ms") || key.contains("_size");

            let entry = Entry::new();
            entry.set_width_chars(if is_bool { 0 } else { 14 });
            entry.set_valign(gtk::Align::Center);
            if let Some(value) = current.get(*key) {
                entry.set_text(value);
            }
            entry.set_placeholder_text(Some(key));

            if is_bool {
                // Show a visual toggle hint in the entry
                let current_val = current.get(*key).map(String::as_str).unwrap_or("true");
                let toggle_btn = Button::with_label(if current_val == "false" { "Off" } else { "On" });
                toggle_btn.add_css_class("ai-model-btn");
                if current_val != "false" {
                    toggle_btn.add_css_class("active");
                }
                // Clicking toggles the hidden entry value
                let entry_c = entry.clone();
                let toggle_c = toggle_btn.clone();
                toggle_btn.connect_clicked(move |_| {
                    let cur = entry_c.text();
                    if cur.as_str() == "false" {
                        entry_c.set_text("true");
                        toggle_c.set_label("On");
                        toggle_c.add_css_class("active");
                    } else {
                        entry_c.set_text("false");
                        toggle_c.set_label("Off");
                        toggle_c.remove_css_class("active");
                    }
                });
                row.append(&toggle_btn);
            } else if is_numeric {
                entry.set_input_purpose(gtk::InputPurpose::Digits);
                row.append(&entry);
            } else {
                row.append(&entry);
            }

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

    let layout = GtkBox::new(Orientation::Horizontal, 8);
    layout.set_margin_top(0);
    layout.set_margin_bottom(0);
    layout.set_margin_start(12);
    layout.set_margin_end(12);
    layout.set_valign(gtk::Align::Center);

    // Type icon (16×16 with colored border when selected)
    let icon = gtk::Image::from_icon_name(item.kind.icon_name());
    icon.set_pixel_size(16);
    icon.add_css_class("result-icon");

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    // First line of preview (truncated)
    let first_line = item.preview.lines().next().unwrap_or(&item.preview);
    let title = Label::new(Some(first_line));
    let is_code = matches!(
        item.kind,
        crate::ClipboardKind::Code | crate::ClipboardKind::Command
    );
    if is_code {
        title.add_css_class("clipboard-text");
        title.add_css_class("code");
    } else {
        title.add_css_class("clipboard-text");
    }
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    text.append(&title);

    // Timestamp
    if let Some(ts) = item.timestamp {
        let ago = Label::new(Some(&crate::config::format_time_ago(ts)));
        ago.add_css_class("clipboard-time");
        ago.set_xalign(0.0);
        text.append(&ago);
    }

    layout.append(&icon);
    layout.append(&text);
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
    if !command.enabled {
        row.add_css_class("extension-disabled");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(0);
    layout.set_margin_bottom(0);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // 32×32 icon box
    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_width_request(32);
    icon_box.set_height_request(32);
    icon_box.add_css_class("control-card-icon");
    if command.enabled {
        icon_box.add_css_class("active");
    }
    icon_box.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name(&command.icon_name);
    icon.set_pixel_size(18);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_hexpand(true);
    icon.set_vexpand(true);
    icon_box.append(&icon);
    layout.append(&icon_box);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

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
    layout.append(&text);

    // Toggle switch — toggling changes row opacity visually
    let toggle = gtk::Switch::new();
    toggle.set_active(command.enabled);
    toggle.set_valign(gtk::Align::Center);
    {
        let row_ref = row.clone();
        let icon_box_ref = icon_box.clone();
        toggle.connect_active_notify(move |sw| {
            if sw.is_active() {
                row_ref.remove_css_class("extension-disabled");
                icon_box_ref.add_css_class("active");
            } else {
                row_ref.add_css_class("extension-disabled");
                icon_box_ref.remove_css_class("active");
            }
        });
    }
    layout.append(&toggle);

    row.set_child(Some(&layout));
    row
}

/// 4-bar Wi-Fi signal indicator (heights 4/7/10/13px)
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
        if i < filled {
            bar.add_css_class("stat-chip");
        } else {
            bar.add_css_class("section-header-row");
        }
        container.append(&bar);
    }
    container
}
