use std::cell::RefCell;
use std::rc::Rc;

use gtk::cairo;
use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, DrawingArea, Label, ListBox, Orientation, ProgressBar};

use crate::{ProcessSummary, SystemSnapshot, ThermalSnapshot};

#[derive(Clone)]
pub struct MetricGraph {
    area: DrawingArea,
    values: Rc<RefCell<Vec<f64>>>,
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

    let list = crate::ui::results_list();
    list.set_vexpand(true);
    let scroller = crate::ui::scrollable_list(&list);
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


pub(crate) fn metric_graph() -> MetricGraph {
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

pub(crate) fn push_metric_graph(graph: &MetricGraph, value: f64) {
    let mut values = graph.values.borrow_mut();
    // Start empty — graph fills from left as data arrives
    values.push(value.clamp(0.0, 1.0));
    if values.len() > 60 {
        values.remove(0);
    }
    graph.area.queue_draw();
}

pub(crate) fn load_fraction(load: f32) -> f64 {
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
        list.append(&crate::ui::secondary_action_row(
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

