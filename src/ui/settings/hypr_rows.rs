//! Rows bound to Hyprland options through the HyprStore. Each shows the
//! value Hyprland uses right now and a reset button while vela overrides it.

use crate::hyprconf::{EdgeGaps, Gaps, Gradient, Kind, OptionInfo, Value};
use crate::ui::hypr_store::HyprStore;
use adw::prelude::*;
use gtk::gdk;
use std::rc::Rc;

#[derive(Clone)]
pub struct Rows {
    pub store: HyprStore,
}

fn argb_to_rgba(c: u32) -> gdk::RGBA {
    let ch = |shift: u32| ((c >> shift) & 0xff) as f32 / 255.0;
    gdk::RGBA::new(ch(16), ch(8), ch(0), ch(24))
}

fn rgba_to_argb(c: &gdk::RGBA) -> u32 {
    let ch = |v: f32| u32::from((v.clamp(0.0, 1.0) * 255.0).round() as u8);
    (ch(c.alpha()) << 24) | (ch(c.red()) << 16) | (ch(c.green()) << 8) | ch(c.blue())
}

/// Subtitle from the given text, else Hyprland's own description.
fn subtitle(info: Option<&OptionInfo>, given: &str) -> String {
    if !given.is_empty() {
        return given.to_owned();
    }
    info.map(|i| {
        let mut d = i.description.trim().to_owned();
        if let Some(first) = d.get(..1) {
            d = first.to_uppercase() + &d[1..];
        }
        d
    })
    .unwrap_or_default()
}

impl Rows {
    pub fn new(store: HyprStore) -> Rows {
        Rows { store }
    }

    /// Reset button that shows while any of `names` is overridden.
    fn reset_button(&self, names: &[&str]) -> gtk::Button {
        let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        let btn = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .tooltip_text("Back to the value from hyprland.lua")
            .css_classes(["flat", "circular", "vela-hypr-reset"])
            .valign(gtk::Align::Center)
            .build();
        let sync = {
            let (store, names, w) = (self.store.clone(), names.clone(), btn.downgrade());
            move || {
                if let Some(b) = w.upgrade() {
                    b.set_visible(names.iter().any(|n| store.is_overridden(n)));
                }
            }
        };
        sync();
        {
            let names = names.clone();
            self.store.subscribe(move |n| {
                if n.is_none_or(|n| names.iter().any(|x| x == n)) {
                    sync();
                }
            });
        }
        let store = self.store.clone();
        btn.connect_clicked(move |_| store.reset(&names));
        btn
    }

    /// Calls `f` now and whenever `name` changes.
    fn watch(&self, name: &str, f: impl Fn(Option<Value>) + 'static) {
        f(self.store.value(name));
        let (store, name) = (self.store.clone(), name.to_owned());
        self.store.subscribe(move |n| {
            if n.is_none_or(|n| n == name) {
                f(store.value(&name));
            }
        });
    }

    fn mark_unsupported(&self, row: &impl IsA<gtk::Widget>, name: &str) -> Option<OptionInfo> {
        let info = self.store.info(name);
        if info.is_none() {
            row.set_sensitive(false);
            row.set_tooltip_text(Some(&format!("{name} is not available in this Hyprland")));
        }
        info
    }

    pub fn switch(&self, name: &str, title: &str, sub: &str) -> adw::SwitchRow {
        let row = adw::SwitchRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        row.add_suffix(&self.reset_button(&[name]));
        let w = row.downgrade();
        self.watch(name, move |v| {
            if let (Some(r), Some(b)) = (w.upgrade(), v.and_then(|v| v.as_bool())) {
                r.set_active(b);
            }
        });
        let (store, name) = (self.store.clone(), name.to_owned());
        row.connect_active_notify(move |r| {
            if !store.notifying() {
                store.set(&name, Value::Bool(r.is_active()));
            }
        });
        row
    }

    /// Row whose switch is a bool option; the rows inside (its details) show
    /// while it is on.
    pub fn expander(&self, name: &str, title: &str, sub: &str) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::builder().title(title).show_enable_switch(true).build();
        let info = self.mark_unsupported(&row, name);
        let s = subtitle(info.as_ref(), sub);
        if !s.is_empty() {
            row.set_subtitle(&s);
        }
        row.add_suffix(&self.reset_button(&[name]));
        let w = row.downgrade();
        self.watch(name, move |v| {
            if let (Some(r), Some(b)) = (w.upgrade(), v.and_then(|v| v.as_bool())) {
                r.set_enable_expansion(b);
            }
        });
        let (store, name) = (self.store.clone(), name.to_owned());
        row.connect_enable_expansion_notify(move |r| {
            if !store.notifying() {
                store.set(&name, Value::Bool(r.enables_expansion()));
            }
        });
        row
    }

    fn scale(min: f64, max: f64, step: f64, digits: u32) -> (gtk::Scale, gtk::Label) {
        let adj = gtk::Adjustment::new(min, min, max, step, step * 10.0, 0.0);
        let scale = gtk::Scale::builder()
            .adjustment(&adj)
            .digits(digits as i32)
            .draw_value(false)
            .width_request(240)
            .valign(gtk::Align::Center)
            .css_classes(["vela-hypr-scale"])
            .build();
        scale.set_round_digits(digits as i32);
        let label = gtk::Label::builder().width_chars(5).xalign(1.0).css_classes(["numeric", "dim-label"]).build();
        (scale, label)
    }

    /// `unit` "%" shows a 0–1 value as a percentage.
    fn format_number(v: f64, digits: u32, unit: &str) -> String {
        if unit == "%" {
            return format!("{:.0} %", v * 100.0);
        }
        format!("{v:.*}{unit}", digits as usize)
    }

    /// Slider for an int or float option. The range is the curated one,
    /// narrowed to what Hyprland accepts.
    #[allow(clippy::too_many_arguments)]
    pub fn slider(&self, name: &str, title: &str, sub: &str, min: f64, max: f64, step: f64, digits: u32, unit: &'static str) -> adw::ActionRow {
        let row = adw::ActionRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        let lo = info.as_ref().and_then(|i| i.min).map_or(min, |m| m.max(min));
        let hi = info.as_ref().and_then(|i| i.max).map_or(max, |m| m.min(max)).max(lo);
        let (scale, label) = Self::scale(lo, hi, step, digits);
        let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        suffix.append(&scale);
        suffix.append(&label);
        suffix.append(&self.reset_button(&[name]));
        row.add_suffix(&suffix);
        let (ws, wl) = (scale.downgrade(), label.downgrade());
        self.watch(name, move |v| {
            let Some(v) = v.and_then(|v| v.as_f64()) else { return };
            if let Some(s) = ws.upgrade() {
                // Values beyond the curated range stay visible in the label.
                s.adjustment().set_upper(s.adjustment().upper().max(v));
                s.set_value(v);
            }
            if let Some(l) = wl.upgrade() {
                l.set_label(&Self::format_number(v, digits, unit));
            }
        });
        let kind = info.map_or(Kind::Float, |i| i.kind);
        let (store, name, wl) = (self.store.clone(), name.to_owned(), label.downgrade());
        scale.connect_value_changed(move |s| {
            let v = s.value();
            if let Some(l) = wl.upgrade() {
                l.set_label(&Self::format_number(v, digits, unit));
            }
            if !store.notifying()
                && let Some(val) = Value::number(kind, v)
            {
                store.set(&name, val);
            }
        });
        row
    }

    /// Drop-down for an int option with named values (or the given ones).
    pub fn choice(&self, name: &str, title: &str, sub: &str, labels: &[(i64, &str)]) -> adw::ComboRow {
        let row = adw::ComboRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        let values: Vec<(i64, String)> = if labels.is_empty() {
            info.map(|i| i.choices.clone()).unwrap_or_default()
        } else {
            labels.iter().map(|(v, l)| (*v, l.to_string())).collect()
        };
        let model = gtk::StringList::new(&values.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>());
        row.set_model(Some(&model));
        row.add_suffix(&self.reset_button(&[name]));
        let values = Rc::new(values);
        {
            let (w, values) = (row.downgrade(), values.clone());
            self.watch(name, move |v| {
                let Some(v) = v.and_then(|v| v.as_f64()) else { return };
                if let (Some(r), Some(i)) = (w.upgrade(), values.iter().position(|(x, _)| *x as f64 == v)) {
                    r.set_selected(i as u32);
                }
            });
        }
        let (store, name) = (self.store.clone(), name.to_owned());
        row.connect_selected_notify(move |r| {
            if store.notifying() {
                return;
            }
            if let Some((v, _)) = values.get(r.selected() as usize) {
                store.set(&name, Value::Int(*v));
            }
        });
        row
    }

    /// Drop-down for a string option with fixed choices.
    pub fn choice_str(&self, name: &str, title: &str, sub: &str, labels: &[(&str, &str)]) -> adw::ComboRow {
        let row = adw::ComboRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        let values: Rc<Vec<(String, String)>> = Rc::new(labels.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect());
        let model = gtk::StringList::new(&values.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>());
        row.set_model(Some(&model));
        row.add_suffix(&self.reset_button(&[name]));
        {
            let (w, values) = (row.downgrade(), values.clone());
            self.watch(name, move |v| {
                let Some(Value::Str(v)) = v else { return };
                if let (Some(r), Some(i)) = (w.upgrade(), values.iter().position(|(x, _)| *x == v)) {
                    r.set_selected(i as u32);
                }
            });
        }
        let (store, name) = (self.store.clone(), name.to_owned());
        row.connect_selected_notify(move |r| {
            if store.notifying() {
                return;
            }
            if let Some((v, _)) = values.get(r.selected() as usize) {
                store.set(&name, Value::Str(v.clone()));
            }
        });
        row
    }

    #[allow(dead_code)] // Keyboard layout (Input page) is next.
    pub fn entry(&self, name: &str, title: &str) -> adw::EntryRow {
        let row = adw::EntryRow::builder().title(title).show_apply_button(true).build();
        if let Some(info) = self.mark_unsupported(&row, name) {
            row.set_tooltip_text(Some(&subtitle(Some(&info), "")));
        }
        row.add_suffix(&self.reset_button(&[name]));
        let w = row.downgrade();
        self.watch(name, move |v| {
            if let (Some(r), Some(Value::Str(s))) = (w.upgrade(), v)
                && r.text() != s
            {
                r.set_text(&s);
            }
        });
        // Applied on Enter / the apply button: half-typed layouts would
        // otherwise reach Hyprland on every key.
        let (store, name) = (self.store.clone(), name.to_owned());
        row.connect_apply(move |r| store.set(&name, Value::Str(r.text().to_string())));
        row
    }

    fn color_button() -> gtk::ColorDialogButton {
        let b = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(true).build()));
        b.set_valign(gtk::Align::Center);
        b
    }

    /// Colour or gradient option. Gradients get a second colour and an angle
    /// behind a switch.
    pub fn color(&self, name: &str, title: &str, sub: &str) -> adw::PreferencesRow {
        let info = self.store.info(name);
        if info.as_ref().is_some_and(|i| i.kind == Kind::Gradient) {
            return self.gradient(name, title, sub).upcast();
        }
        let row = adw::ActionRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        let btn = Self::color_button();
        let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        suffix.append(&btn);
        suffix.append(&self.reset_button(&[name]));
        row.add_suffix(&suffix);
        let w = btn.downgrade();
        self.watch(name, move |v| {
            if let (Some(b), Some(Value::Color(c))) = (w.upgrade(), v) {
                b.set_rgba(&argb_to_rgba(c));
            }
        });
        let (store, name) = (self.store.clone(), name.to_owned());
        btn.connect_rgba_notify(move |b| {
            if !store.notifying() {
                store.set(&name, Value::Color(rgba_to_argb(&b.rgba())));
            }
        });
        row.upcast()
    }

    fn gradient(&self, name: &str, title: &str, sub: &str) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        let s = subtitle(info.as_ref(), sub);
        if !s.is_empty() {
            row.set_subtitle(&s);
        }
        let first = Self::color_button();
        let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        suffix.append(&first);
        suffix.append(&self.reset_button(&[name]));
        row.add_suffix(&suffix);

        let second_row = adw::ActionRow::builder().title("Second colour").subtitle("Blend into a second colour").build();
        let use_second = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let second = Self::color_button();
        second_row.add_suffix(&second);
        second_row.add_suffix(&use_second);
        second_row.set_activatable_widget(Some(&use_second));
        let angle_row = adw::ActionRow::builder().title("Angle").subtitle("Direction of the blend").build();
        let (angle, angle_label) = Self::scale(0.0, 360.0, 15.0, 0);
        angle_row.add_suffix(&angle);
        angle_row.add_suffix(&angle_label);
        row.add_row(&second_row);
        row.add_row(&angle_row);

        let (wf, ws, wu, wa, wl, war) = (
            first.downgrade(),
            second.downgrade(),
            use_second.downgrade(),
            angle.downgrade(),
            angle_label.downgrade(),
            angle_row.downgrade(),
        );
        self.watch(name, move |v| {
            let Some(Value::Gradient(g)) = v else { return };
            let (Some(f), Some(s), Some(u), Some(a), Some(l), Some(ar)) = (wf.upgrade(), ws.upgrade(), wu.upgrade(), wa.upgrade(), wl.upgrade(), war.upgrade())
            else {
                return;
            };
            f.set_rgba(&argb_to_rgba(g.colors[0]));
            let two = g.colors.len() > 1;
            u.set_active(two);
            s.set_sensitive(two);
            ar.set_sensitive(two);
            if let Some(c) = g.colors.get(1) {
                s.set_rgba(&argb_to_rgba(*c));
            }
            a.set_value(g.angle as f64);
            l.set_label(&format!("{}°", g.angle));
        });

        let write = {
            let (store, name) = (self.store.clone(), name.to_owned());
            let (wf, ws, wu, wa) = (first.downgrade(), second.downgrade(), use_second.downgrade(), angle.downgrade());
            Rc::new(move || {
                if store.notifying() {
                    return;
                }
                let (Some(f), Some(s), Some(u), Some(a)) = (wf.upgrade(), ws.upgrade(), wu.upgrade(), wa.upgrade()) else {
                    return;
                };
                let mut colors = vec![rgba_to_argb(&f.rgba())];
                if u.is_active() {
                    colors.push(rgba_to_argb(&s.rgba()));
                }
                let angle = if u.is_active() { a.value().round() as i64 } else { 0 };
                store.set(&name, Value::Gradient(Gradient { colors, angle }));
            })
        };
        let w = write.clone();
        first.connect_rgba_notify(move |_| w());
        let w = write.clone();
        second.connect_rgba_notify(move |_| w());
        let (w, ws, war) = (write.clone(), second.downgrade(), angle_row.downgrade());
        use_second.connect_active_notify(move |u| {
            if let (Some(s), Some(ar)) = (ws.upgrade(), war.upgrade()) {
                s.set_sensitive(u.is_active());
                ar.set_sensitive(u.is_active());
            }
            w();
        });
        let wl = angle_label.downgrade();
        angle.connect_value_changed(move |a| {
            if let Some(l) = wl.upgrade() {
                l.set_label(&format!("{}°", a.value().round()));
            }
            write();
        });
        row
    }

    /// Gaps of one option as a single slider (all four sides alike).
    pub fn gaps_slider(&self, name: &str, title: &str, sub: &str, max: f64) -> adw::ActionRow {
        let row = adw::ActionRow::builder().title(title).build();
        let info = self.mark_unsupported(&row, name);
        row.set_subtitle(&subtitle(info.as_ref(), sub));
        let (scale, label) = Self::scale(0.0, max, 1.0, 0);
        let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        suffix.append(&scale);
        suffix.append(&label);
        suffix.append(&self.reset_button(&[name]));
        row.add_suffix(&suffix);
        let (ws, wl) = (scale.downgrade(), label.downgrade());
        self.watch(name, move |v| {
            let Some(Value::Gaps(g)) = v else { return };
            if let (Some(s), Some(l)) = (ws.upgrade(), wl.upgrade()) {
                s.adjustment().set_upper(s.adjustment().upper().max(g.max() as f64));
                s.set_value(g.max() as f64);
                l.set_label(&format!("{} px", g.max()));
            }
        });
        let (store, name, wl) = (self.store.clone(), name.to_owned(), label.downgrade());
        scale.connect_value_changed(move |s| {
            let v = s.value().round() as i64;
            if let Some(l) = wl.upgrade() {
                l.set_label(&format!("{v} px"));
            }
            if !store.notifying() {
                store.set(&name, Value::Gaps(Gaps::uniform(v)));
            }
        });
        row
    }

    /// `general:gaps_out` as on/off with one width and a switch per edge.
    pub fn edge_gaps(&self) -> adw::ExpanderRow {
        const NAME: &str = "general:gaps_out";
        const REMEMBER: &str = "edge_gap";
        let row = adw::ExpanderRow::builder()
            .title("Gaps at screen edges")
            .subtitle("Space between windows and the edges of the screen")
            .show_enable_switch(true)
            .build();
        self.mark_unsupported(&row, NAME);
        row.add_suffix(&self.reset_button(&[NAME]));
        let width_row = adw::ActionRow::builder().title("Width").build();
        let (scale, label) = Self::scale(1.0, 80.0, 1.0, 0);
        width_row.add_suffix(&scale);
        width_row.add_suffix(&label);
        row.add_row(&width_row);
        let edges: Vec<(&str, adw::SwitchRow)> = [("Top", "top"), ("Bottom", "bottom"), ("Left", "left"), ("Right", "right")]
            .into_iter()
            .map(|(title, key)| (key, adw::SwitchRow::builder().title(title).build()))
            .collect();
        for (_, r) in &edges {
            row.add_row(r);
        }

        let current = {
            let store = self.store.clone();
            move || {
                let g = match store.value(NAME) {
                    Some(Value::Gaps(g)) => g,
                    _ => Gaps::uniform(0),
                };
                EdgeGaps::from_gaps(g, store.remembered(REMEMBER).unwrap_or(8))
            }
        };
        {
            let (wr, ws, wl, current) = (row.downgrade(), scale.downgrade(), label.downgrade(), current.clone());
            let we: Vec<(&str, _)> = edges.iter().map(|(k, r)| (*k, r.downgrade())).collect();
            self.watch(NAME, move |_| {
                let e = current();
                if let Some(r) = wr.upgrade() {
                    r.set_enable_expansion(e.any());
                }
                if let (Some(s), Some(l)) = (ws.upgrade(), wl.upgrade()) {
                    s.adjustment().set_upper(s.adjustment().upper().max(e.width as f64));
                    s.set_value(e.width as f64);
                    l.set_label(&format!("{} px", e.width));
                }
                for (k, w) in &we {
                    if let Some(r) = w.upgrade() {
                        r.set_active(match *k {
                            "top" => e.top,
                            "bottom" => e.bottom,
                            "left" => e.left,
                            _ => e.right,
                        });
                    }
                }
            });
        }
        let store = self.store.clone();
        let write = Rc::new(move |e: EdgeGaps| {
            if store.notifying() {
                return;
            }
            store.remember(REMEMBER, e.width);
            store.set(NAME, Value::Gaps(e.to_gaps()));
        });
        {
            let (write, current) = (write.clone(), current.clone());
            row.connect_enable_expansion_notify(move |r| {
                let e = current();
                if r.enables_expansion() == e.any() {
                    return;
                }
                let on = r.enables_expansion();
                write(EdgeGaps {
                    top: on,
                    right: on,
                    bottom: on,
                    left: on,
                    ..e
                });
            });
        }
        {
            let (write, current, wl) = (write.clone(), current.clone(), label.downgrade());
            scale.connect_value_changed(move |s| {
                let width = s.value().round() as i64;
                if let Some(l) = wl.upgrade() {
                    l.set_label(&format!("{width} px"));
                }
                write(EdgeGaps { width, ..current() });
            });
        }
        for (key, r) in edges {
            let (write, current) = (write.clone(), current.clone());
            r.connect_active_notify(move |r| {
                let mut e = current();
                let on = r.is_active();
                match key {
                    "top" => e.top = on,
                    "bottom" => e.bottom = on,
                    "left" => e.left = on,
                    _ => e.right = on,
                }
                write(e);
            });
        }
        row
    }
}
