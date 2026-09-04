use std::cell::RefCell;
use std::rc::Rc;
use gtk::cairo;
use gtk::prelude::*;
use gtk::DrawingArea;

#[derive(Clone)]
pub struct MetricGraph {
    pub area: DrawingArea,
    pub values: Rc<RefCell<Vec<f64>>>,
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

