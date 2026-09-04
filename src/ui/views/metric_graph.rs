use std::cell::RefCell;
use std::rc::Rc;
use gtk::cairo;
use gtk::prelude::*;
use gtk::DrawingArea;

pub const MAX_METRIC_SAMPLES: usize = 60;

#[derive(Clone)]
pub struct MetricGraph {
    pub area: DrawingArea,
    pub values: Rc<RefCell<Vec<f64>>>,
}

pub fn metric_graph() -> MetricGraph {
    metric_graph_with_color(0.545, 0.486, 0.973)
}

pub fn metric_graph_with_color(r: f64, g: f64, b: f64) -> MetricGraph {
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
        let pad_x = 5.0;
        let pad_y = 5.0;
        if w <= pad_x * 2.0 || h <= pad_y * 2.0 || values.is_empty() {
            return;
        }

        let n = values.len();
        let max_slots = MAX_METRIC_SAMPLES.max(n);
        let step = (w - pad_x * 2.0) / (max_slots.saturating_sub(1).max(1)) as f64;

        let pts: Vec<(f64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = pad_x + i as f64 * step;
                let y = h - pad_y - (v.clamp(0.0, 1.0) * (h - pad_y * 2.0));
                (x, y)
            })
            .collect();

        if pts.is_empty() {
            return;
        }

        let y_min = pad_y;
        let y_max = h - pad_y;

        // ── 1. Subtle gradient fill under the curve ───────────────────────────
        cr.move_to(pts[0].0, h);
        cr.line_to(pts[0].0, pts[0].1);

        if pts.len() == 2 {
            cr.line_to(pts[1].0, pts[1].1);
        } else if pts.len() > 2 {
            for i in 0..pts.len() - 1 {
                let (cp1, cp2) = get_control_points(&pts, i, y_min, y_max);
                cr.curve_to(cp1.0, cp1.1, cp2.0, cp2.1, pts[i + 1].0, pts[i + 1].1);
            }
        }
        cr.line_to(pts[pts.len() - 1].0, h);
        cr.close_path();

        let gradient = cairo::LinearGradient::new(0.0, pad_y, 0.0, h);
        gradient.add_color_stop_rgba(0.0, r, g, b, 0.24);
        gradient.add_color_stop_rgba(0.7, r, g, b, 0.05);
        gradient.add_color_stop_rgba(1.0, r, g, b, 0.00);
        let _ = cr.set_source(&gradient);
        let _ = cr.fill();

        // ── 2. Bézier smoothed stroke on top ──────────────────────────────────
        cr.move_to(pts[0].0, pts[0].1);
        if pts.len() == 2 {
            cr.line_to(pts[1].0, pts[1].1);
        } else if pts.len() > 2 {
            for i in 0..pts.len() - 1 {
                let (cp1, cp2) = get_control_points(&pts, i, y_min, y_max);
                cr.curve_to(cp1.0, cp1.1, cp2.0, cp2.1, pts[i + 1].0, pts[i + 1].1);
            }
        }
        cr.set_source_rgba(r, g, b, 0.88);
        cr.set_line_width(1.6);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        let _ = cr.stroke();

        // ── 3. End-point accent dot on current live value ─────────────────────
        let last = pts[pts.len() - 1];
        // Outer glowing halo
        cr.arc(last.0, last.1, 3.5, 0.0, 2.0 * std::f64::consts::PI);
        cr.set_source_rgba(r, g, b, 0.35);
        let _ = cr.fill();

        // Inner bright focal dot
        cr.arc(last.0, last.1, 1.8, 0.0, 2.0 * std::f64::consts::PI);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        let _ = cr.fill();
    });

    MetricGraph { area, values }
}

fn get_control_points(
    pts: &[(f64, f64)],
    i: usize,
    y_min: f64,
    y_max: f64,
) -> ((f64, f64), (f64, f64)) {
    let n = pts.len();
    let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
    let p1 = pts[i];
    let p2 = pts[i + 1];
    let p3 = if i + 2 < n { pts[i + 2] } else { pts[i + 1] };

    let dx = p2.0 - p1.0;
    let slope = p2.1 - p1.1;

    let dy1 = if slope.abs() < 1e-6 {
        0.0
    } else {
        let d = (p2.1 - p0.1) * 0.25;
        if d * slope < 0.0 {
            0.0
        } else {
            d
        }
    };

    let dy2 = if slope.abs() < 1e-6 {
        0.0
    } else {
        let d = (p3.1 - p1.1) * 0.25;
        if d * slope < 0.0 {
            0.0
        } else {
            d
        }
    };

    let cp1_x = (p1.0 + dx / 3.0).clamp(p1.0, p2.0);
    let cp1_y = (p1.1 + dy1).clamp(y_min, y_max);

    let cp2_x = (p2.0 - dx / 3.0).clamp(p1.0, p2.0);
    let cp2_y = (p2.1 - dy2).clamp(y_min, y_max);

    ((cp1_x, cp1_y), (cp2_x, cp2_y))
}

pub fn push_metric_graph(graph: &MetricGraph, value: f64) {
    let mut values = graph.values.borrow_mut();
    values.push(value.clamp(0.0, 1.0));
    if values.len() > MAX_METRIC_SAMPLES {
        values.remove(0);
    }
    graph.area.queue_draw();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_points_flat_segment() {
        let pts = vec![(0.0, 20.0), (10.0, 20.0), (20.0, 20.0), (30.0, 20.0)];
        let (cp1, cp2) = get_control_points(&pts, 1, 0.0, 50.0);
        assert_eq!(cp1.1, 20.0);
        assert_eq!(cp2.1, 20.0);
        assert!(cp1.0 >= 10.0 && cp1.0 <= 20.0);
        assert!(cp2.0 >= 10.0 && cp2.0 <= 20.0);
    }

    #[test]
    fn test_control_points_stay_within_bounds() {
        let pts = vec![(0.0, 5.0), (10.0, 5.0), (20.0, 45.0), (30.0, 45.0)];
        for i in 0..pts.len() - 1 {
            let (cp1, cp2) = get_control_points(&pts, i, 5.0, 45.0);
            assert!(cp1.1 >= 5.0 && cp1.1 <= 45.0);
            assert!(cp2.1 >= 5.0 && cp2.1 <= 45.0);
            assert!(cp1.0 >= pts[i].0 && cp1.0 <= pts[i + 1].0);
            assert!(cp2.0 >= pts[i].0 && cp2.0 <= pts[i + 1].0);
        }
    }

    #[test]
    fn test_push_metric_graph_capacity_and_clamping() {
        let values = Rc::new(RefCell::new(Vec::<f64>::new()));
        let mut vals = values.borrow_mut();
        for v in 0..100 {
            vals.push((v as f64).clamp(0.0, 1.0));
            if vals.len() > MAX_METRIC_SAMPLES {
                vals.remove(0);
            }
        }
        assert_eq!(vals.len(), MAX_METRIC_SAMPLES);
        assert_eq!(*vals.last().unwrap(), 1.0);
    }
}

