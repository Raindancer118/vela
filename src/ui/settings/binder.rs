//! Two-way bindings between libadwaita rows and config fields.

use crate::config::Config;
use crate::ui::store::ConfigStore;
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type Refresher = Box<dyn Fn(&Config)>;

#[derive(Clone)]
pub struct Binder {
    pub store: ConfigStore,
    refreshers: Rc<RefCell<Vec<Refresher>>>,
    /// Set while widgets are refreshed from the config, so their change
    /// handlers don't write the same values back.
    updating: Rc<Cell<bool>>,
}

impl Binder {
    pub fn new(store: ConfigStore) -> Binder {
        Binder {
            store,
            refreshers: Rc::default(),
            updating: Rc::default(),
        }
    }

    pub fn refresh(&self, cfg: &Config) {
        self.updating.set(true);
        for r in self.refreshers.borrow().iter() {
            r(cfg);
        }
        self.updating.set(false);
    }

    pub fn on_refresh(&self, f: impl Fn(&Config) + 'static) {
        self.refreshers.borrow_mut().push(Box::new(f));
    }

    /// Runs `f` against the store unless a refresh is in progress.
    pub fn write(&self, f: impl FnOnce(&mut Config)) {
        if !self.updating.get() {
            self.store.update(f);
        }
    }

    pub fn switch(&self, title: &str, subtitle: &str, get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> adw::SwitchRow {
        let row = adw::SwitchRow::builder().title(title).subtitle(subtitle).active(get(&self.store.get())).build();
        let b = self.clone();
        row.connect_active_notify(move |r| {
            let v = r.is_active();
            b.write(|c| set(c, v));
        });
        let w = row.downgrade();
        self.on_refresh(move |c| {
            if let Some(r) = w.upgrade() {
                r.set_active(get(c));
            }
        });
        row
    }

    /// Row with an on/off switch; its rows (e.g. a delay) show when on.
    pub fn expander(&self, title: &str, subtitle: &str, get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> adw::ExpanderRow {
        let on = get(&self.store.get());
        let row = adw::ExpanderRow::builder()
            .title(title)
            .show_enable_switch(true)
            .enable_expansion(on)
            .expanded(on)
            .build();
        if !subtitle.is_empty() {
            row.set_subtitle(subtitle);
        }
        let b = self.clone();
        row.connect_enable_expansion_notify(move |r| {
            let v = r.enables_expansion();
            b.write(|c| set(c, v));
        });
        let w = row.downgrade();
        self.on_refresh(move |c| {
            if let Some(r) = w.upgrade() {
                r.set_enable_expansion(get(c));
            }
        });
        row
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spin(
        &self,
        title: &str,
        subtitle: &str,
        min: f64,
        max: f64,
        step: f64,
        digits: u32,
        get: fn(&Config) -> f64,
        set: fn(&mut Config, f64),
    ) -> adw::SpinRow {
        let adj = gtk::Adjustment::new(get(&self.store.get()), min, max, step, step * 10.0, 0.0);
        let row = adw::SpinRow::builder()
            .title(title)
            .subtitle(subtitle)
            .adjustment(&adj)
            .digits(digits)
            .climb_rate(1.0)
            .build();
        let b = self.clone();
        row.connect_value_notify(move |r| {
            let v = r.value();
            b.write(|c| set(c, v));
        });
        let w = row.downgrade();
        self.on_refresh(move |c| {
            if let Some(r) = w.upgrade() {
                r.set_value(get(c));
            }
        });
        row
    }

    pub fn entry(&self, title: &str, get: fn(&Config) -> String, set: fn(&mut Config, String)) -> adw::EntryRow {
        let row = adw::EntryRow::builder().title(title).text(get(&self.store.get())).build();
        let b = self.clone();
        row.connect_changed(move |r| {
            let v = r.text().to_string();
            b.write(|c| set(c, v));
        });
        let w = row.downgrade();
        self.on_refresh(move |c| {
            if let Some(r) = w.upgrade() {
                let v = get(c);
                if r.text() != v {
                    r.set_text(&v);
                }
            }
        });
        row
    }

    pub fn combo(&self, title: &str, subtitle: &str, labels: &[&str], get: fn(&Config) -> usize, set: fn(&mut Config, usize)) -> adw::ComboRow {
        let model = gtk::StringList::new(labels);
        let row = adw::ComboRow::builder()
            .title(title)
            .subtitle(subtitle)
            .model(&model)
            .selected(get(&self.store.get()) as u32)
            .build();
        let b = self.clone();
        row.connect_selected_notify(move |r| {
            let v = r.selected() as usize;
            b.write(|c| set(c, v));
        });
        let w = row.downgrade();
        self.on_refresh(move |c| {
            if let Some(r) = w.upgrade() {
                r.set_selected(get(c) as u32);
            }
        });
        row
    }
}

/// Splits a shell-like argument string; returns None on unbalanced quotes.
pub fn split_args(s: &str) -> Option<Vec<String>> {
    shlex::split(s)
}

pub fn join_args(args: &[String]) -> String {
    shlex::try_join(args.iter().map(String::as_str)).unwrap_or_else(|_| args.join(" "))
}

pub fn group(title: &str, description: &str) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::builder().title(title).build();
    if !description.is_empty() {
        g.set_description(Some(description));
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_strings_roundtrip() {
        let args = vec!["--dangerously-skip-permissions".to_string(), "--model".into(), "claude opus".into()];
        let s = join_args(&args);
        assert_eq!(split_args(&s).unwrap(), args);
        assert!(split_args("\"unbalanced").is_none());
    }
}
