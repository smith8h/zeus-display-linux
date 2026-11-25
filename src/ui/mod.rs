//! The dashboard window.

mod chart;
mod dial;
mod palette;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::device::Link;
use crate::sensors::Reading;
use crate::settings::{Theme, autostart};
use crate::worker::Update;
use chart::{SAMPLES, Sample};
use palette::{Palette, to_unit};

pub const UDEV_COMMAND: &str = "echo 'KERNEL==\"hidraw*\", ATTRS{idVendor}==\"5131\", ATTRS{idProduct}==\"2007\", TAG+=\"uaccess\"' | sudo tee /etc/udev/rules.d/70-zeus-display.rules && sudo udevadm control --reload && sudo udevadm trigger";

pub enum UiAction {
    SetFahrenheit(bool),
    SetTheme(Theme),
    Quit,
}

struct State {
    history: VecDeque<Sample>,
    reading: Reading,
    shown_temp: Option<f64>,
    shown_load: f64,
    fahrenheit: bool,
    theme: Theme,
    system_dark: bool,
    hover_x: Option<f64>,
}

impl State {
    fn dark(&self) -> bool {
        match self.theme {
            Theme::System => self.system_dark,
            Theme::Light => false,
            Theme::Dark => true,
        }
    }
}

pub struct Ui {
    pub window: gtk::ApplicationWindow,
    state: Rc<RefCell<State>>,
    dial: gtk::DrawingArea,
    chart: gtk::DrawingArea,
    hero: Hero,
    load: gtk::Label,
    power: gtk::Label,
    clock: gtk::Label,
    range: gtk::Label,
    pill: gtk::Box,
    pill_label: gtk::Label,
    banner: gtk::Revealer,
    unit_c: gtk::ToggleButton,
    unit_f: gtk::ToggleButton,
    _interface: Option<gio::Settings>,
}

#[derive(Clone)]
struct Hero {
    whole: gtk::Label,
    frac: gtk::Label,
    unit: gtk::Label,
    mood: gtk::Label,
}

impl Hero {
    fn set(&self, temp_c: Option<f64>, fahrenheit: bool) {
        self.unit.set_label(if fahrenheit { "°F" } else { "°C" });
        let Some(t) = temp_c else {
            self.whole.set_label("—");
            self.frac.set_label("");
            self.mood.set_label("No sensor found");
            return;
        };
        let v = to_unit(t, fahrenheit);
        let tenths = (v * 10.0).round() as i64;
        self.whole.set_label(&(tenths / 10).to_string());
        self.frac.set_label(&format!(".{}", (tenths % 10).abs()));
        let (word, class) = palette::mood(t);
        self.mood.set_label(word);
        for c in ["cool", "mint", "warm", "hot"] {
            if c == class {
                self.mood.add_css_class(c);
            } else {
                self.mood.remove_css_class(c);
            }
        }
    }
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn segmented(options: &[&str]) -> (gtk::Box, Vec<gtk::ToggleButton>) {
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bx.add_css_class("segmented");
    bx.set_valign(gtk::Align::Center);
    let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
    for text in options {
        let b = gtk::ToggleButton::with_label(text);
        if let Some(first) = buttons.first() {
            b.set_group(Some(first));
        }
        bx.append(&b);
        buttons.push(b);
    }
    (bx, buttons)
}

fn setting_row(title: &str, detail: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("setting");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let t = label(title, &["setting-title"]);
    t.set_xalign(0.0);
    let d = label(detail, &["setting-detail"]);
    d.set_xalign(0.0);
    text.append(&t);
    text.append(&d);
    row.append(&text);
    row.append(control);
    row
}

fn readout(caption: &str) -> (gtk::Box, gtk::Label) {
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bx.add_css_class("readout");
    bx.append(&label(caption, &["readout-caption"]));
    let value = label("—", &["readout-value"]);
    bx.append(&value);
    (bx, value)
}

impl Ui {
    pub fn new(
        app: &gtk::Application,
        cpu_model: &str,
        fahrenheit: bool,
        theme: Theme,
        on_action: Rc<dyn Fn(UiAction)>,
    ) -> Rc<Self> {
        install_css();

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Zeus Display")
            .default_width(440)
            .build();
        window.add_css_class("zeus");

        // Header: name + CPU on the left, live status on the right.
        let header = gtk::HeaderBar::new();
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
        titles.set_valign(gtk::Align::Center);
        titles.append(&label("Zeus Display", &["app-title"]));
        titles.append(&label(cpu_model, &["app-subtitle"]));
        header.set_title_widget(Some(&titles));
        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        pill.add_css_class("pill");
        pill.set_valign(gtk::Align::Center);
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("dot");
        dot.set_valign(gtk::Align::Center);
        let pill_label = label("Starting", &[]);
        pill.append(&dot);
        pill.append(&pill_label);
        header.pack_start(&pill);
        window.set_titlebar(Some(&header));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add_css_class("content");

        // Permission banner.
        let banner_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        banner_box.add_css_class("banner");
        let b_title = label("The cooler is plugged in, but Linux won't let Zeus write to it", &["banner-title"]);
        b_title.set_wrap(true);
        b_title.set_xalign(0.0);
        let b_text = label(
            "Run this once in a terminal to give your account access. Zeus connects by itself a moment later.",
            &["banner-text"],
        );
        b_text.set_wrap(true);
        b_text.set_xalign(0.0);
        let copy = gtk::Button::with_label("Copy command");
        copy.add_css_class("banner-button");
        copy.set_halign(gtk::Align::Start);
        copy.connect_clicked(|b| {
            b.clipboard().set_text(UDEV_COMMAND);
            b.set_label("Copied");
            let b = b.clone();
            glib::timeout_add_seconds_local_once(2, move || b.set_label("Copy command"));
        });
        banner_box.append(&b_title);
        banner_box.append(&b_text);
        banner_box.append(&copy);
        let banner = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .child(&banner_box)
            .build();
        content.append(&banner);

        // Dial with the number laid over its centre.
        let dial = gtk::DrawingArea::builder().content_width(290).content_height(290).hexpand(true).build();
        let overlay = gtk::Overlay::new();
        overlay.add_css_class("dial");
        overlay.set_child(Some(&dial));
        let centre = gtk::Box::new(gtk::Orientation::Vertical, 0);
        centre.set_halign(gtk::Align::Center);
        centre.set_valign(gtk::Align::Center);
        let number = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        number.set_halign(gtk::Align::Center);
        let whole = label("—", &["hero"]);
        let frac = label("", &["hero-frac"]);
        let unit = label("°C", &["hero-unit"]);
        for l in [&whole, &frac, &unit] {
            l.set_valign(gtk::Align::BaselineFill);
            number.append(l);
        }
        centre.append(&number);
        centre.append(&label("CPU temperature", &["hero-caption"]));
        overlay.add_overlay(&centre);
        let mood = label("", &["mood"]);
        mood.set_halign(gtk::Align::Center);
        mood.set_valign(gtk::Align::End);
        mood.set_margin_bottom(30);
        overlay.add_overlay(&mood);
        content.append(&overlay);
        let hero = Hero { whole, frac, unit, mood };

        // Readouts.
        let readouts = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        readouts.set_homogeneous(true);
        readouts.add_css_class("readouts");
        let (b1, load) = readout("Load");
        let (b2, power) = readout("Package power");
        let (b3, clock) = readout("Average clock");
        readouts.append(&b1);
        readouts.append(&b2);
        readouts.append(&b3);
        content.append(&readouts);

        // History chart.
        let chart_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        chart_head.add_css_class("section-head");
        let h = label("Last two minutes", &["section-title"]);
        h.set_hexpand(true);
        h.set_xalign(0.0);
        let range = label("", &["section-meta"]);
        chart_head.append(&h);
        chart_head.append(&range);
        content.append(&chart_head);
        let chart = gtk::DrawingArea::builder().content_height(150).hexpand(true).build();
        chart.add_css_class("chart");
        content.append(&chart);
        let legend = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        legend.add_css_class("legend");
        for (swatch, text) in [("swatch-temp", "Temperature"), ("swatch-load", "Load")] {
            let item = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let s = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            s.add_css_class(swatch);
            s.set_valign(gtk::Align::Center);
            item.append(&s);
            item.append(&label(text, &[]));
            legend.append(&item);
        }
        content.append(&legend);

        // Settings.
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("card");
        let (unit_box, units) = segmented(&["°C", "°F"]);
        let (unit_c, unit_f) = (units[0].clone(), units[1].clone());
        (if fahrenheit { &unit_f } else { &unit_c }).set_active(true);
        card.append(&setting_row("Temperature unit", "On the cooler and here", &unit_box));
        let (theme_box, themes) = segmented(&["Auto", "Light", "Dark"]);
        themes[match theme {
            Theme::System => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        }]
        .set_active(true);
        card.append(&setting_row("Appearance", "Auto follows your desktop", &theme_box));
        let login = gtk::Switch::builder().valign(gtk::Align::Center).active(autostart::is_enabled()).build();
        card.append(&setting_row("Start at login", "Runs quietly in the top bar", &login));
        content.append(&card);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        footer.add_css_class("footer");
        let note = label("Closing this window keeps the display running.", &["footer-note"]);
        note.set_hexpand(true);
        note.set_xalign(0.0);
        note.set_wrap(true);
        let quit = gtk::Button::with_label("Quit");
        quit.add_css_class("quit");
        footer.append(&note);
        footer.append(&quit);
        content.append(&footer);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&content)
            .build();
        window.set_child(Some(&scroller));

        let interface = gio::SettingsSchemaSource::default()
            .and_then(|src| src.lookup("org.gnome.desktop.interface", true))
            .filter(|schema| schema.has_key("color-scheme"))
            .map(|_| gio::Settings::new("org.gnome.desktop.interface"));
        let system_dark = interface.as_ref().map(|s| s.string("color-scheme") == "prefer-dark").unwrap_or(true);

        let state = Rc::new(RefCell::new(State {
            history: VecDeque::with_capacity(SAMPLES),
            reading: Reading::default(),
            shown_temp: None,
            shown_load: 0.0,
            fahrenheit,
            theme,
            system_dark,
            hover_x: None,
        }));

        let ui = Rc::new(Self {
            window,
            state,
            dial,
            chart,
            hero,
            load,
            power,
            clock,
            range,
            pill,
            pill_label,
            banner,
            unit_c,
            unit_f,
            _interface: interface.clone(),
        });

        // Drawing.
        {
            let st = ui.state.clone();
            ui.dial.set_draw_func(move |_, cr, w, h| {
                let s = st.borrow();
                let pal = Palette::new(s.dark());
                let ds = dial::DialState { temp_c: s.shown_temp, load: s.shown_load, fahrenheit: s.fahrenheit };
                dial::draw(cr, w as f64, h as f64, &ds, &pal);
            });
        }
        {
            let st = ui.state.clone();
            ui.chart.set_draw_func(move |_, cr, w, h| {
                let s = st.borrow();
                let pal = Palette::new(s.dark());
                let cs = chart::ChartState { history: &s.history, hover_x: s.hover_x, fahrenheit: s.fahrenheit };
                chart::draw(cr, w as f64, h as f64, &cs, &pal);
            });
            let motion = gtk::EventControllerMotion::new();
            let (st1, st2) = (ui.state.clone(), ui.state.clone());
            let (c1, c2) = (ui.chart.clone(), ui.chart.clone());
            motion.connect_motion(move |_, x, _| {
                st1.borrow_mut().hover_x = Some(x);
                c1.queue_draw();
            });
            motion.connect_leave(move |_| {
                st2.borrow_mut().hover_x = None;
                c2.queue_draw();
            });
            ui.chart.add_controller(motion);
        }

        // Ease the dial and number toward the latest reading (runs only while the window is mapped).
        {
            let st = ui.state.clone();
            let hero = ui.hero.clone();
            ui.dial.add_tick_callback(move |w, _| {
                let mut s = st.borrow_mut();
                let target_t = s.reading.temp_c;
                let target_l = s.reading.load;
                let mut moved = false;
                s.shown_temp = match (s.shown_temp, target_t) {
                    (Some(cur), Some(t)) if (t - cur).abs() > 0.02 => {
                        moved = true;
                        Some(cur + (t - cur) * 0.14)
                    }
                    (cur, t) => {
                        moved |= cur != t;
                        t
                    }
                };
                if (target_l - s.shown_load).abs() > 0.05 {
                    s.shown_load += (target_l - s.shown_load) * 0.14;
                    moved = true;
                }
                if moved {
                    hero.set(s.shown_temp, s.fahrenheit);
                    w.queue_draw();
                }
                glib::ControlFlow::Continue
            });
        }

        // Controls.
        {
            let (ui2, act) = (Rc::downgrade(&ui), on_action.clone());
            ui.unit_f.connect_toggled(move |b| {
                if let Some(ui) = ui2.upgrade() {
                    let f = b.is_active();
                    if ui.state.borrow().fahrenheit != f {
                        ui.set_fahrenheit(f);
                        act(UiAction::SetFahrenheit(f));
                    }
                }
            });
        }
        for (i, b) in themes.iter().enumerate() {
            let (ui2, act) = (Rc::downgrade(&ui), on_action.clone());
            b.connect_toggled(move |b| {
                if !b.is_active() {
                    return;
                }
                let theme = [Theme::System, Theme::Light, Theme::Dark][i];
                if let Some(ui) = ui2.upgrade() {
                    ui.state.borrow_mut().theme = theme;
                    ui.apply_theme();
                }
                act(UiAction::SetTheme(theme));
            });
        }
        login.connect_state_set(|sw, on| {
            if let Err(e) = autostart::set_enabled(on) {
                eprintln!("zeus-display: could not change the login entry: {e}");
                sw.set_active(!on);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        {
            let act = on_action.clone();
            quit.connect_clicked(move |_| act(UiAction::Quit));
        }
        if let Some(iface) = &interface {
            let ui2 = Rc::downgrade(&ui);
            iface.connect_changed(Some("color-scheme"), move |s, _| {
                if let Some(ui) = ui2.upgrade() {
                    ui.state.borrow_mut().system_dark = s.string("color-scheme") == "prefer-dark";
                    ui.apply_theme();
                }
            });
        }

        // Closing hides; the display loop and tray keep going.
        ui.window.connect_close_request(|w| {
            w.set_visible(false);
            glib::Propagation::Stop
        });
        let esc = gtk::ShortcutController::new();
        esc.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("Escape|<Control>w"),
            Some(gtk::CallbackAction::new(|w, _| {
                w.downcast_ref::<gtk::Window>().map(|w| w.close());
                glib::Propagation::Stop
            })),
        ));
        ui.window.add_controller(esc);

        ui.apply_theme();
        ui
    }

    fn apply_theme(&self) {
        let dark = self.state.borrow().dark();
        let (add, remove) = if dark { ("dark", "light") } else { ("light", "dark") };
        self.window.remove_css_class(remove);
        self.window.add_css_class(add);
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(dark);
        }
        self.dial.queue_draw();
        self.chart.queue_draw();
    }

    pub fn set_fahrenheit(&self, fahrenheit: bool) {
        self.state.borrow_mut().fahrenheit = fahrenheit;
        (if fahrenheit { &self.unit_f } else { &self.unit_c }).set_active(true);
        let s = self.state.borrow();
        self.hero.set(s.shown_temp, fahrenheit);
        drop(s);
        self.refresh_text();
        self.dial.queue_draw();
        self.chart.queue_draw();
    }

    pub fn present(&self, activation_token: Option<String>) {
        if let Some(token) = activation_token {
            self.window.set_startup_id(&token);
        }
        self.window.present();
    }

    pub fn push(&self, update: Update) {
        {
            let mut s = self.state.borrow_mut();
            s.reading = update.reading;
            if s.history.len() == SAMPLES {
                s.history.pop_front();
            }
            s.history.push_back(Sample { temp_c: update.reading.temp_c, load: update.reading.load });
        }
        self.set_link(update.link);
        if self.window.is_visible() {
            self.refresh_text();
            self.chart.queue_draw();
        }
    }

    fn refresh_text(&self) {
        let s = self.state.borrow();
        let r = s.reading;
        self.load.set_label(&format!("{:.0}%", r.load));
        self.power.set_label(&r.power_w.map(|w| format!("{w:.1} W")).unwrap_or_else(|| "—".into()));
        self.clock.set_label(&r.clock_ghz.map(|g| format!("{g:.2} GHz")).unwrap_or_else(|| "—".into()));
        let temps = s.history.iter().filter_map(|p| p.temp_c);
        let (lo, hi) = temps.fold((f64::MAX, f64::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));
        if lo <= hi {
            let f = s.fahrenheit;
            self.range.set_label(&format!("Low {:.0}°  ·  High {:.0}°", to_unit(lo, f), to_unit(hi, f)));
        }
    }

    fn set_link(&self, link: Link) {
        let (text, class) = match link {
            Link::Live => ("Display live", "live"),
            Link::Missing => ("No display", "missing"),
            Link::NoPermission => ("Needs access", "blocked"),
            Link::WriteFailed => ("Reconnecting", "missing"),
        };
        if self.pill_label.label() != text {
            self.pill_label.set_label(text);
        }
        for c in ["live", "missing", "blocked"] {
            if c == class {
                self.pill.add_css_class(c);
            } else {
                self.pill.remove_css_class(c);
            }
        }
        self.banner.set_reveal_child(link == Link::NoPermission);
    }
}

pub fn summary(update: &Update, fahrenheit: bool) -> String {
    let unit = if fahrenheit { "°F" } else { "°C" };
    let temp = update
        .reading
        .temp_c
        .map(|t| format!("{:.0}{unit}", to_unit(t, fahrenheit)))
        .unwrap_or_else(|| "—".into());
    let link = match update.link {
        Link::Live => "",
        Link::Missing | Link::WriteFailed => "  ·  display not connected",
        Link::NoPermission => "  ·  display needs access",
    };
    format!("CPU {temp}  ·  {:.0}% load{link}", update.reading.load)
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}
