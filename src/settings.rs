//! Saved preferences (~/.config/zeus-display/settings.json) and the login autostart entry.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.zeusdisplay.ZeusDisplay";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub fahrenheit: bool,
    pub theme: Theme,
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

impl Settings {
    fn path() -> PathBuf {
        config_dir().join("zeus-display/settings.json")
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Settings are a convenience; failing to save never interrupts the display loop.
    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}

pub mod autostart {
    use super::*;

    fn path() -> PathBuf {
        config_dir().join("autostart").join(format!("{APP_ID}.desktop"))
    }

    pub fn is_enabled() -> bool {
        path().exists()
    }

    pub fn set_enabled(enabled: bool) -> std::io::Result<()> {
        let path = path();
        if !enabled {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let exe = std::env::current_exe()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            format!(
                "[Desktop Entry]\nType=Application\nName=Zeus Display\n\
                 Comment=Sends CPU temperature and load to the cooler display\n\
                 Exec=\"{}\" --background\nIcon={APP_ID}\nTerminal=false\n\
                 X-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=5\n",
                exe.display()
            ),
        )
    }
}
