use gtk::cairo;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, DrawingArea, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ProgressBar, ScrolledWindow,
};

use crate::Action;

pub fn action_panel(
    parent: &ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> Option<ApplicationWindow> {
    let app = parent.application()?;
    let panel = ApplicationWindow::builder()
        .application(&app)
        .title(title)
        .transient_for(parent)
        .default_width(default_width)
        .default_height(default_height)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");
    Some(panel)
}

pub fn panel_root(spacing: i32, margin: i32) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, spacing);
    root.set_margin_top(margin);
    root.set_margin_bottom(margin);
    root.set_margin_start(margin);
    root.set_margin_end(margin);
    root
}

pub fn panel_title(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("action-panel-title");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}

pub fn results_list() -> ListBox {
    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);
    list
}

pub fn scrollable_list(list: &ListBox) -> ScrolledWindow {
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .propagate_natural_height(false)
        .child(list)
        .build();
    scroller.add_css_class("results-scroll");
    scroller.set_vexpand(true);
    scroller
}

pub fn move_selection(list: &ListBox, delta: i32) {
    let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
    let mut next = (current + delta).max(0);

    while let Some(row) = list.row_at_index(next) {
        if row.is_selectable() {
            list.select_row(Some(&row));
            return;
        }
        if next == 0 && delta < 0 {
            return;
        }
        next = (next + delta.signum()).max(0);
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h1 = h / 60.0;
    let x = c * (1.0 - (h1 % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if h1 < 1.0 { (c, x, 0.0) }
        else if h1 < 2.0 { (x, c, 0.0) }
        else if h1 < 3.0 { (0.0, c, x) }
        else if h1 < 4.0 { (0.0, x, c) }
        else if h1 < 5.0 { (x, 0.0, c) }
        else { (c, 0.0, x) };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

fn app_icon_color(name: &str) -> (f64, f64, f64) {
    let lower = name.to_lowercase();
    // Named overrides for well-known apps (match main_maket.png)
    if lower.contains("terminal") || lower.contains("alacritty") || lower.contains("kitty") || lower.contains("foot") {
        return (0.118, 0.302, 0.227); // #1E4D3A
    }
    if lower.contains("code") || lower.contains("vscode") || lower.contains("vscodium") {
        return (0.102, 0.227, 0.420); // #1A3A6B
    }
    if lower.contains("firefox") {
        return (0.478, 0.157, 0.0); // #7A2800
    }
    if lower.contains("spotify") {
        return (0.078, 0.353, 0.196); // #145A32
    }
    if lower.contains("notion") {
        return (0.173, 0.173, 0.173); // #2C2C2C
    }
    if lower.contains("slack") {
        return (0.239, 0.122, 0.369); // #3D1F5E
    }
    if lower.contains("telegram") {
        return (0.161, 0.502, 0.725);
    }
    if lower.contains("chrome") || lower.contains("chromium") {
        return (0.267, 0.655, 0.322);
    }
    // Hash-based: deterministic hue from name, fixed saturation/lightness
    let hash = name.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    let hue = (hash % 360) as f64;
    hsl_to_rgb(hue, 0.55, 0.32)
}

pub fn letter_icon(title: &str, size: i32) -> DrawingArea {
    use gtk::prelude::*;
    let first = title.chars().next().unwrap_or('?').to_ascii_uppercase();
    let color = app_icon_color(title);
    let letter_str = first.to_string();

    let area = DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_size_request(size, size);
    area.set_valign(gtk::Align::Center);

    area.set_draw_func(move |_, cr, w, h| {
        let wf = w as f64;
        let hf = h as f64;
        let r = (wf.min(hf) * 0.22).round(); // corner radius ≈ 7px for 28px square

        // Rounded rect background
        cr.new_sub_path();
        cr.arc(wf - r, r, r, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(wf - r, hf - r, r, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(r, hf - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
        cr.arc(r, r, r, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
        cr.close_path();
        cr.set_source_rgb(color.0, color.1, color.2);
        let _ = cr.fill();

        // Letter (white, centered, bold)
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.92);
        let font_size = wf * 0.48;
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size(font_size);
        let (tx, ty) = if let Ok(extents) = cr.text_extents(&letter_str) {
            (
                (wf - extents.width()) / 2.0 - extents.x_bearing(),
                (hf - extents.height()) / 2.0 - extents.y_bearing(),
            )
        } else {
            (wf * 0.15, hf * 0.75)
        };
        cr.move_to(tx, ty);
        let _ = cr.show_text(&letter_str);
    });

    area
}

pub fn result_row(action: &Action) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 12);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(14);
    layout.set_margin_end(14);

    let icon = letter_icon(&action.title, 28);

    let text = GtkBox::new(Orientation::Horizontal, 8);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&action.title));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(false);
    title.set_margin_top(1);
    title.set_margin_bottom(1);

    let subtitle = Label::new(Some(&action.subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);
    subtitle.set_margin_top(1);
    subtitle.set_margin_bottom(1);

    text.append(&title);
    text.append(&subtitle);

    let category = Label::new(Some(&action.category));
    category.add_css_class("result-badge");
    category.set_xalign(0.5);
    category.set_valign(gtk::Align::Center);
    category.set_margin_top(1);
    category.set_margin_bottom(1);

    layout.append(&icon);
    layout.append(&text);
    layout.append(&category);
    row.set_child(Some(&layout));
    row
}

/// Metric card: small uppercase label → large value + unit → progress bar.
/// Layout matches design: CPU\n23 %\n[====...]
pub fn metric_card(title: &str, _icon_name: &str) -> (GtkBox, Label, Label, ProgressBar) {
    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("metric-card");
    card.set_hexpand(true);

    // Title row (uppercase label)
    let title_label = Label::new(Some(title));
    title_label.add_css_class("metric-label");
    title_label.set_xalign(0.0);
    card.append(&title_label);

    // Value row: big number + small unit
    let value_row = GtkBox::new(Orientation::Horizontal, 3);
    value_row.set_valign(gtk::Align::Baseline);

    let value_label = Label::new(Some("—"));
    value_label.add_css_class("metric-value");
    value_label.set_xalign(0.0);
    value_row.append(&value_label);

    let subtitle_label = Label::new(None);
    subtitle_label.add_css_class("metric-unit");
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_valign(gtk::Align::End);
    subtitle_label.set_margin_bottom(2);
    value_row.append(&subtitle_label);

    card.append(&value_row);

    let bar = ProgressBar::new();
    bar.add_css_class("dashboard-metric-bar");
    card.append(&bar);

    (card, value_label, subtitle_label, bar)
}

/// Control card: icon box + label header → value (bold) → sub text → action buttons.
/// Layout matches: [icon] NETWORK / Connected / wlo1 · 94 Mbps
pub fn control_card(title: &str, icon_name: &str) -> (GtkBox, Label, GtkBox) {
    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("control-card");
    card.set_hexpand(true);

    // Header: icon box + label
    let header = GtkBox::new(Orientation::Horizontal, 8);

    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_width_request(26);
    icon_box.set_height_request(26);
    icon_box.add_css_class("control-card-icon");
    let icon = super::icons::fa_icon(icon_name, 14);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_hexpand(true);
    icon.set_vexpand(true);
    icon_box.append(&icon);

    let title_label = Label::new(Some(title));
    title_label.add_css_class("control-card-heading");
    title_label.set_xalign(0.0);
    title_label.set_valign(gtk::Align::Center);

    header.append(&icon_box);
    header.append(&title_label);
    card.append(&header);

    // Value (bold, primary)
    let state_label = Label::new(None);
    state_label.add_css_class("control-card-value");
    state_label.set_xalign(0.0);
    state_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    card.append(&state_label);

    // Actions row (sub text or buttons)
    let actions_row = GtkBox::new(Orientation::Horizontal, 4);
    actions_row.add_css_class("control-card-actions");
    card.append(&actions_row);

    (card, state_label, actions_row)
}

pub fn section_header(title: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("section-header-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let label = Label::new(Some(title));
    label.add_css_class("section-header");
    label.set_xalign(0.0);
    label.set_margin_top(8);
    label.set_margin_bottom(4);
    label.set_margin_start(16);
    label.set_margin_end(16);
    row.set_child(Some(&label));
    row
}

pub fn secondary_action_row(icon_name: &str, title: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(10);
    layout.set_margin_bottom(10);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = super::icons::fa_icon(icon_name, 20);

    let label = Label::new(Some(title));
    label.add_css_class("result-title");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_margin_top(1);
    label.set_margin_bottom(1);

    layout.append(&icon);
    layout.append(&label);
    row.set_child(Some(&layout));
    row
}
