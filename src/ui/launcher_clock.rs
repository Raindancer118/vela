//! The launcher's clock on the blurred backdrop: a click-through full-monitor
//! layer between the backdrop and the launcher ("vela-clock", no blur rule,
//! so the text stays sharp).

use crate::config::HorizontalPosition;
use crate::launcher_layout::{clock_formats, clock_origin, time_locale};
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

pub struct LauncherClock {
    window: gtk::Window,
    fixed: gtk::Fixed,
    column: gtk::Box,
    time: gtk::Label,
    date: gtk::Label,
    formats: (&'static str, &'static str),
    tick: RefCell<Option<glib::SourceId>>,
}

/// Visible launcher card on its monitor: left edge, width, top.
pub type LauncherRect = (i32, i32, i32);

impl LauncherClock {
    pub fn new(app: &adw::Application) -> Rc<LauncherClock> {
        let window = gtk::Window::builder().application(app).title("Vela clock").decorated(false).build();
        window.add_css_class("vela-clock");
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("vela-clock"));
        window.set_keyboard_mode(KeyboardMode::None);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.connect_map(|w| {
            if let Some(surface) = w.surface() {
                surface.set_input_region(Some(&gtk::cairo::Region::create()));
            }
        });
        let time = gtk::Label::builder().css_classes(["vela-clock-time"]).build();
        let date = gtk::Label::builder().css_classes(["vela-clock-date"]).build();
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&time);
        column.append(&date);
        let fixed = gtk::Fixed::new();
        fixed.put(&column, 0.0, 0.0);
        window.set_child(Some(&fixed));
        let locale = time_locale(|k| std::env::var(k).ok());
        Rc::new(LauncherClock {
            window,
            fixed,
            column,
            time,
            date,
            formats: clock_formats(&locale),
            tick: RefCell::default(),
        })
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    fn update_text(&self) {
        let Ok(now) = glib::DateTime::now_local() else {
            return;
        };
        for (label, fmt) in [(&self.time, self.formats.0), (&self.date, self.formats.1)] {
            if let Ok(s) = now.format(fmt)
                && label.text() != s
            {
                label.set_text(&s);
            }
        }
    }

    /// Places the clock on `monitor` next to `launcher`. Returns true when it
    /// (re)mapped the surface, so the launcher has to be mapped after it to
    /// stay on top. `restack` maps it again even if it is already shown.
    pub fn show(self: &Rc<Self>, monitor: &gdk::Monitor, launcher: LauncherRect, pos: HorizontalPosition, restack: bool) -> bool {
        self.update_text();
        let g = monitor.geometry();
        let (_, cw, _, _) = self.column.measure(gtk::Orientation::Horizontal, -1);
        let (_, ch, _, _) = self.column.measure(gtk::Orientation::Vertical, cw);
        let (x, y) = clock_origin((g.width(), g.height()), launcher, (cw, ch), pos);
        self.fixed.move_(&self.column, f64::from(x), f64::from(y));
        if self.tick.borrow().is_none() {
            let me = Rc::downgrade(self);
            let id = glib::timeout_add_seconds_local(1, move || match me.upgrade() {
                Some(c) => {
                    c.update_text();
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            });
            *self.tick.borrow_mut() = Some(id);
        }
        let same = self.window.is_visible() && self.window.monitor().as_ref() == Some(monitor);
        if same && !restack {
            return false;
        }
        self.window.set_visible(false);
        self.window.set_monitor(Some(monitor));
        self.window.present();
        true
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
        if let Some(id) = self.tick.borrow_mut().take() {
            id.remove();
        }
    }
}
