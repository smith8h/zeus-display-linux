//! Two-minute history: load as quiet columns, temperature as a heat-graded line on top.

use std::collections::VecDeque;

use gtk::cairo::{Context, LineCap, LineJoin, LinearGradient};
use gtk::pango;

use super::palette::{Palette, set_source, to_unit};

pub const SAMPLES: usize = 120;

#[derive(Clone, Copy)]
pub struct Sample {
    pub temp_c: Option<f64>,
    pub load: f64,
}

pub struct ChartState<'a> {
    pub history: &'a VecDeque<Sample>,
    pub hover_x: Option<f64>,
    pub fahrenheit: bool,
}

/// Temperature range shown, padded and at least 12 °C tall so idle noise stays flat.
fn temp_range(history: &VecDeque<Sample>) -> (f64, f64) {
    let temps = history.iter().filter_map(|s| s.temp_c);
    let (lo, hi) = temps.fold((f64::MAX, f64::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));
    if lo > hi {
        return (30.0, 60.0);
    }
    let mid = (lo + hi) / 2.0;
    let half = ((hi - lo) / 2.0 + 3.0).max(6.0);
    (mid - half, mid + half)
}

pub fn draw(cr: &Context, w: f64, h: f64, s: &ChartState, pal: &Palette) {
    let top = 22.0;
    let bottom = h - 4.0;
    let plot_h = bottom - top;
    let slot = w / SAMPLES as f64;
    let n = s.history.len();
    let x_of = |i: usize| w - (n - i) as f64 * slot + slot / 2.0;

    // Baseline grid.
    set_source(cr, pal.muted, 0.14);
    cr.set_line_width(1.0);
    for k in 0..3 {
        let y = (top + plot_h * k as f64 / 2.0).round() + 0.5;
        cr.move_to(0.0, y);
        cr.line_to(w, y);
    }
    let _ = cr.stroke();

    // Load columns.
    for (i, sample) in s.history.iter().enumerate() {
        let bh = (sample.load / 100.0).clamp(0.0, 1.0) * plot_h;
        if bh < 0.5 {
            continue;
        }
        let x = x_of(i) - slot / 2.0 + 0.75;
        set_source(cr, pal.ink, if pal.dark { 0.11 } else { 0.09 });
        cr.rectangle(x, bottom - bh, (slot - 1.5).max(1.0), bh);
        let _ = cr.fill();
    }

    // Temperature line.
    let (lo, hi) = temp_range(s.history);
    let y_of = |t: f64| bottom - (t - lo) / (hi - lo) * plot_h;
    let points: Vec<(f64, f64)> = s
        .history
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.temp_c.map(|t| (x_of(i), y_of(t))))
        .collect();

    if points.len() >= 2 {
        let grad = LinearGradient::new(0.0, top, 0.0, bottom);
        for k in 0..=4 {
            let f = k as f64 / 4.0;
            let c = pal.heat(hi - (hi - lo) * f);
            grad.add_color_stop_rgb(f, c.0, c.1, c.2);
        }

        trace_smooth(cr, &points);
        cr.line_to(points.last().unwrap().0, bottom);
        cr.line_to(points[0].0, bottom);
        cr.close_path();
        let fill = LinearGradient::new(0.0, top, 0.0, bottom);
        let c = pal.heat(hi);
        fill.add_color_stop_rgba(0.0, c.0, c.1, c.2, if pal.dark { 0.20 } else { 0.14 });
        fill.add_color_stop_rgba(1.0, c.0, c.1, c.2, 0.0);
        let _ = cr.set_source(&fill);
        let _ = cr.fill();

        trace_smooth(cr, &points);
        let _ = cr.set_source(&grad);
        cr.set_line_width(2.2);
        cr.set_line_cap(LineCap::Round);
        cr.set_line_join(LineJoin::Round);
        let _ = cr.stroke();

        let (lx, ly) = *points.last().unwrap();
        let last_t = s.history.back().and_then(|p| p.temp_c).unwrap_or(lo);
        set_source(cr, pal.heat(last_t), 1.0);
        cr.arc(lx - 1.0, ly, 3.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }

    // Hover readout.
    let Some(hx) = s.hover_x else { return };
    if n == 0 {
        return;
    }
    let i = (((hx - (w - n as f64 * slot)) / slot).floor().max(0.0) as usize).min(n - 1);
    let sample = s.history[i];
    let x = x_of(i).round() + 0.5;
    set_source(cr, pal.ink, 0.35);
    cr.set_line_width(1.0);
    cr.move_to(x, top - 2.0);
    cr.line_to(x, bottom);
    let _ = cr.stroke();
    if let Some(t) = sample.temp_c {
        set_source(cr, pal.surface, 1.0);
        cr.arc(x, y_of(t), 5.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        set_source(cr, pal.heat(t), 1.0);
        cr.arc(x, y_of(t), 3.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }

    let ago = n - 1 - i;
    let unit = if s.fahrenheit { "°F" } else { "°C" };
    let temp_txt = sample
        .temp_c
        .map(|t| format!("{:.1}{unit}", to_unit(t, s.fahrenheit)))
        .unwrap_or_else(|| "—".into());
    let when = if ago == 0 { "now".to_string() } else { format!("{ago}s ago") };
    let text = format!("{temp_txt}  ·  {:.0}% load  ·  {when}", sample.load);
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pango::FontDescription::from_string("Inter Variable 8.5")));
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrFontFeatures::new("tnum 1"));
    layout.set_attributes(Some(&attrs));
    layout.set_text(&text);
    let (tw, _) = layout.pixel_size();
    let tx = (x - tw as f64 / 2.0).clamp(0.0, (w - tw as f64).max(0.0));
    set_source(cr, pal.ink, 0.9);
    cr.move_to(tx, 0.0);
    pangocairo::functions::show_layout(cr, &layout);
}

/// Monotone-ish smoothing: Catmull-Rom converted to cubic Béziers with a gentle tension.
fn trace_smooth(cr: &Context, p: &[(f64, f64)]) {
    cr.move_to(p[0].0, p[0].1);
    let k = 0.18;
    for i in 0..p.len() - 1 {
        let p0 = if i == 0 { p[0] } else { p[i - 1] };
        let p1 = p[i];
        let p2 = p[i + 1];
        let p3 = if i + 2 < p.len() { p[i + 2] } else { p2 };
        let c1 = (p1.0 + (p2.0 - p0.0) * k, p1.1 + (p2.1 - p0.1) * k);
        let c2 = (p2.0 - (p3.0 - p1.0) * k, p2.1 - (p3.1 - p1.1) * k);
        cr.curve_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
    }
}
