//! Owner of vela's Hyprland overrides. A change is applied live through
//! `hyprctl eval` (coalesced, sliders fire often) and saved shortly after as
//! hyprland.toml plus the generated Lua `vela.lua` loads on every config
//! (re)load. Resetting an option drops the override and reloads Hyprland's
//! config, so the value from hyprland.lua comes back.

use crate::config::write_atomic;
use crate::hypranim::{Anim, AnimState};
use crate::hyprconf::{OptionInfo, Overrides, Value};
use crate::hyprmon::{MonitorInfo, MonitorRule};
use crate::{hyprland, paths};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Called with the option that changed, or None when everything may have.
type Listener = Box<dyn Fn(Option<&str>)>;
type ErrorListener = Box<dyn Fn(Option<&str>)>;

struct Inner {
    toml_path: PathBuf,
    lua_path: PathBuf,
    overrides: RefCell<Overrides>,
    infos: RefCell<BTreeMap<String, OptionInfo>>,
    current: RefCell<BTreeMap<String, Value>>,
    anims: RefCell<BTreeMap<String, AnimState>>,
    curves: RefCell<Vec<String>>,
    monitors: RefCell<Vec<MonitorInfo>>,
    /// Values from before vela's first change this session: setting an
    /// option back to it drops the override again.
    baseline: RefCell<BTreeMap<String, Value>>,
    baseline_anims: RefCell<BTreeMap<String, Anim>>,
    pending_monitors: RefCell<Vec<MonitorRule>>,
    monitor_refresh: RefCell<Option<glib::SourceId>>,
    loaded: Cell<bool>,
    pending_apply: RefCell<BTreeMap<String, Value>>,
    pending_anims: RefCell<BTreeMap<String, Anim>>,
    apply_source: RefCell<Option<glib::SourceId>>,
    save_source: RefCell<Option<glib::SourceId>>,
    last_written: RefCell<String>,
    listeners: RefCell<Vec<Listener>>,
    error_listeners: RefCell<Vec<ErrorListener>>,
    notifying: Cell<bool>,
    monitor: RefCell<Option<gio::FileMonitor>>,
}

#[derive(Clone)]
pub struct HyprStore(Rc<Inner>);

impl HyprStore {
    pub fn load() -> HyprStore {
        let toml_path = paths::hypr_overrides_file();
        let text = std::fs::read_to_string(&toml_path).unwrap_or_default();
        let overrides = Overrides::from_toml(&text).unwrap_or_else(|e| {
            log::error!("{}: {e:#}; starting without Hyprland overrides", toml_path.display());
            Overrides::default()
        });
        let store = HyprStore(Rc::new(Inner {
            toml_path,
            lua_path: paths::hypr_lua_file(),
            overrides: RefCell::new(overrides),
            infos: RefCell::default(),
            current: RefCell::default(),
            anims: RefCell::default(),
            curves: RefCell::default(),
            monitors: RefCell::default(),
            baseline: RefCell::default(),
            baseline_anims: RefCell::default(),
            pending_monitors: RefCell::default(),
            monitor_refresh: RefCell::default(),
            loaded: Cell::new(false),
            pending_apply: RefCell::default(),
            pending_anims: RefCell::default(),
            apply_source: RefCell::default(),
            save_source: RefCell::default(),
            last_written: RefCell::new(text),
            listeners: RefCell::default(),
            error_listeners: RefCell::default(),
            notifying: Cell::new(false),
            monitor: RefCell::default(),
        }));
        store.sync_lua();
        store
    }

    /// Writes the Lua if it doesn't match hyprland.toml (edited while vela
    /// wasn't running, or a first start) and applies it to the running
    /// Hyprland.
    fn sync_lua(&self) {
        let lua = self.0.overrides.borrow().to_lua();
        if std::fs::read_to_string(&self.0.lua_path).ok().as_deref() == Some(lua.as_str()) {
            return;
        }
        if let Err(e) = write_atomic(&self.0.lua_path, &lua) {
            log::error!("cannot write {}: {e:#}", self.0.lua_path.display());
            return;
        }
        let code = format!("dofile({})", crate::hyprconf::lua_string(&self.0.lua_path.to_string_lossy()));
        if let Err(e) = hyprland::eval(&[code]) {
            log::info!("hyprland overrides not applied now: {e}");
        }
    }

    /// Asks Hyprland for its options; the settings call this when shown.
    pub fn ensure_loaded(&self) -> bool {
        if !self.0.loaded.get() {
            self.refresh_from_hyprland();
        }
        self.0.loaded.get()
    }

    fn refresh_from_hyprland(&self) {
        let Some((infos, current)) = hyprland::options() else { return };
        *self.0.infos.borrow_mut() = infos.into_iter().map(|i| (i.name.clone(), i)).collect();
        // Values not yet sent still win over what Hyprland reports.
        let mut current = current;
        for (k, v) in self.0.pending_apply.borrow().iter() {
            current.insert(k.clone(), v.clone());
        }
        *self.0.current.borrow_mut() = current;
        if let Some((mut anims, mut curves)) = hyprland::animations() {
            for (leaf, a) in self.0.pending_anims.borrow().iter() {
                anims.insert(
                    leaf.clone(),
                    AnimState {
                        overridden: true,
                        anim: a.clone(),
                    },
                );
            }
            for c in crate::hypranim::vela_curve_names() {
                if !curves.contains(&c) {
                    curves.push(c);
                }
            }
            *self.0.anims.borrow_mut() = anims;
            *self.0.curves.borrow_mut() = curves;
        }
        if let Some(m) = hyprland::monitors_all() {
            *self.0.monitors.borrow_mut() = m;
        }
        self.0.loaded.set(true);
    }

    pub fn monitors(&self) -> Vec<MonitorInfo> {
        self.0.monitors.borrow().clone()
    }

    /// Re-reads the monitors (after a change Hyprland needs a moment).
    pub fn refresh_monitors(&self) {
        if let Some(m) = hyprland::monitors_all() {
            let changed = *self.0.monitors.borrow() != m;
            *self.0.monitors.borrow_mut() = m;
            if changed {
                self.notify(Some("monitor:*"));
            }
        }
    }

    pub fn monitor_overrides(&self) -> BTreeMap<String, MonitorRule> {
        self.0.overrides.borrow().monitors.clone()
    }

    /// Applies full monitor rules live and keeps them.
    pub fn set_monitors(&self, rules: Vec<MonitorRule>) {
        if rules.is_empty() {
            return;
        }
        {
            let mut o = self.0.overrides.borrow_mut();
            for r in &rules {
                o.monitors.insert(r.output.clone(), r.clone());
            }
        }
        self.0.pending_monitors.borrow_mut().extend(rules);
        self.schedule_apply();
        self.schedule_save();
        self.schedule_monitor_refresh();
    }

    /// Puts the monitor overrides back as they were and applies `rules`
    /// (the state before a change that wasn't kept).
    pub fn restore_monitors(&self, overrides: BTreeMap<String, MonitorRule>, rules: Vec<MonitorRule>) {
        self.0.overrides.borrow_mut().monitors = overrides;
        self.0.pending_monitors.borrow_mut().extend(rules);
        self.schedule_apply();
        self.save_now();
        self.schedule_monitor_refresh();
    }

    fn schedule_monitor_refresh(&self) {
        if let Some(id) = self.0.monitor_refresh.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(700), move || {
            this.0.monitor_refresh.borrow_mut().take();
            this.refresh_monitors();
        });
        *self.0.monitor_refresh.borrow_mut() = Some(id);
    }

    /// What an animation does now (its own settings or inherited ones).
    pub fn anim(&self, leaf: &str) -> Option<Anim> {
        crate::hypranim::effective(&self.0.anims.borrow(), leaf)
    }

    /// Bezier names and `spring:<name>` springs Hyprland knows.
    pub fn curves(&self) -> Vec<String> {
        self.0.curves.borrow().clone()
    }

    pub fn set_anim(&self, leaf: &str, anim: Anim) {
        if self.anim(leaf).as_ref() == Some(&anim) && self.0.overrides.borrow().animations.contains_key(leaf) {
            return;
        }
        if !self.is_overridden(&format!("anim:{leaf}"))
            && let Some(before) = self.anim(leaf)
        {
            self.0.baseline_anims.borrow_mut().entry(leaf.to_owned()).or_insert(before);
        }
        let was_own = self.0.anims.borrow().get(leaf).is_some_and(|s| s.overridden);
        self.0.anims.borrow_mut().insert(
            leaf.to_owned(),
            AnimState {
                overridden: true,
                anim: anim.clone(),
            },
        );
        if self.0.baseline_anims.borrow().get(leaf) == Some(&anim) && was_own {
            self.0.overrides.borrow_mut().animations.remove(leaf);
        } else {
            self.0.overrides.borrow_mut().animations.insert(leaf.to_owned(), anim.clone());
        }
        self.0.pending_anims.borrow_mut().insert(leaf.to_owned(), anim);
        self.schedule_apply();
        self.schedule_save();
        // Children without own settings change too.
        self.notify(Some(&format!("anim:{leaf}")));
    }

    pub fn available(&self) -> bool {
        self.0.loaded.get()
    }

    pub fn info(&self, name: &str) -> Option<OptionInfo> {
        self.0.infos.borrow().get(name).cloned()
    }

    pub fn infos(&self) -> Vec<OptionInfo> {
        self.0.infos.borrow().values().cloned().collect()
    }

    pub fn value(&self, name: &str) -> Option<Value> {
        self.0.current.borrow().get(name).cloned()
    }

    /// `name` is an option or `anim:<leaf>`.
    pub fn is_overridden(&self, name: &str) -> bool {
        let o = self.0.overrides.borrow();
        if let Some(leaf) = name.strip_prefix("anim:") {
            return o.animations.contains_key(leaf);
        }
        if let Some(out) = name.strip_prefix("monitor:") {
            return o.monitors.contains_key(out);
        }
        o.options.contains_key(name)
    }

    /// Everything vela overrides (options and `anim:<leaf>`), for "reset all".
    pub fn overridden(&self) -> Vec<String> {
        let o = self.0.overrides.borrow();
        o.options
            .keys()
            .cloned()
            .chain(o.animations.keys().map(|l| format!("anim:{l}")))
            .chain(o.monitors.keys().map(|m| format!("monitor:{m}")))
            .collect()
    }

    pub fn remembered(&self, key: &str) -> Option<i64> {
        self.0.overrides.borrow().remember.get(key).copied()
    }

    pub fn remember(&self, key: &str, v: i64) {
        let changed = self.0.overrides.borrow_mut().remember.insert(key.to_owned(), v) != Some(v);
        if changed {
            self.schedule_save();
        }
    }

    /// True while listeners run: widget handlers must not write back.
    pub fn notifying(&self) -> bool {
        self.0.notifying.get()
    }

    pub fn subscribe(&self, f: impl Fn(Option<&str>) + 'static) {
        self.0.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn subscribe_errors(&self, f: impl Fn(Option<&str>) + 'static) {
        self.0.error_listeners.borrow_mut().push(Box::new(f));
    }

    fn notify(&self, name: Option<&str>) {
        let was = self.0.notifying.replace(true);
        for l in self.0.listeners.borrow().iter() {
            l(name);
        }
        self.0.notifying.set(was);
    }

    fn set_error(&self, err: Option<&str>) {
        for l in self.0.error_listeners.borrow().iter() {
            l(err);
        }
    }

    /// Overrides an option, applies it live and saves it.
    pub fn set(&self, name: &str, value: Value) {
        let Some(info) = self.info(name) else {
            log::warn!("unknown Hyprland option {name}");
            return;
        };
        let Some(value) = value.coerce(info.kind) else {
            log::warn!("value of the wrong type for {name}");
            return;
        };
        let unchanged = self.value(name).as_ref() == Some(&value) && self.is_overridden(name);
        if unchanged {
            return;
        }
        if !self.is_overridden(name)
            && let Some(before) = self.value(name)
        {
            self.0.baseline.borrow_mut().entry(name.to_owned()).or_insert(before);
        }
        self.0.current.borrow_mut().insert(name.to_owned(), value.clone());
        if self.0.baseline.borrow().get(name) == Some(&value) {
            // Back where hyprland.lua had it: nothing to override any more.
            self.0.overrides.borrow_mut().options.remove(name);
        } else {
            self.0.overrides.borrow_mut().options.insert(name.to_owned(), value.clone());
        }
        self.0.pending_apply.borrow_mut().insert(name.to_owned(), value);
        self.schedule_apply();
        self.schedule_save();
        self.notify(Some(name));
    }

    fn schedule_apply(&self) {
        if self.0.apply_source.borrow().is_some() {
            return;
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(16), move || {
            this.0.apply_source.borrow_mut().take();
            this.apply_now();
        });
        *self.0.apply_source.borrow_mut() = Some(id);
    }

    fn apply_now(&self) {
        let pending = std::mem::take(&mut *self.0.pending_apply.borrow_mut());
        let anims = std::mem::take(&mut *self.0.pending_anims.borrow_mut());
        let mut code: Vec<String> = pending.iter().map(|(n, v)| crate::hyprconf::eval_code(n, v)).collect();
        if !anims.is_empty() {
            // A reload of hyprland.lua without vela.lua would drop vela's curves.
            code.extend(crate::hypranim::curves_lua());
        }
        code.extend(anims.iter().map(|(leaf, a)| a.eval_code(leaf)));
        code.extend(self.0.pending_monitors.borrow_mut().drain(..).map(|r| r.eval_code()));
        match hyprland::eval(&code) {
            Ok(()) => self.set_error(None),
            Err(e) => {
                log::warn!("applying Hyprland options failed: {e}");
                self.set_error(Some(&format!("Hyprland: {e}")));
            }
        }
    }

    /// Drops the overrides; Hyprland reloads its config to get the values
    /// from hyprland.lua back.
    pub fn reset(&self, names: &[String]) {
        let removed = {
            let mut o = self.0.overrides.borrow_mut();
            names
                .iter()
                .filter(|n| {
                    if let Some(leaf) = n.strip_prefix("anim:") {
                        o.animations.remove(leaf).is_some()
                    } else if let Some(out) = n.strip_prefix("monitor:") {
                        o.monitors.remove(out).is_some()
                    } else {
                        o.options.remove(n.as_str()).is_some()
                    }
                })
                .count()
        };
        let monitors = names.iter().any(|n| n.starts_with("monitor:"));
        if removed == 0 {
            return;
        }
        for n in names {
            match n.strip_prefix("anim:") {
                Some(leaf) => {
                    self.0.pending_anims.borrow_mut().remove(leaf);
                }
                None => {
                    self.0.pending_apply.borrow_mut().remove(n);
                }
            }
        }
        self.save_now();
        let reloaded = if monitors { hyprland::reload_all() } else { hyprland::reload_config() };
        if !reloaded {
            self.set_error(Some("Hyprland did not reload its config"));
        }
        for n in names {
            self.0.baseline.borrow_mut().remove(n);
            if let Some(leaf) = n.strip_prefix("anim:") {
                self.0.baseline_anims.borrow_mut().remove(leaf);
            }
        }
        self.refresh_from_hyprland();
        self.notify(None);
    }

    fn schedule_save(&self) {
        if let Some(id) = self.0.save_source.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
            this.0.save_source.borrow_mut().take();
            this.save_now();
        });
        *self.0.save_source.borrow_mut() = Some(id);
    }

    pub fn save_now(&self) {
        if let Some(id) = self.0.save_source.borrow_mut().take() {
            id.remove();
        }
        let (text, lua) = {
            let o = self.0.overrides.borrow();
            (o.to_toml(), o.to_lua())
        };
        // The Lua first: a reload triggered by the toml change must see it.
        let result = write_atomic(&self.0.lua_path, &lua).and_then(|()| write_atomic(&self.0.toml_path, &text));
        match result {
            Ok(()) => *self.0.last_written.borrow_mut() = text,
            Err(e) => {
                log::error!("saving Hyprland settings failed: {e:#}");
                self.set_error(Some(&format!("Saving failed: {e:#}")));
            }
        }
    }

    /// Picks up hand edits of hyprland.toml.
    pub fn watch(&self) {
        let Some(dir) = self.0.toml_path.parent() else { return };
        let _ = std::fs::create_dir_all(dir);
        let Ok(monitor) = gio::File::for_path(dir).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) else {
            return;
        };
        let name = self.0.toml_path.file_name().map(|n| n.to_owned());
        let weak = Rc::downgrade(&self.0);
        let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        monitor.connect_changed(move |_, f, other, _| {
            if ![Some(f), other].into_iter().flatten().any(|f| f.basename().map(|b| b.into_os_string()) == name) {
                return;
            }
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            let (weak, d) = (weak.clone(), debounce.clone());
            *debounce.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(200), move || {
                d.borrow_mut().take();
                if let Some(inner) = weak.upgrade() {
                    HyprStore(inner).reload_from_disk();
                }
            }));
        });
        *self.0.monitor.borrow_mut() = Some(monitor);
    }

    fn reload_from_disk(&self) {
        let text = std::fs::read_to_string(&self.0.toml_path).unwrap_or_default();
        if text == *self.0.last_written.borrow() {
            return;
        }
        match Overrides::from_toml(&text) {
            Ok(o) => {
                *self.0.last_written.borrow_mut() = text;
                *self.0.overrides.borrow_mut() = o;
                self.set_error(None);
                // Also writes the Lua; the reload drops removed overrides.
                let lua = self.0.overrides.borrow().to_lua();
                if let Err(e) = write_atomic(&self.0.lua_path, &lua) {
                    log::error!("cannot write {}: {e:#}", self.0.lua_path.display());
                }
                hyprland::reload_config();
                if self.0.loaded.get() {
                    self.refresh_from_hyprland();
                }
                self.notify(None);
            }
            Err(e) => self.set_error(Some(&format!("hyprland.toml has an error, keeping previous settings: {e:#}"))),
        }
    }
}
