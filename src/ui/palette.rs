//! Colours for the custom-drawn dial and chart. The CSS in style.css mirrors these values.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub f64, pub f64, pub f64);

impl Rgb {
    const fn hex(v: u32) -> Self {
        Self(
            ((v >> 16) & 0xff) as f64 / 255.0,
            ((v >> 8) & 0xff) as f64 / 255.0,
            (v & 0xff) as f64 / 255.0,
        )
    }

    fn mix(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self(
            self.0 + (other.0 - self.0) * t,
            self.1 + (other.1 - self.1) * t,
            self.2 + (other.2 - self.2) * t,
        )
    }
}

/// The dial covers 20–100 °C.
pub const SCALE_MIN: f64 = 20.0;
pub const SCALE_MAX: f64 = 100.0;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub ink: Rgb,
    pub muted: Rgb,
    pub track: Rgb,
    pub surface: Rgb,
    heat: [Rgb; 4],
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                ink: Rgb::hex(0xeeece8),
                muted: Rgb::hex(0x8a8c93),
                track: Rgb::hex(0x24262b),
                surface: Rgb::hex(0x0b0c0e),
                heat: [Rgb::hex(0x58b4ff), Rgb::hex(0x7de3c3), Rgb::hex(0xffb547), Rgb::hex(0xff5a4a)],
            }
        } else {
            Self {
                dark,
                ink: Rgb::hex(0x1a1b1e),
                muted: Rgb::hex(0x6d6f76),
                track: Rgb::hex(0xe4e2dd),
                surface: Rgb::hex(0xf6f5f2),
                heat: [Rgb::hex(0x1f7fd6), Rgb::hex(0x16a383), Rgb::hex(0xd98500), Rgb::hex(0xdc3a2b)],
            }
        }
    }

    /// Colour for a temperature in °C: blue when cool, mint at idle-warm, amber under load, red when hot.
    pub fn heat(&self, temp_c: f64) -> Rgb {
        const STOPS: [f64; 4] = [30.0, 50.0, 72.0, 90.0];
        let [c0, c1, c2, c3] = self.heat;
        let t = temp_c;
        if t <= STOPS[0] {
            c0
        } else if t <= STOPS[1] {
            c0.mix(c1, (t - STOPS[0]) / (STOPS[1] - STOPS[0]))
        } else if t <= STOPS[2] {
            c1.mix(c2, (t - STOPS[1]) / (STOPS[2] - STOPS[1]))
        } else if t <= STOPS[3] {
            c2.mix(c3, (t - STOPS[2]) / (STOPS[3] - STOPS[2]))
        } else {
            c3
        }
    }
}

/// A word for the current temperature, used under the dial and for CSS classes.
pub fn mood(temp_c: f64) -> (&'static str, &'static str) {
    match temp_c {
        t if t < 40.0 => ("Cool", "cool"),
        t if t < 55.0 => ("Comfortable", "mint"),
        t if t < 72.0 => ("Warm", "warm"),
        t if t < 88.0 => ("Hot", "hot"),
        _ => ("Very hot", "hot"),
    }
}

pub fn to_unit(temp_c: f64, fahrenheit: bool) -> f64 {
    if fahrenheit { temp_c * 9.0 / 5.0 + 32.0 } else { temp_c }
}

pub fn set_source(cr: &gtk::cairo::Context, c: Rgb, alpha: f64) {
    cr.set_source_rgba(c.0, c.1, c.2, alpha);
}
