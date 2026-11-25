//! The cooler display, written to through its `/dev/hidrawN` node.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use crate::protocol::{PRODUCT_ID, REPORT_LEN, VENDOR_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Frames are reaching the display.
    Live,
    /// No matching USB device is plugged in.
    Missing,
    /// The device is there but this user may not write to it (udev rule not installed).
    NoPermission,
    /// Opened, but the last write failed (unplugged mid-write, or asleep).
    WriteFailed,
}

#[derive(Default)]
pub struct Device {
    file: Option<File>,
}

impl Device {
    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    pub fn open(&mut self) -> Result<(), Link> {
        self.close();
        let node = find_node().ok_or(Link::Missing)?;
        match OpenOptions::new().write(true).open(&node) {
            Ok(file) => {
                self.file = Some(file);
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => Err(Link::NoPermission),
            Err(_) => Err(Link::Missing),
        }
    }

    pub fn send(&mut self, frame: &[u8; REPORT_LEN]) -> bool {
        let Some(file) = self.file.as_mut() else { return false };
        if file.write_all(frame).is_ok() {
            return true;
        }
        self.close();
        false
    }

    pub fn close(&mut self) {
        self.file = None;
    }
}

/// Finds the hidraw node whose HID_ID matches the cooler's vendor/product IDs.
fn find_node() -> Option<PathBuf> {
    let wanted = format!("{VENDOR_ID:08X}:{PRODUCT_ID:08X}");
    let mut entries: Vec<_> = fs::read_dir("/sys/class/hidraw").ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    entries.into_iter().find_map(|entry| {
        let uevent = fs::read_to_string(entry.path().join("device/uevent")).ok()?;
        let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
        id.to_uppercase()
            .ends_with(&wanted)
            .then(|| PathBuf::from("/dev").join(entry.file_name()))
    })
}
