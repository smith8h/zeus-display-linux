//! Zeus Display: drives a Zeus cooler's USB display with live CPU temperature and load,
//! with a GTK 4 dashboard and a top-bar indicator.

mod device;
mod protocol;
mod sensors;
mod settings;
mod tray;
mod ui;
mod worker;

use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use settings::{APP_ID, Settings};
use tray::{TrayCommand, ZeusTray};
use ui::{Ui, UiAction};
use worker::Worker;

struct App {
    gtk_app: gtk::Application,
    ui: Rc<Ui>,
    worker: RefCell<Worker>,
    tray: Option<tray::Handle>,
    settings: RefCell<Settings>,
    _hold: gio::ApplicationHoldGuard,
}

impl App {
    fn set_fahrenheit(&self, f: bool) {
        self.settings.borrow_mut().fahrenheit = f;
        self.settings.borrow().save();
        self.worker.borrow().fahrenheit.store(f, std::sync::atomic::Ordering::Relaxed);
        self.ui.set_fahrenheit(f);
        if let Some(t) = &self.tray {
            t.update(|t| t.fahrenheit = f);
        }
    }

    fn quit(&self) {
        self.ui.window.set_visible(false);
        self.worker.borrow_mut().stop(); // sends the idle packet so the display doesn't freeze on a stale value
        if let Some(t) = &self.tray {
            t.shutdown().wait();
        }
        self.gtk_app.quit();
    }
}

/// Icons ship next to the installed binary; during development they come from the source tree.
fn icon_dir() -> PathBuf {
    let installed = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("icons")));
    match installed {
        Some(dir) if dir.is_dir() => dir,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packaging/icons"),
    }
}

fn main() -> glib::ExitCode {
    glib::set_application_name("Zeus Display");
    glib::set_prgname(Some(APP_ID));

    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.add_main_option(
        "background",
        glib::Char::from(b'b'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Start without opening the window (used at login)",
        None,
    );

    let start_hidden = Rc::new(Cell::new(false));
    {
        let start_hidden = start_hidden.clone();
        app.connect_handle_local_options(move |app, opts| {
            let background = opts.contains("background");
            if app.register(None::<&gio::Cancellable>).is_err() {
                return ControlFlow::Break(glib::ExitCode::FAILURE);
            }
            if app.is_remote() && background {
                return ControlFlow::Break(glib::ExitCode::SUCCESS); // already running; nothing to do
            }
            start_hidden.set(background);
            ControlFlow::Continue(())
        });
    }

    let state: Rc<RefCell<Option<Rc<App>>>> = Rc::default();

    {
        let state = state.clone();
        app.connect_startup(move |gtk_app| {
            let settings = Settings::load();
            let icons = icon_dir();
            if let Some(display) = gdk::Display::default() {
                gtk::IconTheme::for_display(&display).add_search_path(&icons);
            }
            gtk::Window::set_default_icon_name(APP_ID);

            let (tx, rx) = async_channel::bounded(8);
            let worker = Worker::start(settings.fahrenheit, tx);

            let (tray_tx, tray_rx) = async_channel::unbounded();
            let tray = tray::spawn(ZeusTray {
                tx: tray_tx,
                summary: "Starting…".into(),
                fahrenheit: settings.fahrenheit,
                icon_dir: icons.to_string_lossy().into_owned(),
            });

            let weak: Rc<RefCell<Option<std::rc::Weak<App>>>> = Rc::default();
            let on_action: Rc<dyn Fn(UiAction)> = {
                let weak = weak.clone();
                Rc::new(move |action| {
                    let Some(app) = weak.borrow().as_ref().and_then(|w| w.upgrade()) else { return };
                    match action {
                        UiAction::SetFahrenheit(f) => app.set_fahrenheit(f),
                        UiAction::SetTheme(t) => {
                            app.settings.borrow_mut().theme = t;
                            app.settings.borrow().save();
                        }
                        UiAction::Quit => app.quit(),
                    }
                })
            };

            let ui = Ui::new(gtk_app, &sensors::cpu_model(), settings.fahrenheit, settings.theme, on_action);
            let app = Rc::new(App {
                gtk_app: gtk_app.clone(),
                ui,
                worker: RefCell::new(worker),
                tray,
                settings: RefCell::new(settings),
                _hold: gtk_app.hold(),
            });
            *weak.borrow_mut() = Some(Rc::downgrade(&app));

            // Sensor updates → window and tray.
            {
                let app = Rc::downgrade(&app);
                glib::spawn_future_local(async move {
                    while let Ok(update) = rx.recv().await {
                        let Some(app) = app.upgrade() else { break };
                        app.ui.push(update);
                        if let Some(t) = &app.tray {
                            let text = ui::summary(&update, app.settings.borrow().fahrenheit);
                            t.update(move |t| {
                                if t.summary != text {
                                    t.summary = text;
                                }
                            });
                        }
                    }
                });
            }

            // Tray menu → app.
            {
                let app = Rc::downgrade(&app);
                glib::spawn_future_local(async move {
                    while let Ok(cmd) = tray_rx.recv().await {
                        let Some(app) = app.upgrade() else { break };
                        match cmd {
                            TrayCommand::Show(token) => app.ui.present(token),
                            TrayCommand::SetFahrenheit(f) => app.set_fahrenheit(f),
                            TrayCommand::Quit => app.quit(),
                        }
                    }
                });
            }

            // Logout / `kill`: blank the display before going.
            {
                let (sig_tx, sig_rx) = async_channel::bounded::<()>(1);
                if let Ok(mut signals) = signal_hook::iterator::Signals::new([
                    signal_hook::consts::SIGTERM,
                    signal_hook::consts::SIGINT,
                    signal_hook::consts::SIGHUP,
                ]) {
                    std::thread::spawn(move || {
                        if signals.forever().next().is_some() {
                            let _ = sig_tx.send_blocking(());
                        }
                    });
                }
                let app = Rc::downgrade(&app);
                glib::spawn_future_local(async move {
                    if sig_rx.recv().await.is_ok() {
                        if let Some(app) = app.upgrade() {
                            app.quit();
                        }
                    }
                });
            }

            *state.borrow_mut() = Some(app);
        });
    }

    {
        let state = state.clone();
        let first = Cell::new(true);
        app.connect_activate(move |_| {
            let Some(app) = state.borrow().clone() else { return };
            if first.replace(false) && start_hidden.get() {
                return;
            }
            if let Some(h) = std::env::var("ZEUS_CAPTURE_HEIGHT").ok().and_then(|h| h.parse().ok()) {
                app.ui.window.set_default_size(440, h);
            }
            app.ui.present(None);

            // Development aid: ZEUS_CAPTURE=out.png saves the window after a few seconds, then quits.
            if let Some(path) = std::env::var_os("ZEUS_CAPTURE") {
                let app = app.clone();
                glib::timeout_add_seconds_local_once(6, move || {
                    let path = PathBuf::from(path);
                    capture(&app.ui.window, &path);
                    // Also the whole scrolled content, below the fold.
                    let content = app.ui.window.child().and_then(|s| s.first_child()).and_then(|v| v.first_child());
                    if let Some(content) = content {
                        capture(&content, &path.with_extension("full.png"));
                    }
                    app.quit();
                });
            }
        });
    }

    app.connect_shutdown(move |_| {
        if let Some(app) = state.borrow_mut().take() {
            app.worker.borrow_mut().stop();
        }
    });

    app.run()
}

fn capture(window: &impl IsA<gtk::Widget>, path: &std::path::Path) {
    let window = window.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
    let Some(node) = snapshot.to_node() else { return };
    let Some(renderer) = window.native().and_then(|n| n.renderer()) else { return };
    let texture = renderer.render_texture(&node, None);
    if let Err(e) = texture.save_to_png(path) {
        eprintln!("zeus-display: capture failed: {e}");
    }
}
