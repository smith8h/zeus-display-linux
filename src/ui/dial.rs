//! The temperature dial: a 270° heat-coloured arc with an inner load ring.

use std::f64::consts::PI;

use gtk::cairo::{Context, LineCap, RadialGradient};
use gtk::pango;

use super::palette::{Palette, SCALE_MAX, SCALE_MIN, set_source, to_unit};

const START: f64 = 0.75 * PI; // 135°, bottom-left
const SWEEP: f64 = 1.5 * PI; // 270°, clockwise

fn angle_for(temp_c: f64) -> f64 {
    START + SWEEP * ((temp_c - SCALE_MIN) / (SCALE_MAX - SCALE_MIN)).clamp(0.0, 1.0)
}

pub struct DialState {
    pub temp_c: Option<f64>,
    pub load: f64,
    pub fahrenheit: bool,
}

pub fn draw(cr: &Context, w: f64, h: f64, s: &DialState, pal: &Palette) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = (w.min(h) / 2.0 - 24.0).max(40.0);
    let cy = cy - 6.0;
    let temp = s.temp_c.unwrap_or(SCALE_MIN);
    let heat = pal.heat(temp);

    // Soft heat glow behind the dial.
    let glow = RadialGradient::new(cx, cy, r * 0.15, cx, cy, r * 1.25);
    let a = if pal.dark { 0.20 } else { 0.12 };
    glow.add_color_stop_rgba(0.0, heat.0, heat.1, heat.2, a);
    glow.add_color_stop_rgba(1.0, heat.0, heat.1, heat.2, 0.0);
    let _ = cr.set_source(&glow);
    cr.arc(cx, cy, r * 1.25, 0.0, 2.0 * PI);
    let _ = cr.fill();

    // Scale ticks every 5 °C, longer every 20 °C.
    cr.set_line_cap(LineCap::Round);
    let mut t = SCALE_MIN;
    while t <= SCALE_MAX + 0.01 {
        let ang = angle_for(t);
        let major = (t as i64) % 20 == 0;
        let (r0, r1) = if major { (r + 13.0, r + 20.0) } else { (r + 14.0, r + 17.0) };
        let lit = s.temp_c.is_some_and(|v| v >= t);
        if lit {
            set_source(cr, pal.heat(t), if major { 0.95 } else { 0.75 });
        } else {
            set_source(cr, pal.muted, if major { 0.55 } else { 0.28 });
        }
        cr.set_line_width(if major { 1.6 } else { 1.2 });
        cr.move_to(cx + r0 * ang.cos(), cy + r0 * ang.sin());
        cr.line_to(cx + r1 * ang.cos(), cy + r1 * ang.sin());
        let _ = cr.stroke();
        t += 5.0;
    }

    // Scale labels, centred just below each end of the arc.
    let font = pango::FontDescription::from_string("Inter Variable 8");
    for t in [SCALE_MIN, SCALE_MAX] {
        let ang = angle_for(t);
        let layout = pangocairo::functions::create_layout(cr);
        layout.set_font_description(Some(&font));
        layout.set_text(&format!("{:.0}°", to_unit(t, s.fahrenheit)));
        let (lw, _) = layout.pixel_size();
        set_source(cr, pal.muted, 0.8);
        cr.move_to(cx + r * ang.cos() - lw as f64 / 2.0, cy + r * ang.sin() + 12.0);
        pangocairo::functions::show_layout(cr, &layout);
    }
    cr.new_path();

    // Track.
    let lw = 12.0;
    cr.set_line_width(lw);
    cr.set_line_cap(LineCap::Round);
    set_source(cr, pal.track, 1.0);
    cr.arc(cx, cy, r, START, START + SWEEP);
    let _ = cr.stroke();

    // Value arc, painted in short segments so its colour runs cool → hot along its length.
    if let Some(temp_c) = s.temp_c {
        let end = angle_for(temp_c);
        let step = 0.02;
        cr.set_line_cap(LineCap::Butt);
        let mut a0 = START;
        while a0 < end {
            let a1 = (a0 + step).min(end);
            let t_here = SCALE_MIN + (a0 - START) / SWEEP * (SCALE_MAX - SCALE_MIN);
            set_source(cr, pal.heat(t_here), 1.0);
            cr.new_path();
            cr.arc(cx, cy, r, a0, a1 + 0.004);
            let _ = cr.stroke();
            a0 = a1;
        }
        // Rounded start cap and a bright knob at the tip.
        set_source(cr, pal.heat(SCALE_MIN), 1.0);
        cr.arc(cx + r * START.cos(), cy + r * START.sin(), lw / 2.0, 0.0, 2.0 * PI);
        let _ = cr.fill();
        let (kx, ky) = (cx + r * end.cos(), cy + r * end.sin());
        let knob_glow = RadialGradient::new(kx, ky, 0.0, kx, ky, lw * 1.6);
        knob_glow.add_color_stop_rgba(0.0, heat.0, heat.1, heat.2, 0.55);
        knob_glow.add_color_stop_rgba(1.0, heat.0, heat.1, heat.2, 0.0);
        let _ = cr.set_source(&knob_glow);
        cr.arc(kx, ky, lw * 1.6, 0.0, 2.0 * PI);
        let _ = cr.fill();
        set_source(cr, heat, 1.0);
        cr.arc(kx, ky, lw / 2.0, 0.0, 2.0 * PI);
        let _ = cr.fill();
        set_source(cr, pal.surface, 1.0);
        cr.arc(kx, ky, lw / 2.0 - 3.5, 0.0, 2.0 * PI);
        let _ = cr.fill();
    }

    // Inner load ring.
    let ri = r - 22.0;
    cr.set_line_cap(LineCap::Round);
    cr.set_line_width(3.0);
    set_source(cr, pal.track, 1.0);
    cr.arc(cx, cy, ri, START, START + SWEEP);
    let _ = cr.stroke();
    let load = (s.load / 100.0).clamp(0.0, 1.0);
    if load > 0.004 {
        set_source(cr, pal.ink, if pal.dark { 0.78 } else { 0.72 });
        cr.arc(cx, cy, ri, START, START + SWEEP * load);
        let _ = cr.stroke();
    }
}
