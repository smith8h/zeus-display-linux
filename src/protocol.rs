//! Wire format for the cooler's display, recovered from the vendor app.
//!
//! A HID output report is `[report id = 0]` followed by 64 payload bytes.
//! Payload = `00 01 02` + 24 value slots + zero padding.
//! Slots used: 0/1 = CPU temperature in Celsius (whole, hundredths),
//! 2 = unit flag (0 = °C, 1 = °F, the display converts), 3 = CPU load %.
//! Idle/exit command: payload[1] = 0x0F.

pub const VENDOR_ID: u16 = 0x5131;
pub const PRODUCT_ID: u16 = 0x2007;

pub const REPORT_LEN: usize = 65;

pub fn build_frame(temp_c: f64, load: f64, fahrenheit: bool) -> [u8; REPORT_LEN] {
    let mut frame = [0u8; REPORT_LEN];
    frame[2] = 1;
    frame[3] = 2;
    let v = 4;
    let (whole, hundredths) = split_hundredths(temp_c);
    frame[v] = whole;
    frame[v + 1] = hundredths;
    frame[v + 2] = u8::from(fahrenheit);
    frame[v + 3] = to_byte(load.min(100.0));
    frame
}

pub fn shutdown_frame() -> [u8; REPORT_LEN] {
    let mut frame = [0u8; REPORT_LEN];
    frame[2] = 0x0F;
    frame
}

fn split_hundredths(x: f64) -> (u8, u8) {
    let x = if x.is_finite() && x > 0.0 { x } else { 0.0 };
    let whole = x.floor();
    let hundredths = ((x - whole) * 100.0).round().min(99.0) as u8;
    (to_byte(whole), hundredths)
}

fn to_byte(x: f64) -> u8 {
    if !x.is_finite() {
        return 0;
    }
    x.floor().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_matches_vendor_app() {
        let f = build_frame(46.875, 25.4, false);
        assert_eq!(f.len(), 65);
        assert_eq!(&f[..4], &[0, 0, 1, 2]);
        assert_eq!(f[4], 46);
        assert_eq!(f[5], 88);
        assert_eq!(f[6], 0);
        assert_eq!(f[7], 25);
        assert!(f[8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn fahrenheit_only_sets_the_flag() {
        let f = build_frame(50.0, 10.0, true);
        assert_eq!(f[4], 50);
        assert_eq!(f[6], 1);
    }

    #[test]
    fn bad_values_are_clamped() {
        let f = build_frame(f64::NAN, 140.0, false);
        assert_eq!((f[4], f[5], f[7]), (0, 0, 100));
        let f = build_frame(-5.0, -1.0, false);
        assert_eq!((f[4], f[7]), (0, 0));
        let f = build_frame(42.999, 0.0, false);
        assert_eq!((f[4], f[5]), (42, 99));
    }

    #[test]
    fn shutdown_is_the_idle_packet() {
        let f = shutdown_frame();
        assert_eq!(f[2], 0x0F);
        assert_eq!(f.iter().filter(|&&b| b != 0).count(), 1);
    }
}
