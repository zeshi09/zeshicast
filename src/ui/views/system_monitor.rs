use std::cell::RefCell;
use std::rc::Rc;
use gtk::cairo;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, DrawingArea, Label, ListBox, Orientation, ProgressBar,
};
use crate::{ProcessSummary, SystemSnapshot, ThermalSnapshot};
use super::dashboard::{dashboard_grid, dashboard_plain_card, format_duration};

#[derive(Clone)]
pub struct MetricGraph {
    pub area: DrawingArea,
    pub values: Rc<RefCell<Vec<f64>>>,
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

pub fn system_monitor_view(
    snapshot: &SystemSnapshot,
    processes: &[ProcessSummary],
) -> SystemMonitorView {
    let root = crate::ui::panel_root(10, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("System Monitor");
    root.append(&header);

    let metric_grid = dashboard_grid();
    let (uptime_card, uptime, _) =
        crate::ui::control_card("Uptime", "appointment-soon-symbolic");
    let (load_card, load, _, load_bar) =
        crate::ui::metric_card("Load", "utilities-system-monitor-symbolic");
    let (memory_card, memory, _, memory_bar) =
        crate::ui::metric_card("Memory", "media-flash-symbolic");
    let (disk_card, disk, _, disk_bar) =
        crate::ui::metric_card("Disk", "drive-harddisk-symbolic");
    let (temperature_card, temperature, _) =
        crate::ui::control_card("Temperature", "weather-clear-symbolic");
    let (process_count_card, process_count, _) =
        crate::ui::control_card("Processes", "application-x-executable-symbolic");
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

    let list = crate::ui::results_list();
    let scroller = crate::ui::scrollable_list(&list);
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


pub fn metric_graph() -> MetricGraph {
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

pub fn push_metric_graph(graph: &MetricGraph, value: f64) {
    let mut values = graph.values.borrow_mut();
    // Start empty — graph fills from left as data arrives
    values.push(value.clamp(0.0, 1.0));
    if values.len() > 60 {
        values.remove(0);
    }
    graph.area.queue_draw();
}

pub fn load_fraction(load: f32) -> f64 {
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
