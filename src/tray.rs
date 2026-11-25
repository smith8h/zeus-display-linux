//! Top-bar indicator (StatusNotifierItem; shown on Ubuntu by the AppIndicator extension).

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{MenuItem, ToolTip};

use crate::settings::APP_ID;

pub enum TrayCommand {
    Show(Option<String>),
    SetFahrenheit(bool),
    Quit,
}

pub struct ZeusTray {
    pub tx: async_channel::Sender<TrayCommand>,
    pub summary: String,
    pub fahrenheit: bool,
    pub icon_dir: String,
}

pub type Handle = ksni::blocking::Handle<ZeusTray>;

impl ksni::Tray for ZeusTray {
    fn id(&self) -> String {
        "zeus-display".into()
    }

    fn title(&self) -> String {
        "Zeus Display".into()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_dir.clone()
    }

    fn icon_name(&self) -> String {
        format!("{APP_ID}-symbolic")
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip { title: "Zeus Display".into(), description: self.summary.clone(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send_blocking(TrayCommand::Show(ksni::current_xdg_activation_token()));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: self.summary.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Open Zeus Display".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayCommand::Show(ksni::current_xdg_activation_token()));
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Show Fahrenheit".into(),
                checked: self.fahrenheit,
                activate: Box::new(|t: &mut Self| {
                    t.fahrenheit = !t.fahrenheit;
                    let _ = t.tx.send_blocking(TrayCommand::SetFahrenheit(t.fahrenheit));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn(tray: ZeusTray) -> Option<Handle> {
    match tray.spawn() {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("zeus-display: no tray available ({e}); the app keeps running in the background");
            None
        }
    }
}
