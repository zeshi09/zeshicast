use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Grid, Label, Orientation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridSnapTarget {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Fullscreen,
}

#[derive(Clone)]
pub struct WindowGridView {
    pub root: GtkBox,
    pub preview: DrawingArea,
    pub selected_target: Rc<RefCell<Option<GridSnapTarget>>>,
    pub status_label: Label,
}

pub fn window_grid_view() -> WindowGridView {
    let root = crate::ui::panel_root(12, 16);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Window Grid & Tile Manager");
    root.append(&header);

    let desc = Label::new(Some(
        "Click a sector or press a shortcut to snap and arrange the active window across your workspace.",
    ));
    desc.add_css_class("dashboard-subtitle");
    desc.set_xalign(0.0);
    root.append(&desc);

    let selected_target: Rc<RefCell<Option<GridSnapTarget>>> =
        Rc::new(RefCell::new(Some(GridSnapTarget::LeftHalf)));

    // Interactive Cairo Screen Preview Area
    let preview = DrawingArea::new();
    preview.set_content_width(360);
    preview.set_content_height(180);
    preview.set_halign(gtk::Align::Center);
    preview.set_margin_top(8);
    preview.set_margin_bottom(12);
    preview.add_css_class("metric-graph-area");

    let sel_for_draw = selected_target.clone();
    preview.set_draw_func(move |_, cr, width, height| {
        let w = width as f64;
        let h = height as f64;

        // Monitor background
        cr.set_source_rgba(0.08, 0.09, 0.12, 0.95);
        let radius = 8.0;
        let _ = cr.arc(radius, radius, radius, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        let _ = cr.arc(w - radius, radius, radius, 1.5 * std::f64::consts::PI, 2.0 * std::f64::consts::PI);
        let _ = cr.arc(w - radius, h - radius, radius, 0.0, 0.5 * std::f64::consts::PI);
        let _ = cr.arc(radius, h - radius, radius, 0.5 * std::f64::consts::PI, std::f64::consts::PI);
        cr.close_path();
        let _ = cr.fill();

        // Monitor border
        cr.set_source_rgba(0.2, 0.22, 0.3, 0.8);
        cr.set_line_width(1.5);
        let _ = cr.arc(radius, radius, radius, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        let _ = cr.arc(w - radius, radius, radius, 1.5 * std::f64::consts::PI, 2.0 * std::f64::consts::PI);
        let _ = cr.arc(w - radius, h - radius, radius, 0.0, 0.5 * std::f64::consts::PI);
        let _ = cr.arc(radius, h - radius, radius, 0.5 * std::f64::consts::PI, std::f64::consts::PI);
        cr.close_path();
        let _ = cr.stroke();

        // Screen Grid Lines
        cr.set_source_rgba(0.18, 0.2, 0.26, 0.5);
        cr.set_line_width(1.0);
        // Vertical center line
        cr.move_to(w / 2.0, 4.0);
        cr.line_to(w / 2.0, h - 4.0);
        let _ = cr.stroke();
        // Horizontal center line
        cr.move_to(4.0, h / 2.0);
        cr.line_to(w - 4.0, h / 2.0);
        let _ = cr.stroke();

        // Draw selected highlight bounding box
        if let Some(target) = *sel_for_draw.borrow() {
            let (rx, ry, rw, rh) = match target {
                GridSnapTarget::LeftHalf => (6.0, 6.0, (w / 2.0) - 8.0, h - 12.0),
                GridSnapTarget::RightHalf => ((w / 2.0) + 2.0, 6.0, (w / 2.0) - 8.0, h - 12.0),
                GridSnapTarget::TopHalf => (6.0, 6.0, w - 12.0, (h / 2.0) - 8.0),
                GridSnapTarget::BottomHalf => (6.0, (h / 2.0) + 2.0, w - 12.0, (h / 2.0) - 8.0),
                GridSnapTarget::TopLeft => (6.0, 6.0, (w / 2.0) - 8.0, (h / 2.0) - 8.0),
                GridSnapTarget::TopRight => ((w / 2.0) + 2.0, 6.0, (w / 2.0) - 8.0, (h / 2.0) - 8.0),
                GridSnapTarget::BottomLeft => (6.0, (h / 2.0) + 2.0, (w / 2.0) - 8.0, (h / 2.0) - 8.0),
                GridSnapTarget::BottomRight => {
                    ((w / 2.0) + 2.0, (h / 2.0) + 2.0, (w / 2.0) - 8.0, (h / 2.0) - 8.0)
                }
                GridSnapTarget::Center => (w * 0.18, h * 0.15, w * 0.64, h * 0.7),
                GridSnapTarget::Fullscreen => (6.0, 6.0, w - 12.0, h - 12.0),
            };

            // Accent Fill
            cr.set_source_rgba(0.95, 0.4, 0.4, 0.35);
            let _ = cr.rectangle(rx, ry, rw, rh);
            let _ = cr.fill();

            // Accent Stroke
            cr.set_source_rgba(1.0, 0.45, 0.45, 0.9);
            cr.set_line_width(2.0);
            let _ = cr.rectangle(rx, ry, rw, rh);
            let _ = cr.stroke();
        }
    });

    root.append(&preview);

    let status_label = Label::new(Some("Select a preset to execute layout snapping"));
    status_label.add_css_class("dashboard-subtitle");
    status_label.set_margin_bottom(8);
    root.append(&status_label);

    // Preset Grid Buttons
    let grid = Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    grid.set_halign(gtk::Align::Center);
    grid.set_margin_bottom(12);

    let presets = [
        ("Left Half  [H]", "go-previous-symbolic", GridSnapTarget::LeftHalf, 0, 0),
        ("Right Half  [L]", "go-next-symbolic", GridSnapTarget::RightHalf, 1, 0),
        ("Top Half  [K]", "go-up-symbolic", GridSnapTarget::TopHalf, 2, 0),
        ("Bottom Half  [J]", "go-down-symbolic", GridSnapTarget::BottomHalf, 3, 0),
        ("Top Left", "pan-start-symbolic", GridSnapTarget::TopLeft, 0, 1),
        ("Top Right", "pan-end-symbolic", GridSnapTarget::TopRight, 1, 1),
        ("Bottom Left", "pan-down-symbolic", GridSnapTarget::BottomLeft, 2, 1),
        ("Bottom Right", "pan-down-symbolic", GridSnapTarget::BottomRight, 3, 1),
        ("Center 70%  [C]", "zoom-fit-best-symbolic", GridSnapTarget::Center, 0, 2),
        ("Fullscreen  [F]", "view-fullscreen-symbolic", GridSnapTarget::Fullscreen, 1, 2),
    ];

    for (label_text, icon_name, target, col, row) in presets {
        let btn = Button::builder()
            .label(label_text)
            .icon_name(icon_name)
            .tooltip_text(format!("Snap Window: {label_text}"))
            .build();
        btn.add_css_class("dashboard-button");

        let sel = selected_target.clone();
        let p = preview.clone();
        let stat = status_label.clone();
        btn.connect_clicked(move |_| {
            *sel.borrow_mut() = Some(target);
            p.queue_draw();
            stat.set_text(&format!("Applied: {label_text}"));
            execute_grid_snap(target);
        });

        grid.attach(&btn, col, row, 1, 1);
    }

    root.append(&grid);

    // Fine-tuning Row (Column Expand/Shrink)
    let fine_tuning_box = GtkBox::new(Orientation::Horizontal, 8);
    fine_tuning_box.set_halign(gtk::Align::Center);

    let expand_btn = Button::builder()
        .label("Expand  [+]")
        .icon_name("zoom-in-symbolic")
        .build();
    expand_btn.add_css_class("dashboard-button");
    expand_btn.connect_clicked(|_| {
        execute_column_resize(10);
    });

    let shrink_btn = Button::builder()
        .label("Shrink  [-]")
        .icon_name("zoom-out-symbolic")
        .build();
    shrink_btn.add_css_class("dashboard-button");
    shrink_btn.connect_clicked(|_| {
        execute_column_resize(-10);
    });

    let close_win_btn = Button::builder()
        .label("Close Window")
        .icon_name("window-close-symbolic")
        .build();
    close_win_btn.add_css_class("dashboard-button");
    close_win_btn.connect_clicked(|_| {
        execute_close_focused_window();
    });

    fine_tuning_box.append(&expand_btn);
    fine_tuning_box.append(&shrink_btn);
    fine_tuning_box.append(&close_win_btn);

    root.append(&fine_tuning_box);

    WindowGridView {
        root,
        preview,
        selected_target,
        status_label,
    }
}

pub fn execute_grid_snap(target: GridSnapTarget) {
    // Detect active compositor: Niri, Hyprland, Sway
    if is_command_available("niri") {
        match target {
            GridSnapTarget::LeftHalf => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "consume-or-expel-window-left"])
                    .spawn();
            }
            GridSnapTarget::RightHalf => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "consume-or-expel-window-right"])
                    .spawn();
            }
            GridSnapTarget::TopHalf | GridSnapTarget::BottomHalf => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "set-window-height", "50%"])
                    .spawn();
            }
            GridSnapTarget::Fullscreen => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "fullscreen-window"])
                    .spawn();
            }
            GridSnapTarget::Center => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "center-column"])
                    .spawn();
            }
            _ => {
                let _ = Command::new("niri")
                    .args(["msg", "action", "center-column"])
                    .spawn();
            }
        }
        return;
    }

    if is_command_available("hyprctl") {
        match target {
            GridSnapTarget::LeftHalf => {
                let _ = Command::new("hyprctl").args(["dispatch", "movewindow", "l"]).spawn();
            }
            GridSnapTarget::RightHalf => {
                let _ = Command::new("hyprctl").args(["dispatch", "movewindow", "r"]).spawn();
            }
            GridSnapTarget::TopHalf => {
                let _ = Command::new("hyprctl").args(["dispatch", "movewindow", "u"]).spawn();
            }
            GridSnapTarget::BottomHalf => {
                let _ = Command::new("hyprctl").args(["dispatch", "movewindow", "d"]).spawn();
            }
            GridSnapTarget::Fullscreen => {
                let _ = Command::new("hyprctl").args(["dispatch", "fullscreen", "1"]).spawn();
            }
            GridSnapTarget::Center => {
                let _ = Command::new("hyprctl").args(["dispatch", "togglefloating"]).spawn();
            }
            _ => {
                let _ = Command::new("hyprctl").args(["dispatch", "togglesplit"]).spawn();
            }
        }
        return;
    }

    if is_command_available("swaymsg") {
        match target {
            GridSnapTarget::LeftHalf => {
                let _ = Command::new("swaymsg").args(["move", "left"]).spawn();
            }
            GridSnapTarget::RightHalf => {
                let _ = Command::new("swaymsg").args(["move", "right"]).spawn();
            }
            GridSnapTarget::TopHalf => {
                let _ = Command::new("swaymsg").args(["move", "up"]).spawn();
            }
            GridSnapTarget::BottomHalf => {
                let _ = Command::new("swaymsg").args(["move", "down"]).spawn();
            }
            GridSnapTarget::Fullscreen => {
                let _ = Command::new("swaymsg").args(["fullscreen", "toggle"]).spawn();
            }
            GridSnapTarget::Center => {
                let _ = Command::new("swaymsg").args(["floating", "toggle"]).spawn();
            }
            _ => {
                let _ = Command::new("swaymsg").args(["split", "toggle"]).spawn();
            }
        }
    }
}

pub fn execute_column_resize(delta_percent: i32) {
    if is_command_available("niri") {
        let sign = if delta_percent >= 0 { "+" } else { "" };
        let _ = Command::new("niri")
            .args(["msg", "action", "set-column-width", &format!("{sign}{delta_percent}%")])
            .spawn();
    } else if is_command_available("hyprctl") {
        let ratio = if delta_percent >= 0 { "0.05" } else { "-0.05" };
        let _ = Command::new("hyprctl")
            .args(["dispatch", "splitratio", ratio])
            .spawn();
    } else if is_command_available("swaymsg") {
        let action = if delta_percent >= 0 { "grow" } else { "shrink" };
        let _ = Command::new("swaymsg")
            .args(["resize", action, "width", "50 px"])
            .spawn();
    }
}

pub fn execute_close_focused_window() {
    if is_command_available("niri") {
        let _ = Command::new("niri").args(["msg", "action", "close-window"]).spawn();
    } else if is_command_available("hyprctl") {
        let _ = Command::new("hyprctl").args(["dispatch", "killactive"]).spawn();
    } else if is_command_available("swaymsg") {
        let _ = Command::new("swaymsg").args(["kill"]).spawn();
    }
}

fn is_command_available(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|p| {
                let full = p.join(cmd);
                if full.is_file() { Some(full) } else { None }
            })
        })
        .is_some()
}
