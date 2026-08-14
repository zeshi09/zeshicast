use std::cell::RefCell;
use std::rc::Rc;

use gtk::cairo;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Label, Orientation};

use crate::services::compositor::{WindowSnapPosition, snap_window};

#[derive(Clone)]
pub struct WindowGridView {
    pub root: GtkBox,
    pub drawing_area: DrawingArea,
    pub status_label: Label,
    pub current_position: Rc<RefCell<WindowSnapPosition>>,
}

pub fn window_grid_view() -> WindowGridView {
    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_vexpand(true);
    root.set_hexpand(true);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_margin_top(12);
    root.set_margin_bottom(12);

    let title_box = GtkBox::new(Orientation::Horizontal, 8);
    let title = Label::new(Some("Window Grid Overlay"));
    title.add_css_class("action-panel-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let status_label = Label::new(Some("Select window tile region"));
    status_label.add_css_class("result-subtitle");
    status_label.set_xalign(1.0);

    title_box.append(&title);
    title_box.append(&status_label);
    root.append(&title_box);

    let current_position = Rc::new(RefCell::new(WindowSnapPosition::LeftHalf));

    let drawing_area = DrawingArea::builder()
        .content_width(520)
        .content_height(280)
        .hexpand(true)
        .vexpand(true)
        .build();

    let cur_pos_draw = Rc::clone(&current_position);
    drawing_area.set_draw_func(move |_, cr, width, height| {
        draw_grid(cr, width as f64, height as f64, *cur_pos_draw.borrow());
    });

    root.append(&drawing_area);

    // Keyboard Shortcuts / Action bar
    let hints_box = GtkBox::new(Orientation::Vertical, 6);
    hints_box.add_css_class("grid-hints-box");

    let row1 = GtkBox::new(Orientation::Horizontal, 8);
    row1.append(&grid_hint_btn("H", "Left Half", WindowSnapPosition::LeftHalf, &current_position, &drawing_area, &status_label));
    row1.append(&grid_hint_btn("L", "Right Half", WindowSnapPosition::RightHalf, &current_position, &drawing_area, &status_label));
    row1.append(&grid_hint_btn("K", "Top Half", WindowSnapPosition::TopHalf, &current_position, &drawing_area, &status_label));
    row1.append(&grid_hint_btn("J", "Bottom Half", WindowSnapPosition::BottomHalf, &current_position, &drawing_area, &status_label));
    row1.append(&grid_hint_btn("F", "Fullscreen", WindowSnapPosition::Fullscreen, &current_position, &drawing_area, &status_label));
    row1.append(&grid_hint_btn("C", "Center", WindowSnapPosition::Center, &current_position, &drawing_area, &status_label));

    let row2 = GtkBox::new(Orientation::Horizontal, 8);
    row2.append(&grid_hint_btn("1", "Left ⅓", WindowSnapPosition::FirstThird, &current_position, &drawing_area, &status_label));
    row2.append(&grid_hint_btn("2", "Mid ⅓", WindowSnapPosition::CenterThird, &current_position, &drawing_area, &status_label));
    row2.append(&grid_hint_btn("3", "Right ⅓", WindowSnapPosition::RightThird, &current_position, &drawing_area, &status_label));
    row2.append(&grid_hint_btn("4", "Left ⅔", WindowSnapPosition::LeftTwoThirds, &current_position, &drawing_area, &status_label));
    row2.append(&grid_hint_btn("5", "Right ⅔", WindowSnapPosition::RightTwoThirds, &current_position, &drawing_area, &status_label));

    let row3 = GtkBox::new(Orientation::Horizontal, 8);
    row3.append(&grid_hint_btn("U", "Top-Left ¼", WindowSnapPosition::TopLeftQuarter, &current_position, &drawing_area, &status_label));
    row3.append(&grid_hint_btn("I", "Top-Right ¼", WindowSnapPosition::TopRightQuarter, &current_position, &drawing_area, &status_label));
    row3.append(&grid_hint_btn("N", "Bottom-Left ¼", WindowSnapPosition::BottomLeftQuarter, &current_position, &drawing_area, &status_label));
    row3.append(&grid_hint_btn("M", "Bottom-Right ¼", WindowSnapPosition::BottomRightQuarter, &current_position, &drawing_area, &status_label));

    hints_box.append(&row1);
    hints_box.append(&row2);
    hints_box.append(&row3);
    root.append(&hints_box);

    WindowGridView {
        root,
        drawing_area,
        status_label,
        current_position,
    }
}

fn grid_hint_btn(
    key: &str,
    label: &str,
    pos: WindowSnapPosition,
    current_pos: &Rc<RefCell<WindowSnapPosition>>,
    area: &DrawingArea,
    status: &Label,
) -> Button {
    let btn = Button::builder().hexpand(true).build();
    btn.add_css_class("dashboard-button");

    let h_box = GtkBox::new(Orientation::Horizontal, 6);
    let key_lbl = Label::new(Some(&format!("[{key}]")));
    key_lbl.add_css_class("keycap-badge");

    let text_lbl = Label::new(Some(label));
    text_lbl.add_css_class("result-title");
    text_lbl.set_xalign(0.0);

    h_box.append(&key_lbl);
    h_box.append(&text_lbl);
    btn.set_child(Some(&h_box));

    let cur_pos_c = Rc::clone(current_pos);
    let area_c = area.clone();
    let status_c = status.clone();
    let label_s = label.to_string();

    btn.connect_clicked(move |_| {
        *cur_pos_c.borrow_mut() = pos;
        area_c.queue_draw();
        snap_window(pos);
        status_c.set_text(&format!("Applied: {label_s}"));
    });

    btn
}

fn draw_grid(cr: &cairo::Context, w: f64, h: f64, pos: WindowSnapPosition) {
    let margin = 16.0;
    let mon_w = w - (margin * 2.0);
    let mon_h = h - (margin * 2.0);
    let x0 = margin;
    let y0 = margin;
    let radius = 10.0;

    // Outer screen frame (dark bezel)
    cr.save().ok();
    cr.new_sub_path();
    cr.arc(x0 + mon_w - radius, y0 + radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x0 + mon_w - radius, y0 + mon_h - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x0 + radius, y0 + mon_h - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x0 + radius, y0 + radius, radius, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();

    cr.set_source_rgba(0.10, 0.11, 0.14, 0.95);
    cr.fill_preserve().ok();

    cr.set_source_rgba(0.25, 0.28, 0.35, 0.6);
    cr.set_line_width(1.5);
    cr.stroke().ok();
    cr.restore().ok();

    // Grid guide lines (subtle dashed lines)
    cr.save().ok();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
    cr.set_line_width(1.0);

    // Half vertical
    cr.move_to(x0 + mon_w * 0.5, y0);
    cr.line_to(x0 + mon_w * 0.5, y0 + mon_h);
    // Half horizontal
    cr.move_to(x0, y0 + mon_h * 0.5);
    cr.line_to(x0 + mon_w, y0 + mon_h * 0.5);

    // Thirds vertical
    cr.move_to(x0 + mon_w * (1.0 / 3.0), y0);
    cr.line_to(x0 + mon_w * (1.0 / 3.0), y0 + mon_h);
    cr.move_to(x0 + mon_w * (2.0 / 3.0), y0);
    cr.line_to(x0 + mon_w * (2.0 / 3.0), y0 + mon_h);

    cr.stroke().ok();
    cr.restore().ok();

    // Highlight target snap region
    let (rx, ry, rw, rh) = match pos {
        WindowSnapPosition::LeftHalf => (x0, y0, mon_w * 0.5, mon_h),
        WindowSnapPosition::RightHalf => (x0 + mon_w * 0.5, y0, mon_w * 0.5, mon_h),
        WindowSnapPosition::TopHalf => (x0, y0, mon_w, mon_h * 0.5),
        WindowSnapPosition::BottomHalf => (x0, y0 + mon_h * 0.5, mon_w, mon_h * 0.5),
        WindowSnapPosition::Fullscreen => (x0, y0, mon_w, mon_h),
        WindowSnapPosition::Center => (x0 + mon_w * 0.15, y0 + mon_h * 0.1, mon_w * 0.7, mon_h * 0.8),
        WindowSnapPosition::TopLeftQuarter => (x0, y0, mon_w * 0.5, mon_h * 0.5),
        WindowSnapPosition::TopRightQuarter => (x0 + mon_w * 0.5, y0, mon_w * 0.5, mon_h * 0.5),
        WindowSnapPosition::BottomLeftQuarter => (x0, y0 + mon_h * 0.5, mon_w * 0.5, mon_h * 0.5),
        WindowSnapPosition::BottomRightQuarter => (x0 + mon_w * 0.5, y0 + mon_h * 0.5, mon_w * 0.5, mon_h * 0.5),
        WindowSnapPosition::FirstThird => (x0, y0, mon_w / 3.0, mon_h),
        WindowSnapPosition::CenterThird => (x0 + mon_w / 3.0, y0, mon_w / 3.0, mon_h),
        WindowSnapPosition::RightThird => (x0 + mon_w * (2.0 / 3.0), y0, mon_w / 3.0, mon_h),
        WindowSnapPosition::LeftTwoThirds => (x0, y0, mon_w * (2.0 / 3.0), mon_h),
        WindowSnapPosition::RightTwoThirds => (x0 + mon_w * (1.0 / 3.0), y0, mon_w * (2.0 / 3.0), mon_h),
    };

    let inner_pad = 4.0;
    let hx = rx + inner_pad;
    let hy = ry + inner_pad;
    let hw = rw - (inner_pad * 2.0);
    let hh = rh - (inner_pad * 2.0);
    let snap_radius = 6.0;

    cr.save().ok();
    cr.new_sub_path();
    cr.arc(hx + hw - snap_radius, hy + snap_radius, snap_radius, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(hx + hw - snap_radius, hy + hh - snap_radius, snap_radius, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(hx + snap_radius, hy + hh - snap_radius, snap_radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(hx + snap_radius, hy + snap_radius, snap_radius, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();

    // Accent fill (#8ab4f8 with 35% alpha)
    cr.set_source_rgba(0.54, 0.71, 0.97, 0.35);
    cr.fill_preserve().ok();

    // Glowing border (#8ab4f8 solid)
    cr.set_source_rgba(0.54, 0.71, 0.97, 0.9);
    cr.set_line_width(2.0);
    cr.stroke().ok();
    cr.restore().ok();
}
