use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use chrono::Local;
use gtk::cairo;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, DrawingArea, DropDown, Entry, Grid, Image, Label, ListBox, ListBoxRow,
    Orientation, Paned, ProgressBar, Stack, StringList,
};

use crate::{
    Action, AudioDeviceOption, AudioDeviceSnapshot, AudioSnapshot, AudioStreamSnapshot,
    BatterySnapshot, ClipboardSummary, CommandSummary, MediaSnapshot, NetworkInterfaceSnapshot,
    NetworkSnapshot, NotificationSnapshot, ProcessSummary, SnippetSummary, SystemSnapshot,
    ThermalSnapshot,
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
    /// Container the dynamic model buttons are filled into.
    pub model_list: GtkBox,
    /// Re-fetch the model list from Ollama.
    pub refresh_models: Button,
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
    pub output_devices: ListBox,
    pub input_devices: ListBox,
    pub output_scale: gtk::Scale,
    pub input_scale: gtk::Scale,
    /// Set while we push real volumes into the scales so their value-changed
    /// handlers don't fire `wpctl set-volume` back at the device.
    suppress_volume_cb: Rc<Cell<bool>>,
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
    pub detail_image: gtk::Picture,
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
    pub memory_bar: DrawingArea,
    pub memory_bar_vals: Rc<RefCell<(f64, f64)>>,
    pub disk_bar: ProgressBar,
    pub load_graph: MetricGraph,
    pub memory_graph: MetricGraph,
    pub disk_graph: MetricGraph,
    pub net_iface: String,
    pub net_rx: Label,
    pub net_tx: Label,
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
    pub scrubber: gtk::Scale,
    pub time_pos: Label,
    pub time_total: Label,
    pub art_picture: Image,
    pub art_icon: Label,
    /// Last art URL we loaded, so we don't refetch on every refresh tick.
    art_url: Rc<RefCell<Option<String>>>,
}

#[derive(Clone)]
pub struct SnippetManagerView {
    pub root: GtkBox,
    pub list: ListBox,
}

#[derive(Clone)]
pub struct FontBrowserView {
    pub root: GtkBox,
    pub search: Entry,
    pub preview_entry: Entry,
    pub list: ListBox,
}

#[derive(Clone)]
pub struct EmojiPickerView {
    pub root: GtkBox,
    pub search: Entry,
    pub flow: gtk::FlowBox,
    pub confirm: Label,
}

#[derive(Clone)]
pub struct PreferencesView {
    pub root: GtkBox,
    pub fields: Vec<(String, Entry)>,
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

    let model_label = Label::new(Some("Model"));
    model_label.add_css_class("action-panel-label");
    model_label.set_valign(gtk::Align::Center);
    model_bar.append(&model_label);

    // Filled at runtime from the Ollama server (see populate_ai_models).
    let model_list = GtkBox::new(Orientation::Horizontal, 6);
    model_list.set_hexpand(true);
    model_bar.append(&model_list);

    let refresh_models = Button::with_label("⟳");
    refresh_models.add_css_class("ai-model-btn");
    refresh_models.set_valign(gtk::Align::Center);
    refresh_models.set_tooltip_text(Some("Refresh models"));
    model_bar.append(&refresh_models);

    root.append(&model_bar);

    // ── Messages scroll area ─────────────────────────────────────────────────
    let answer_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    answer_scroll.add_css_class("results-scroll");

    let output = Label::new(Some("Hi! Running on Ollama. Ask me anything."));
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
    stop.add_css_class("widget-btn");
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
        model_list,
        refresh_models,
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

    ScriptOutputView {
        root,
        title,
        output,
    }
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

    let suppress_volume_cb = Rc::new(Cell::new(false));

    let output_devices = ListBox::new();
    output_devices.add_css_class("results-list");
    output_devices.set_activate_on_single_click(true);
    // Populated from the real device list in set_audio_snapshot.
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
    output_bar_scale.set_draw_value(false);
    output_bar_scale.set_hexpand(true);
    output_bar_scale.add_css_class("audio-volume-bar");
    {
        let suppress = Rc::clone(&suppress_volume_cb);
        output_bar_scale.connect_value_changed(move |scale| {
            if suppress.get() {
                return;
            }
            set_default_volume("@DEFAULT_AUDIO_SINK@", scale.value());
        });
    }

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
    // Populated from the real device list in set_audio_snapshot.
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
    input_bar_scale.set_draw_value(false);
    input_bar_scale.set_hexpand(true);
    input_bar_scale.add_css_class("audio-volume-bar");
    {
        let suppress = Rc::clone(&suppress_volume_cb);
        input_bar_scale.connect_value_changed(move |scale| {
            if suppress.get() {
                return;
            }
            set_default_volume("@DEFAULT_AUDIO_SOURCE@", scale.value());
        });
    }

    let input_volume = Label::new(Some("80%"));
    input_volume.add_css_class("audio-volume-value");
    input_volume.set_width_chars(5);
    input_volume.set_xalign(1.0);

    in_vol_row.append(&mute_input);
    in_vol_row.append(&input_bar_scale);
    in_vol_row.append(&input_volume);
    root.append(&in_vol_row);

    // ── App streams ─────────────────────────────────────────────────────────
    // PipeWire per-application streams are intentionally not shown (they clutter
    // the view and aren't part of the target design). Kept for struct/data compat.
    let streams_list = super::results_list();
    streams_list.set_visible(false);

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
        output_devices,
        input_devices,
        output_scale: output_bar_scale,
        input_scale: input_bar_scale,
        suppress_volume_cb,
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
        super::metric_card("CPU", "utilities-system-monitor-symbolic");
    let (memory_card, memory, memory_sub, memory_bar) =
        super::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, disk_sub, disk_bar) =
        super::metric_card("Disk", "drive-harddisk-symbolic");
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
        super::control_card("Network", "network-wireless-symbolic");
    let (audio_card, audio, audio_row) = super::control_card("Audio", "audio-volume-high-symbolic");
    let (media_card, media, media_row) =
        super::control_card("Media", "media-playback-start-symbolic");
    // Keep notifications_card for struct compat (hidden)
    let (notifications_card, notifications, notify_row) =
        super::control_card("Notifications", "preferences-system-notifications-symbolic");
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
    load.add_css_class("mono");
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
                let col = if v > 0.8 {
                    (1.0, 0.42, 0.37, 1.0)
                } else if v > 0.6 {
                    (0.96, 0.65, 0.14, 1.0)
                } else {
                    (0.545, 0.486, 0.973, 1.0)
                };
                cr.set_source_rgba(col.0, col.1, col.2, col.3);
                cr.rectangle(x, y, bar_w - 2.0, bar_h);
                let _ = cr.fill();
            }
        });
    }

    let load_graph = metric_graph();
    load_graph.area.set_hexpand(false);
    load_graph.area.set_content_width(120);
    load_graph.area.set_content_height(40);
    load_graph.area.set_size_request(120, 40);
    cpu_row.append(&cpu_label);
    cpu_row.append(&load);
    cpu_row.append(&core_area);
    cpu_row.append(&load_graph.area);
    overview.append(&cpu_row);

    // RAM row — segmented bar (red=used, accent=cached)
    let ram_row = GtkBox::new(Orientation::Horizontal, 10);
    let ram_label = Label::new(Some("RAM"));
    ram_label.add_css_class("metric-label");
    ram_label.set_width_chars(4);
    ram_label.set_xalign(0.0);
    let memory = Label::new(Some("—"));
    memory.add_css_class("metric-value");
    memory.add_css_class("mono");
    memory.set_width_chars(10);
    memory.set_xalign(0.0);
    let memory_bar_vals = Rc::new(RefCell::new((0.0_f64, 0.0_f64)));
    let memory_bar = DrawingArea::new();
    memory_bar.add_css_class("dashboard-metric-bar");
    memory_bar.set_hexpand(true);
    memory_bar.set_content_height(5);
    memory_bar.set_valign(gtk::Align::Center);
    {
        let vals = Rc::clone(&memory_bar_vals);
        memory_bar.set_draw_func(move |_, cr, w, h| {
            let (used, cached) = *vals.borrow();
            let wf = w as f64;
            let hf = h as f64;
            // Track
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
            cr.rectangle(0.0, 0.0, wf, hf);
            let _ = cr.fill();
            // Used segment (red >85%, orange >65%, accent otherwise)
            let col = if used > 0.85 {
                (1.0_f64, 0.42, 0.37, 1.0)
            } else if used > 0.65 {
                (0.96, 0.65, 0.14, 1.0)
            } else {
                (0.545, 0.486, 0.973, 1.0)
            };
            cr.set_source_rgba(col.0, col.1, col.2, col.3);
            let used_w = wf * used.clamp(0.0, 1.0);
            cr.rectangle(0.0, 0.0, used_w, hf);
            let _ = cr.fill();
            // Cached segment (accent 50% alpha, after used)
            let cached_end = wf * (used + cached).clamp(0.0, 1.0);
            if cached_end > used_w {
                cr.set_source_rgba(0.545, 0.486, 0.973, 0.45);
                cr.rectangle(used_w, 0.0, cached_end - used_w, hf);
                let _ = cr.fill();
            }
        });
    }
    let memory_graph = metric_graph();
    ram_row.append(&ram_label);
    ram_row.append(&memory);
    ram_row.append(&memory_bar);
    overview.append(&ram_row);

    // Disk row
    let disk_row = GtkBox::new(Orientation::Horizontal, 10);
    let disk_label = Label::new(Some("DISK"));
    disk_label.add_css_class("metric-label");
    disk_label.set_width_chars(4);
    disk_label.set_xalign(0.0);
    let disk = Label::new(Some("—"));
    disk.add_css_class("metric-value");
    disk.add_css_class("mono");
    disk.set_width_chars(10);
    disk.set_xalign(0.0);
    let disk_bar = ProgressBar::new();
    disk_bar.add_css_class("dashboard-metric-bar");
    disk_bar.set_hexpand(true);
    let disk_graph = metric_graph();
    disk_row.append(&disk_label);
    disk_row.append(&disk);
    disk_row.append(&disk_bar);
    overview.append(&disk_row);

    // NET row
    let net_row = GtkBox::new(Orientation::Horizontal, 10);
    let net_label = Label::new(Some("NET"));
    net_label.add_css_class("metric-label");
    net_label.set_width_chars(4);
    net_label.set_xalign(0.0);
    let net_rx = Label::new(Some("—"));
    net_rx.add_css_class("result-subtitle");
    net_rx.add_css_class("mono");
    net_rx.set_xalign(0.0);
    let net_tx = Label::new(Some("—"));
    net_tx.add_css_class("result-subtitle");
    net_tx.add_css_class("mono");
    net_tx.set_xalign(0.0);
    net_tx.set_hexpand(true);
    net_row.append(&net_label);
    let rx_chip = GtkBox::new(Orientation::Horizontal, 3);
    rx_chip.add_css_class("stat-chip");
    let rx_icon = Label::new(Some("↓"));
    rx_icon.add_css_class("result-subtitle");
    rx_chip.append(&rx_icon);
    rx_chip.append(&net_rx);
    let tx_chip = GtkBox::new(Orientation::Horizontal, 3);
    tx_chip.add_css_class("stat-chip");
    let tx_icon = Label::new(Some("↑"));
    tx_icon.add_css_class("result-subtitle");
    tx_chip.append(&tx_icon);
    tx_chip.append(&net_tx);
    net_row.append(&rx_chip);
    net_row.append(&tx_chip);
    overview.append(&net_row);

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

    // Resolved lazily by refreshes; keep startup free of network subprocesses.
    let net_iface = "eth0".to_string();

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
    kill.add_css_class("widget-btn");
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
        memory_bar_vals,
        disk_bar,
        load_graph,
        memory_graph,
        disk_graph,
        net_iface,
        net_rx,
        net_tx,
        list,
        kill,
    };
    set_system_monitor_snapshot(&view, snapshot, processes);
    view
}

pub fn network_view(snapshot: &NetworkSnapshot) -> NetworkView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Section headers ("Ethernet", "Wi-Fi") live inside the list so their order
    // is data-driven (Ethernet is shown first when a wired link is present).
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
    // We're the notification daemon ourselves — there's no external panel to
    // open, so the Settings button is kept (struct compat) but hidden.
    let open_panel = Button::with_label("Settings");
    open_panel.add_css_class("action-bar-more");
    open_panel.set_visible(false);

    // DND / Clear All are wired in launcher.rs (so they can refresh the view).

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
    let info_row = GtkBox::new(Orientation::Horizontal, 16);
    info_row.set_margin_bottom(20);

    let art = GtkBox::new(Orientation::Vertical, 0);
    art.set_width_request(96);
    art.set_height_request(96);
    art.add_css_class("media-art");
    art.set_valign(gtk::Align::Start);
    art.set_halign(gtk::Align::Start);
    // Block the inner icon's expand (used to centre the glyph) from propagating
    // out and stretching the art box / info row.
    art.set_hexpand(false);
    art.set_vexpand(false);
    let art_icon = Label::new(Some("♪"));
    art_icon.set_vexpand(true);
    art_icon.set_hexpand(true);
    art_icon.set_valign(gtk::Align::Center);
    art_icon.set_halign(gtk::Align::Center);
    art_icon.add_css_class("media-art-icon");
    art.append(&art_icon);

    // Real album art (shown instead of the ♪ glyph once loaded). A fixed-size
    // Image scales the (large) cover texture down to 96px.
    let art_picture = Image::new();
    art_picture.set_pixel_size(96);
    art_picture.add_css_class("media-art-image");
    art_picture.set_visible(false);
    art.append(&art_picture);
    info_row.append(&art);

    let track_info = GtkBox::new(Orientation::Vertical, 4);
    track_info.set_valign(gtk::Align::Center);
    track_info.set_hexpand(true);

    let title = Label::new(Some("No active player"));
    title.add_css_class("media-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    // Artist line.
    let player = Label::new(None);
    player.add_css_class("media-artist");
    player.set_xalign(0.0);
    player.set_ellipsize(gtk::pango::EllipsizeMode::End);

    // "album · player" meta line.
    let status = Label::new(None);
    status.add_css_class("media-meta");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);

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

    let previous = media_ctrl_btn("media-skip-backward-symbolic", "Previous", "media-btn-skip");
    let seek_back = media_ctrl_btn(
        "media-seek-backward-symbolic",
        "Seek back 10s",
        "media-btn-seek",
    );
    let play_pause = media_play_btn("media-playback-start-symbolic");
    let seek_fwd = media_ctrl_btn(
        "media-seek-forward-symbolic",
        "Seek forward 10s",
        "media-btn-seek",
    );
    let next = media_ctrl_btn("media-skip-forward-symbolic", "Next", "media-btn-skip");

    // Wire MPRIS controls (direct D-Bus, no playerctl).
    previous.connect_clicked(|_| crate::media_control(crate::MediaControl::Previous));
    next.connect_clicked(|_| crate::media_control(crate::MediaControl::Next));
    play_pause.connect_clicked(|_| crate::media_control(crate::MediaControl::PlayPause));
    seek_back.connect_clicked(|_| crate::media_control(crate::MediaControl::SeekBy(-10_000_000)));
    seek_fwd.connect_clicked(|_| crate::media_control(crate::MediaControl::SeekBy(10_000_000)));
    scrubber.connect_change_value(|scale, _, val| {
        // Relative seek by the delta between the dragged value and the current one.
        let offset = ((val - scale.value()) * 1_000_000.0).round() as i64;
        crate::media_control(crate::MediaControl::SeekBy(offset));
        glib::Propagation::Proceed
    });

    controls.append(&previous);
    controls.append(&seek_back);
    controls.append(&play_pause);
    controls.append(&seek_fwd);
    controls.append(&next);
    root.append(&controls);

    // Absorbs the remaining height so the player stays pinned to the top
    // (matching the mockup) instead of centring/floating in the page.
    let spacer = GtkBox::new(Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    root.append(&spacer);

    let view = MediaView {
        root,
        player,
        status,
        title,
        previous,
        play_pause,
        next,
        scrubber,
        time_pos,
        time_total,
        art_picture,
        art_icon,
        art_url: Rc::new(RefCell::new(None)),
    };
    set_media_snapshot(&view, snapshot);
    view
}

fn media_ctrl_btn(icon_name: &str, tooltip: &str, css_class: &str) -> Button {
    // Symbolic icons (not emoji glyphs) so buttons stay crisp and recolour via
    // CSS. A square size_request keeps them perfectly round (border-radius 50%)
    // — otherwise a wide icon + button padding makes them oval.
    let btn = Button::from_icon_name(icon_name);
    btn.add_css_class(css_class);
    btn.set_tooltip_text(Some(tooltip));
    let size = if css_class == "media-btn-seek" {
        36
    } else {
        32
    };
    btn.set_size_request(size, size);
    btn.set_halign(gtk::Align::Center);
    btn.set_valign(gtk::Align::Center);
    btn
}

fn media_play_btn(_icon: &str) -> Button {
    let btn = Button::from_icon_name("media-playback-pause-symbolic");
    btn.add_css_class("media-btn-primary");
    btn.set_size_request(48, 48);
    btn.set_halign(gtk::Align::Center);
    btn.set_valign(gtk::Align::Center);
    btn
}

/// Swap the album art when the URL changes. `file://` loads synchronously;
/// `http(s)://` is fetched on a background thread (cached by URL so we don't
/// refetch on every refresh tick). Falls back to the ♪ glyph when absent.
fn update_media_art(view: &MediaView, art_url: Option<&str>) {
    if view.art_url.borrow().as_deref() == art_url {
        return; // unchanged — nothing to do
    }
    *view.art_url.borrow_mut() = art_url.map(str::to_string);

    let show_placeholder = |view: &MediaView| {
        view.art_picture.clear();
        view.art_picture.set_visible(false);
        view.art_icon.set_visible(true);
    };

    let Some(url) = art_url else {
        show_placeholder(view);
        return;
    };

    let set_texture = |view: &MediaView, texture: &gtk::gdk::Texture| {
        view.art_picture.set_property("paintable", texture);
        view.art_picture.set_visible(true);
        view.art_icon.set_visible(false);
    };

    if url.starts_with("file://") {
        match gtk::gdk::Texture::from_file(&gtk::gio::File::for_uri(url)) {
            Ok(texture) => set_texture(view, &texture),
            Err(_) => show_placeholder(view),
        }
        return;
    }

    if !(url.starts_with("http://") || url.starts_with("https://")) {
        show_placeholder(view);
        return;
    }

    // Remote art: fetch off-thread, deliver bytes back to the UI thread.
    let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
    let fetch_url = url.to_string();
    std::thread::spawn(move || {
        let bytes = ureq::get(&fetch_url).call().ok().and_then(|resp| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
                .ok()
                .map(|_| buf)
        });
        let _ = tx.send(bytes);
    });

    let picture = view.art_picture.clone();
    let icon = view.art_icon.clone();
    let expected = url.to_string();
    let art_url = Rc::clone(&view.art_url);
    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        match rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Ok(Some(bytes)) => {
                // Ignore if the track already moved on while we were fetching.
                if art_url.borrow().as_deref() == Some(expected.as_str()) {
                    let glib_bytes = glib::Bytes::from_owned(bytes);
                    if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                        picture.set_property("paintable", &texture);
                        picture.set_visible(true);
                        icon.set_visible(false);
                    }
                }
                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Break,
        }
    });
}

fn fmt_secs(s: f64) -> String {
    let total = s as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub fn set_media_snapshot(view: &MediaView, snapshot: &MediaSnapshot) {
    if snapshot.is_active() {
        // Title (bold) · artist · "album · player" — matches the mockup.
        view.title
            .set_text(snapshot.title.as_deref().unwrap_or("Unknown track"));
        view.player
            .set_text(snapshot.artist.as_deref().unwrap_or(""));

        let player = snapshot.player.as_deref().unwrap_or("");
        let meta = match snapshot.album.as_deref() {
            Some(album) if !album.is_empty() && !player.is_empty() => {
                format!("{album}  ·  {player}")
            }
            Some(album) if !album.is_empty() => album.to_string(),
            _ => player.to_string(),
        };
        view.status.set_text(&meta);

        update_media_art(view, snapshot.art_url.as_deref());

        let is_playing = snapshot.status.as_deref() == Some("Playing");
        view.play_pause.set_icon_name(if is_playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });

        if let Some(len) = snapshot.length_secs {
            view.scrubber.set_range(0.0, len);
            view.time_total.set_text(&fmt_secs(len));
        }
        if let Some(pos) = snapshot.position_secs {
            view.scrubber.set_value(pos);
            view.time_pos.set_text(&fmt_secs(pos));
        }
        view.scrubber.set_sensitive(snapshot.length_secs.is_some());
    } else {
        view.title.set_text("No active player");
        view.player
            .set_text("Start a media player to see MPRIS status");
        view.status.set_text("");
        update_media_art(view, None);
        view.play_pause
            .set_icon_name("media-playback-start-symbolic");
        view.scrubber.set_sensitive(false);
        view.time_pos.set_text("0:00");
        view.time_total.set_text("0:00");
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
        list.append(&super::section_header("Ethernet"));
        for iface in wired {
            list.append(&ethernet_row(iface));
        }
    }

    // ── Wi-Fi section ─────────────────────────────────────────────────────────
    list.append(&super::section_header("Wi-Fi"));
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
        list.append(&super::secondary_action_row(
            "network-wireless-offline-symbolic",
            "No Wi-Fi networks found",
        ));
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

    // App icon 32×32 — colored letter square
    let app_str = entry.app_name.as_deref().unwrap_or("App");
    let icon_area = super::letter_icon(app_str, 32);
    icon_area.set_valign(gtk::Align::Start);
    layout.append(&icon_area);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    // App name + timestamp row
    let meta_row = GtkBox::new(Orientation::Horizontal, 6);
    let app_name = Label::new(entry.app_name.as_deref().or(Some("App")));
    app_name.add_css_class("clipboard-time");
    app_name.set_xalign(0.0);
    app_name.set_hexpand(true);
    meta_row.append(&app_name);
    if let Some(ts) = &entry.timestamp {
        let ts_lbl = Label::new(Some(ts.as_str()));
        ts_lbl.add_css_class("notif-time");
        ts_lbl.set_xalign(1.0);
        meta_row.append(&ts_lbl);
    }
    text.append(&meta_row);

    // Summary (title)
    let title = Label::new(Some(&entry.summary));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    // Body
    if let Some(body) = &entry.body
        && !body.is_empty()
    {
        let body_lbl = Label::new(Some(body));
        body_lbl.add_css_class("result-subtitle");
        body_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        body_lbl.set_xalign(0.0);
        text.append(&body_lbl);
    }

    layout.append(&text);

    // Dismiss × button — closes the notification by id when available
    let dismiss = Button::with_label("×");
    dismiss.add_css_class("action-bar-btn");
    dismiss.add_css_class("kill-btn");
    dismiss.set_valign(gtk::Align::Start);
    if let Some(id) = entry.id {
        let row_weak = row.downgrade();
        dismiss.connect_clicked(move |_| {
            crate::close_notification(id);
            if let Some(row) = row_weak.upgrade() {
                row.set_visible(false);
            }
        });
    }
    layout.append(&dismiss);

    row.set_child(Some(&layout));
    row
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

    // Real device lists (click a row to make it the default device).
    populate_audio_device_list(&view.output_devices, &snapshot.output_devices, "Sinks");
    populate_audio_device_list(&view.input_devices, &snapshot.input_devices, "Sources");

    // Reflect real volumes on the sliders without re-triggering set-volume.
    view.suppress_volume_cb.set(true);
    if let Some(output) = snapshot.output.as_ref() {
        view.output_scale.set_value(output.volume_percent as f64);
    }
    if let Some(input) = snapshot.input.as_ref() {
        view.input_scale.set_value(input.volume_percent as f64);
    }
    view.suppress_volume_cb.set(false);

    set_audio_stream_rows(&view.streams_list, &snapshot.streams);
}

/// `wpctl set-volume <target> <percent>%` — clamped to a sane 0–150 range.
fn set_default_volume(target: &str, percent: f64) {
    let pct = percent.round().clamp(0.0, 150.0) as u32;
    let _ = std::process::Command::new("wpctl")
        .args(["set-volume", target, &format!("{pct}%")])
        .status();
}

/// Fill a device ListBox from real devices; clicking a row sets it as the
/// system default (`wpctl set-default <id>`) and repopulates in place.
fn populate_audio_device_list(
    list: &ListBox,
    devices: &[AudioDeviceOption],
    section: &'static str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if devices.is_empty() {
        list.append(&super::secondary_action_row(
            "audio-card-symbolic",
            "No devices found",
        ));
        return;
    }

    for device in devices {
        let row = audio_device_row(&device.name, device.is_default);
        if let Some(id) = device.id {
            let gesture = gtk::GestureClick::new();
            let list = list.clone();
            gesture.connect_released(move |_, _, _, _| {
                let _ = std::process::Command::new("wpctl")
                    .args(["set-default", &id.to_string()])
                    .status();
                let snapshot = crate::audio_snapshot();
                let devices = if section == "Sinks" {
                    snapshot.output_devices
                } else {
                    snapshot.input_devices
                };
                populate_audio_device_list(&list, &devices, section);
            });
            row.add_controller(gesture);
        }
        list.append(&row);
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
    let cached_fraction = match (snapshot.memory_cached_kib, snapshot.memory_total_kib) {
        (Some(cached), Some(total)) if total > 0 => {
            (cached as f64 / total as f64).clamp(0.0, 1.0 - memory_fraction)
        }
        _ => 0.0,
    };
    *view.memory_bar_vals.borrow_mut() = (memory_fraction, cached_fraction);
    view.memory_bar.queue_draw();
    push_metric_graph(&view.memory_graph, memory_fraction);

    // NET row speeds
    let (rx_mbps, tx_mbps) = crate::net_speed_mbps(&view.net_iface);
    let fmt_speed = |v: f64| -> String {
        if v < 0.001 {
            "0 B/s".to_string()
        } else if v < 1.0 {
            format!("{:.0} KB/s", v * 1000.0)
        } else {
            format!("{v:.1} MB/s")
        }
    };
    view.net_rx.set_text(&fmt_speed(rx_mbps));
    view.net_tx.set_text(&fmt_speed(tx_mbps));
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
    dot.add_css_class(if active {
        "radio-dot-active"
    } else {
        "radio-dot-inactive"
    });
    layout.append(&dot);

    let label = Label::new(Some(name));
    label.add_css_class(if active {
        "result-title"
    } else {
        "result-subtitle"
    });
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

fn dashboard_button(label: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("dashboard-button");
    button.add_css_class("widget-btn");
    button
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
    title.add_css_class("process-name");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    layout.append(&title);

    // Mini memory usage bar (36×3px, width relative to max in process list).
    // Colour follows usage: subtle → purple → amber.
    let mem_frac = process
        .memory_kib
        .map(|v| v as f64 / max_memory_kib.max(1) as f64)
        .unwrap_or(0.0);
    let mem_bar = ProgressBar::new();
    mem_bar.add_css_class("process-memory-bar");
    mem_bar.add_css_class(if mem_frac > 0.5 {
        "usage-high"
    } else if mem_frac > 0.15 {
        "usage-mid"
    } else {
        "usage-low"
    });
    mem_bar.set_fraction(mem_frac.clamp(0.0, 1.0));
    mem_bar.set_show_text(false);
    mem_bar.set_width_request(36);
    layout.append(&mem_bar);

    // MEM
    let mem_text = process
        .memory_kib
        .map(|v| {
            if v >= 1024 * 1024 {
                format!("{:.1}G", v as f64 / 1024.0 / 1024.0)
            } else {
                format!("{}M", v / 1024)
            }
        })
        .unwrap_or_else(|| "—".to_string());
    let mem_lbl = Label::new(Some(&mem_text));
    mem_lbl.add_css_class("clipboard-time");
    mem_lbl.add_css_class("mono");
    mem_lbl.set_width_chars(6);
    mem_lbl.set_xalign(1.0);
    layout.append(&mem_lbl);

    // Kill × — hidden, shown only when row is selected (via CSS .kill-btn)
    let kill_btn = Button::with_label("×");
    kill_btn.add_css_class("action-bar-btn");
    kill_btn.add_css_class("kill-btn");
    kill_btn.set_valign(gtk::Align::Center);
    kill_btn.set_tooltip_text(Some("Kill process"));
    layout.append(&kill_btn);

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

    // Content area holds either the text label or an image preview.
    let content_box = GtkBox::new(Orientation::Vertical, 0);

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
    content_box.append(&detail_preview);

    // Fills the preview pane width; height follows the image aspect ratio.
    let detail_image = gtk::Picture::new();
    detail_image.set_can_shrink(true);
    detail_image.set_hexpand(true);
    detail_image.set_halign(gtk::Align::Fill);
    detail_image.set_valign(gtk::Align::Start);
    detail_image.add_css_class("clipboard-image");
    detail_image.set_visible(false);
    content_box.append(&detail_image);

    content_scroll.set_child(Some(&content_box));
    right_panel.append(&content_scroll);

    // Invisible compat label for detail_title
    let detail_title = Label::new(None);
    detail_title.set_visible(false);
    right_panel.append(&detail_title);

    // Copy button (full width, bottom)
    let copy_row = GtkBox::new(Orientation::Horizontal, 0);
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
        detail_image,
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
        view.detail_preview.set_visible(true);
        view.detail_image.set_visible(false);
        view.detail_preview.set_text("No item selected");
        view.detail_kind.set_text("–");
        view.detail_size.set_text("");
        view.detail_mime.set_text("");
        return;
    };

    view.detail_kind.set_text(item.kind.label());
    if let Some(ts) = item.timestamp {
        view.detail_mime
            .set_text(&format!("·  {}", crate::config::format_time_ago(ts)));
    } else {
        view.detail_mime.set_text("");
    }

    // Image entry → show the picture; otherwise the text label.
    if let Some(path) = crate::clipboard_image_path(&item.value) {
        view.detail_preview.set_visible(false);
        view.detail_image.set_visible(true);
        match gtk::gdk::Texture::from_filename(path) {
            Ok(texture) => {
                view.detail_image.set_paintable(Some(&texture));
                view.detail_size
                    .set_text(&format!("{}×{}", texture.width(), texture.height()));
            }
            Err(_) => {
                view.detail_image.set_paintable(gtk::gdk::Paintable::NONE);
                view.detail_size.set_text("missing");
            }
        }
        return;
    }

    view.detail_preview.set_visible(true);
    view.detail_image.set_visible(false);
    view.detail_preview
        .set_text(&clipboard_detail_text(&item.value));
    // Character count (matches the mockup's "N ch"), not raw byte size.
    view.detail_size
        .set_text(&format!("{} ch", item.value.chars().count()));

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

pub fn font_browser_view() -> FontBrowserView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Top bar: search + preview text input
    let top_bar = GtkBox::new(Orientation::Horizontal, 8);
    top_bar.add_css_class("search-bar");

    let search = Entry::builder()
        .placeholder_text("Search fonts…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");

    let preview_entry = Entry::builder()
        .text("The quick brown fox jumps over the lazy dog")
        .width_chars(24)
        .build();
    preview_entry.add_css_class("search-entry");

    top_bar.append(&search);
    top_bar.append(&preview_entry);
    root.append(&top_bar);

    // Font list
    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroll);

    // Populate initial list
    let fonts = list_system_fonts();
    populate_font_list(
        &list,
        &fonts,
        "",
        "The quick brown fox jumps over the lazy dog",
    );

    // Wire up search filter
    {
        let list_c = list.clone();
        let preview_c = preview_entry.clone();
        let fonts_c = fonts.clone();
        search.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c, &e.text(), &preview_c.text());
        });
    }
    {
        let list_c = list.clone();
        let search_c = search.clone();
        let fonts_c = fonts.clone();
        preview_entry.connect_changed(move |e| {
            populate_font_list(&list_c, &fonts_c, &search_c.text(), &e.text());
        });
    }

    FontBrowserView {
        root,
        search,
        preview_entry,
        list,
    }
}

fn list_system_fonts() -> Vec<String> {
    let out = std::process::Command::new("fc-list")
        .args([":", "family"])
        .output();
    match out {
        Ok(o) => {
            let mut fonts: Vec<String> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .flat_map(|l| l.split(',').map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty() && !s.starts_with('.'))
                .collect();
            fonts.sort();
            fonts.dedup();
            fonts
        }
        Err(_) => vec!["Sans".into(), "Serif".into(), "Monospace".into()],
    }
}

fn populate_font_list(list: &ListBox, fonts: &[String], query: &str, preview: &str) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let q = query.trim().to_lowercase();
    let preview_text = if preview.trim().is_empty() {
        "The quick brown fox jumps over the lazy dog"
    } else {
        preview
    };

    let mut shown = 0;
    for font in fonts {
        if !q.is_empty() && !font.to_lowercase().contains(&q) {
            continue;
        }
        if shown >= 120 {
            break;
        }
        list.append(&font_row(font, preview_text));
        shown += 1;
    }
}

fn font_row(font_name: &str, preview: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    row.set_height_request(52);

    let layout = GtkBox::new(Orientation::Vertical, 2);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);
    layout.set_margin_top(6);
    layout.set_margin_bottom(6);

    // Font name label (uppercase, muted, 10px)
    let name_lbl = Label::new(Some(font_name));
    name_lbl.add_css_class("font-name-label");
    name_lbl.set_xalign(0.0);
    layout.append(&name_lbl);

    // Preview text in that font using Pango markup
    let escaped = font_name.replace('"', "");
    let safe_preview: String = preview.chars().take(60).collect();
    let markup = format!(
        "<span font_desc=\"{escaped} 16\">{}</span>",
        glib::markup_escape_text(&safe_preview)
    );
    let preview_lbl = Label::new(None);
    preview_lbl.set_markup(&markup);
    preview_lbl.set_xalign(0.0);
    preview_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    layout.append(&preview_lbl);

    row.set_child(Some(&layout));
    row
}

pub fn emoji_picker_view() -> EmojiPickerView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Search row
    let search_row = GtkBox::new(Orientation::Horizontal, 0);
    search_row.add_css_class("search-bar");
    let search = Entry::builder()
        .placeholder_text("Search emoji…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    search_row.append(&search);
    root.append(&search_row);

    // Category tab bar (horizontally scrollable)
    let cat_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let cat_bar = GtkBox::new(Orientation::Horizontal, 4);
    cat_bar.set_margin_top(6);
    cat_bar.set_margin_bottom(6);
    cat_bar.set_margin_start(10);
    cat_bar.set_margin_end(10);
    cat_scroll.set_child(Some(&cat_bar));
    root.append(&cat_scroll);

    // Emoji grid (FlowBox)
    let flow_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    let flow = gtk::FlowBox::new();
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(12);
    flow.set_min_children_per_line(6);
    flow.set_column_spacing(2);
    flow.set_row_spacing(2);
    flow.set_margin_start(8);
    flow.set_margin_end(8);
    flow.set_margin_top(4);
    flow_scroll.set_child(Some(&flow));
    root.append(&flow_scroll);

    // Confirmation strip
    let confirm = Label::new(None);
    confirm.add_css_class("emoji-confirm");
    confirm.set_halign(gtk::Align::Center);
    confirm.set_hexpand(true);
    confirm.set_visible(false);
    confirm.set_margin_top(4);
    confirm.set_margin_bottom(6);
    root.append(&confirm);

    // Build category buttons and initial grid
    const CATEGORIES: &[(&str, &str)] = &[
        ("all", "All"),
        ("smileys", "😀 Smileys"),
        ("gestures", "👍 Gestures"),
        ("body", "👁 Body"),
        ("symbols", "❤️ Symbols"),
        ("celebration", "🎉 Celebration"),
        ("travel", "✈️ Travel"),
        ("food", "🍎 Food"),
        ("animals", "🐾 Animals"),
        ("nature", "🌿 Nature"),
        ("music", "🎵 Music"),
        ("sports", "⚽ Sports"),
        ("technology", "💻 Technology"),
        ("tools", "🔧 Tools"),
        ("office", "📁 Office"),
        ("communication", "💬 Communication"),
        ("weather", "☀️ Weather"),
    ];

    let active_cat = Rc::new(RefCell::new("all".to_string()));

    for &(cat_id, cat_label) in CATEGORIES {
        let btn = Button::with_label(cat_label);
        btn.add_css_class("ai-model-btn");
        if cat_id == "all" {
            btn.add_css_class("active");
        }
        let flow_c = flow.clone();
        let confirm_c = confirm.clone();
        let active_cat_c = Rc::clone(&active_cat);
        let cat_bar_c = cat_bar.clone();
        let cat_id_s = cat_id.to_string();
        btn.connect_clicked(move |clicked_btn| {
            // Update active category
            *active_cat_c.borrow_mut() = cat_id_s.clone();
            // Update button styles
            let mut child = cat_bar_c.first_child();
            while let Some(w) = child {
                if let Some(b) = w.downcast_ref::<Button>() {
                    b.remove_css_class("active");
                }
                child = w.next_sibling();
            }
            clicked_btn.add_css_class("active");
            // Repopulate grid
            populate_emoji_flow(&flow_c, &cat_id_s, "", &confirm_c);
        });
        cat_bar.append(&btn);
    }

    // Initial population
    populate_emoji_flow(&flow, "all", "", &confirm);

    // Search updates grid
    {
        let flow_c = flow.clone();
        let confirm_c = confirm.clone();
        let active_cat_c = Rc::clone(&active_cat);
        search.connect_changed(move |entry| {
            let query = entry.text().to_string();
            populate_emoji_flow(&flow_c, &active_cat_c.borrow(), &query, &confirm_c);
        });
    }

    EmojiPickerView {
        root,
        search,
        flow,
        confirm,
    }
}

fn populate_emoji_flow(flow: &gtk::FlowBox, category: &str, query: &str, confirm: &Label) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }

    let emoji_data = crate::search::emoji::emoji_data();
    let query_lower = query.to_lowercase();

    for &(emoji, name, cat) in emoji_data {
        let cat_match = category == "all" || cat == category;
        let query_match = query_lower.is_empty()
            || emoji.contains(&*query_lower)
            || name.contains(&*query_lower)
            || cat.contains(&*query_lower);
        if !cat_match || !query_match {
            continue;
        }

        let btn = Button::with_label(emoji);
        btn.add_css_class("emoji-btn");
        btn.set_width_request(36);
        btn.set_height_request(36);
        btn.set_tooltip_text(Some(name));

        let confirm_c = confirm.clone();
        let emoji_s = emoji.to_string();
        let name_s = name.to_string();
        btn.connect_clicked(move |_| {
            crate::copy_text(&emoji_s);
            confirm_c.set_text(&format!("Copied  {emoji_s}  {name_s}"));
            confirm_c.set_visible(true);
        });

        flow.insert(&btn, -1);
    }
}

/// A linked segmented toggle (radio) for a fixed set of `(value, label)`
/// choices, writing the chosen value into the bound preference `entry`.
fn segmented_choice(options: &[(&str, &str)], current: &str, entry: &Entry) -> GtkBox {
    let btn_box = GtkBox::new(Orientation::Horizontal, 0);
    btn_box.add_css_class("linked");
    btn_box.set_valign(gtk::Align::Center);

    let has_match = options.iter().any(|(value, _)| *value == current);
    let mut anchor: Option<gtk::ToggleButton> = None;

    for (i, (value, label)) in options.iter().enumerate() {
        let btn = gtk::ToggleButton::with_label(label);
        match &anchor {
            Some(group) => btn.set_group(Some(group)),
            None => anchor = Some(btn.clone()),
        }
        let entry_c = entry.clone();
        let value_owned = value.to_string();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                entry_c.set_text(&value_owned);
            }
        });
        // Activate the matching option (or the first when the stored value is
        // unknown). Done after wiring so the entry reflects the selection.
        btn.set_active(*value == current || (!has_match && i == 0));
        btn_box.append(&btn);
    }

    btn_box
}

pub fn preferences_view(current: &HashMap<String, String>) -> PreferencesView {
    let outer = super::panel_root(0, 0);
    outer.set_vexpand(true);
    outer.set_hexpand(true);

    // No in-view title/search: the nav header already shows "‹ Preferences",
    // and the design goes straight to the sidebar + content panes.
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

        // Special static sections
        match section.name {
            "About" => {
                let about_text = format!(
                    "zeshicast  v{}\nRaycast-inspired launcher for Wayland / Niri\n\nBuilt with Rust + GTK4\nOS: {}\nArch: {}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::var("PRETTY_NAME")
                        .or_else(|_| std::fs::read_to_string("/etc/os-release")
                            .ok()
                            .and_then(|s| s
                                .lines()
                                .find(|l| l.starts_with("PRETTY_NAME"))
                                .and_then(|l| l.split('=').nth(1))
                                .map(|v| v.trim_matches('"').to_string()))
                            .ok_or(std::env::VarError::NotPresent))
                        .unwrap_or_else(|_| "Linux".to_string()),
                    std::env::consts::ARCH,
                );
                let lbl = Label::new(Some(&about_text));
                lbl.add_css_class("result-subtitle");
                lbl.set_xalign(0.0);
                lbl.set_wrap(true);
                lbl.set_selectable(true);
                lbl.set_margin_start(14);
                lbl.set_margin_top(14);
                fields_box.append(&lbl);
            }
            "Keyboard" => {
                let shortcuts = [
                    ("Super+Space", "Open launcher"),
                    ("Escape", "Close / go back / clear"),
                    ("↑ / ↓", "Navigate results"),
                    ("Enter", "Launch selected"),
                    ("Tab", "Jump to AI Chat"),
                    ("Ctrl+K", "Open Action Panel"),
                    ("Ctrl+D", "Dashboard"),
                    ("Ctrl+T", "System Monitor"),
                    ("Ctrl+I", "AI Chat"),
                    ("Ctrl+M", "Media"),
                    ("Ctrl+O", "Audio"),
                    ("Ctrl+N", "Network"),
                    ("Ctrl+H", "Clipboard"),
                    ("Ctrl+B", "Extensions"),
                    ("Ctrl+,", "Preferences"),
                    ("=", "Calculator mode"),
                ];
                for (key, desc) in shortcuts {
                    let row = GtkBox::new(Orientation::Horizontal, 10);
                    row.add_css_class("pref-field-row");
                    let lbl = Label::new(Some(desc));
                    lbl.add_css_class("pref-field-label");
                    lbl.set_xalign(0.0);
                    lbl.set_hexpand(true);
                    row.append(&lbl);
                    let kbd = Label::new(Some(key));
                    kbd.add_css_class("ctrl-k-hint");
                    kbd.set_xalign(1.0);
                    row.append(&kbd);
                    fields_box.append(&row);
                }
            }
            "Privacy" => {
                let privacy_rows = [
                    (
                        "Clipboard history",
                        "Stores last 50 clipboard entries locally",
                    ),
                    (
                        "Usage frequency",
                        "Tracks launch frequency for frecency scoring",
                    ),
                    ("No telemetry", "Zero data sent to remote servers"),
                    ("Config location", "~/.config/zeshicast/"),
                ];
                for (name, detail) in privacy_rows {
                    let row = GtkBox::new(Orientation::Horizontal, 10);
                    row.add_css_class("pref-field-row");
                    let text = GtkBox::new(Orientation::Vertical, 2);
                    text.set_hexpand(true);
                    let name_lbl = Label::new(Some(name));
                    name_lbl.add_css_class("pref-field-label");
                    name_lbl.set_xalign(0.0);
                    let detail_lbl = Label::new(Some(detail));
                    detail_lbl.add_css_class("result-subtitle");
                    detail_lbl.set_xalign(0.0);
                    text.append(&name_lbl);
                    text.append(&detail_lbl);
                    row.append(&text);
                    fields_box.append(&row);
                }
            }
            _ => {
                for (key, description) in section.keys {
                    let row = GtkBox::new(Orientation::Horizontal, 10);
                    row.add_css_class("pref-field-row");

                    let label = Label::new(Some(description));
                    label.add_css_class("pref-field-label");
                    label.set_xalign(0.0);
                    label.set_hexpand(true);
                    label.set_valign(gtk::Align::Center);
                    row.append(&label);

                    let default_val = super::preferences::PREFERENCE_DEFAULTS
                        .iter()
                        .find(|(k, _)| *k == *key)
                        .map(|(_, v)| *v)
                        .unwrap_or("");
                    let effective_val =
                        current.get(*key).map(String::as_str).unwrap_or(default_val);

                    // A preference is boolean when its default is true/false.
                    let is_bool = matches!(default_val, "true" | "false");
                    let is_numeric = key.contains("_ms") || key.contains("_size");

                    let entry = Entry::new();
                    entry.add_css_class("pref-entry");
                    entry.set_width_chars(if is_bool { 0 } else { 14 });
                    entry.set_valign(gtk::Align::Center);
                    entry.set_text(effective_val);
                    entry.set_placeholder_text(Some(default_val));

                    if is_bool {
                        let current_val = effective_val;
                        let sw = gtk::Switch::new();
                        sw.set_active(current_val != "false");
                        sw.set_valign(gtk::Align::Center);
                        let entry_c = entry.clone();
                        sw.connect_active_notify(move |sw| {
                            entry_c.set_text(if sw.is_active() { "true" } else { "false" });
                        });
                        row.append(&sw);
                    } else if *key == "ui_font_size" {
                        let scale =
                            gtk::Scale::with_range(Orientation::Horizontal, 12.0, 22.0, 1.0);
                        scale.set_hexpand(true);
                        scale.set_draw_value(true);
                        scale.set_value_pos(gtk::PositionType::Right);
                        scale.set_value(effective_val.parse::<f64>().unwrap_or(15.0));
                        scale.set_valign(gtk::Align::Center);
                        let entry_c = entry.clone();
                        scale.connect_value_changed(move |s| {
                            entry_c.set_text(&format!("{}", s.value() as u32));
                        });
                        row.append(&scale);
                    } else if *key == "ui_density" {
                        row.append(&segmented_choice(
                            &[("compact", "Compact"), ("comfortable", "Comfortable")],
                            effective_val,
                            &entry,
                        ));
                    } else if *key == "ui_theme" {
                        row.append(&segmented_choice(
                            &[("system", "System"), ("dark", "Dark"), ("light", "Light")],
                            effective_val,
                            &entry,
                        ));
                    } else if *key == "ai_provider" {
                        row.append(&segmented_choice(
                            &[("ollama", "Ollama"), ("openai", "OpenAI")],
                            effective_val,
                            &entry,
                        ));
                    } else if *key == "dashboard_poll_interval_ms" {
                        let spin = gtk::SpinButton::with_range(500.0, 5000.0, 100.0);
                        spin.add_css_class("pref-entry");
                        spin.set_value(effective_val.parse::<f64>().unwrap_or(1000.0));
                        spin.set_valign(gtk::Align::Center);
                        let entry_c = entry.clone();
                        spin.connect_value_changed(move |s| {
                            entry_c.set_text(&format!("{}", s.value() as u32));
                        });
                        row.append(&spin);
                    } else if is_numeric {
                        entry.set_input_purpose(gtk::InputPurpose::Digits);
                        row.append(&entry);
                    } else {
                        // Text values (lists, endpoints…) can be long: let the
                        // field take the row's free width so they aren't clipped.
                        label.set_hexpand(false);
                        entry.set_hexpand(true);
                        entry.set_width_chars(0);
                        // Mask secrets.
                        if key.ends_with("_api_key") {
                            entry.set_visibility(false);
                            entry.set_input_purpose(gtk::InputPurpose::Password);
                        }
                        row.append(&entry);
                    }

                    fields.push((key.to_string(), entry));
                    fields_box.append(&row);
                }
            }
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

    if let Some(row) = sidebar.row_at_index(0) {
        sidebar.select_row(Some(&row));
    }
    if let Some(first) = super::preferences::PREFERENCE_SECTIONS.first() {
        content_stack.set_visible_child_name(first.name);
    }

    paned.set_start_child(Some(&sidebar_scroller));
    paned.set_end_child(Some(&content_stack));
    outer.append(&paned);

    // No Save/Cancel buttons (not in the mockup): changes auto-save, wired in
    // launcher.rs against each field's `changed` signal.
    PreferencesView {
        root: outer,
        fields,
    }
}

fn clipboard_row(item: &ClipboardSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 8);
    layout.set_margin_start(12);
    layout.set_margin_end(12);
    layout.set_valign(gtk::Align::Center);

    // Type icon (16×16; image entries use the generic image icon).
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

fn clipboard_detail_text(value: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 1400;
    if value.chars().count() <= MAX_DETAIL_CHARS {
        return value.to_string();
    }

    let mut detail = value.chars().take(MAX_DETAIL_CHARS).collect::<String>();
    detail.push_str("\n...");
    detail
}

fn extension_row(command: &CommandSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    if !command.enabled {
        row.add_css_class("extension-disabled");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 10);
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
        command.description.clone()
    } else {
        command.keyword.clone().unwrap_or_default()
    };
    let capability_text = if command.capabilities.is_empty() {
        "capabilities: none".to_string()
    } else {
        format!("capabilities: {}", command.capabilities.join(", "))
    };
    let subtitle_text = if subtitle_text.is_empty() {
        capability_text
    } else {
        format!("{subtitle_text} - {capability_text}")
    };
    let subtitle = Label::new(Some(&subtitle_text));
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
/// A wired-interface row, styled to match the Wi-Fi rows (icon · name ·
/// status), with the purple active treatment when the link is up.
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
