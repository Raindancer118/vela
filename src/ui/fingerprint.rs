//! "Touch the fingerprint reader" while an update waits for sudo
//! (pam_fprintd prints its request into the log, nobody would see it).
//! A small overlay in the middle of the focused monitor that never takes
//! the keyboard. The fingerprint icon breathes slowly while it waits; a check
//! mark or a red shake ends it before it fades out.

use super::store::ConfigStore;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

pub struct FingerprintPrompt {
    window: gtk::Window,
    card: gtk::Box,
    icon: gtk::Image,
    title: gtk::Label,
    detail: gtk::Label,
    css: gtk::CssProvider,
    store: ConfigStore,
    waiting: Cell<bool>,
    hide_timer: RefCell<Option<glib::SourceId>>,
}

impl FingerprintPrompt {
    pub fn new(store: ConfigStore) -> Rc<FingerprintPrompt> {
        let window = gtk::Window::builder().title("Vela").resizable(false).decorated(false).build();
        window.add_css_class("vela-fp-window");
        if gtk4_layer_shell::is_supported() {
            window.init_layer_shell();
            window.set_layer(Layer::Overlay);
            // Same namespace as the launcher: blurred by vela.lua's layer rule.
            window.set_namespace(Some("vela"));
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_exclusive_zone(-1);
        }
        let css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 10);
        }

        let icon = gtk::Image::builder()
            .icon_name("vela-fingerprint-symbolic")
            .pixel_size(56)
            .css_classes(["vela-fp-icon"])
            .build();
        let disc = gtk::Box::builder().halign(gtk::Align::Center).css_classes(["vela-fp-disc"]).build();
        disc.append(&icon);
        let stage = gtk::Box::builder().halign(gtk::Align::Center).css_classes(["vela-fp-stage"]).build();
        stage.append(&disc);
        let title = gtk::Label::builder().css_classes(["vela-fp-title"]).margin_top(14).build();
        let detail = gtk::Label::builder()
            .css_classes(["vela-fp-detail"])
            .wrap(true)
            .max_width_chars(36)
            .justify(gtk::Justification::Center)
            .build();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        card.add_css_class("vela-fp");
        card.append(&stage);
        card.append(&title);
        card.append(&detail);
        window.set_child(Some(&card));

        Rc::new(FingerprintPrompt {
            window,
            card,
            icon,
            title,
            detail,
            css,
            store,
            waiting: Cell::new(false),
            hide_timer: RefCell::default(),
        })
    }

    fn set_state(&self, class: &str, icon: &str) {
        self.waiting.set(class == "waiting");
        for c in ["waiting", "ok", "fail", "in", "out"] {
            self.card.remove_css_class(c);
        }
        self.card.add_css_class(class);
        self.icon.set_icon_name(Some(icon));
    }

    /// `request` is pam_fprintd's line, e.g. "Place your right index finger …".
    pub fn show(&self, request: &str) {
        if let Some(t) = self.hide_timer.borrow_mut().take() {
            t.remove();
        }
        self.css.load_from_string(&super::style::fingerprint_css(&self.store.get()));
        self.title.set_label("Touch the fingerprint reader");
        self.detail.set_label(request.trim());
        if gtk4_layer_shell::is_supported() {
            // Where the user is right now, not the launcher's main monitor.
            let monitors: Vec<gdk::Monitor> = gdk::Display::default()
                .map(|d| d.monitors())
                .map(|l| (0..l.n_items()).filter_map(|i| l.item(i).and_downcast::<gdk::Monitor>()).collect())
                .unwrap_or_default();
            let monitor = crate::hyprland::focused_monitor().and_then(|n| monitors.into_iter().find(|m| m.connector().is_some_and(|c| c == n)));
            log::info!("updates: fingerprint prompt on {:?}", monitor.as_ref().and_then(|m| m.connector()));
            self.window.set_monitor(monitor.as_ref());
        }
        let was_visible = self.window.is_visible();
        // A retry after a mismatch keeps breathing without a restart.
        if !(was_visible && self.waiting.get()) {
            self.set_state("waiting", "vela-fingerprint-symbolic");
        }
        if !was_visible {
            self.card.add_css_class("in");
        }
        self.window.set_visible(true);
    }

    /// The reader answered (or gave up); fades out after a moment.
    pub fn finish(self: &Rc<Self>, ok: bool) {
        if !self.window.is_visible() {
            return;
        }
        log::info!("updates: fingerprint {}", if ok { "accepted" } else { "not accepted" });
        if ok {
            self.set_state("ok", "object-select-symbolic");
            self.title.set_label("Fingerprint accepted");
            self.detail.set_label("");
        } else {
            self.set_state("fail", "vela-fingerprint-symbolic");
            self.title.set_label("No fingerprint");
            self.detail.set_label("Enter your password in the dialog instead");
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(if ok { 1600 } else { 2600 }), move || {
            let Some(p) = weak.upgrade() else { return };
            p.hide_timer.borrow_mut().take();
            p.card.add_css_class("out");
            let weak = Rc::downgrade(&p);
            let id = glib::timeout_add_local_once(Duration::from_millis(400), move || {
                if let Some(p) = weak.upgrade() {
                    p.hide_timer.borrow_mut().take();
                    p.window.set_visible(false);
                }
            });
            *p.hide_timer.borrow_mut() = Some(id);
        });
        *self.hide_timer.borrow_mut() = Some(id);
    }

    /// Gone at once (the run ended while it was showing).
    pub fn hide(&self) {
        if let Some(t) = self.hide_timer.borrow_mut().take() {
            t.remove();
        }
        self.window.set_visible(false);
    }
}
