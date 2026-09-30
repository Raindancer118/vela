//! Main-thread owner of the configuration. Every change goes through
//! `update`, is applied to all listeners immediately (live settings) and
//! written to disk shortly afterwards. External edits of the file are picked
//! up by a file monitor.

use crate::config::{self, Config};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

type Listener = Box<dyn Fn(&Config, &Config)>;
type ReplaceListener = Box<dyn Fn(&Config)>;
type ErrorListener = Box<dyn Fn(Option<&str>)>;

struct Inner {
    path: PathBuf,
    config: RefCell<Arc<Config>>,
    listeners: RefCell<Vec<Listener>>,
    replace_listeners: RefCell<Vec<ReplaceListener>>,
    error_listeners: RefCell<Vec<ErrorListener>>,
    pending_save: RefCell<Option<glib::SourceId>>,
    last_written: RefCell<String>,
    /// The file on disk could not be parsed; back it up before overwriting.
    broken_on_disk: Cell<bool>,
    monitor: RefCell<Option<gio::FileMonitor>>,
    last_error: RefCell<Option<String>>,
    /// The file was created with defaults during this start.
    created: Cell<bool>,
}

#[derive(Clone)]
pub struct ConfigStore(Rc<Inner>);

impl ConfigStore {
    pub fn load(path: PathBuf) -> ConfigStore {
        let (cfg, broken, error, created) = match config::load_or_create(&path) {
            Ok(config::LoadOutcome::Loaded(c)) => (c, false, None, false),
            Ok(config::LoadOutcome::Created(c)) => {
                log::info!("created default config at {}", path.display());
                (c, false, None, true)
            }
            Err(e) => {
                log::error!("{e:#}; using defaults");
                (Config::default(), path.exists(), Some(format!("{e:#}")), false)
            }
        };
        let last_written = std::fs::read_to_string(&path).unwrap_or_default();
        ConfigStore(Rc::new(Inner {
            path,
            config: RefCell::new(Arc::new(cfg)),
            listeners: RefCell::default(),
            replace_listeners: RefCell::default(),
            error_listeners: RefCell::default(),
            pending_save: RefCell::default(),
            last_written: RefCell::new(last_written),
            broken_on_disk: Cell::new(broken),
            monitor: RefCell::default(),
            last_error: RefCell::new(error),
            created: Cell::new(created),
        }))
    }

    pub fn get(&self) -> Arc<Config> {
        self.0.config.borrow().clone()
    }

    pub fn path(&self) -> PathBuf {
        self.0.path.clone()
    }

    /// True once, if this start created the config file.
    pub fn take_created(&self) -> bool {
        self.0.created.replace(false)
    }

    pub fn last_error(&self) -> Option<String> {
        self.0.last_error.borrow().clone()
    }

    /// Called with (old, new) after every change.
    pub fn subscribe(&self, f: impl Fn(&Config, &Config) + 'static) {
        self.0.listeners.borrow_mut().push(Box::new(f));
    }

    /// Called when the whole config was replaced from outside the settings
    /// widgets (file reload, restore defaults) so they can refresh.
    pub fn subscribe_replace(&self, f: impl Fn(&Config) + 'static) {
        self.0.replace_listeners.borrow_mut().push(Box::new(f));
    }

    pub fn subscribe_errors(&self, f: impl Fn(Option<&str>) + 'static) {
        self.0.error_listeners.borrow_mut().push(Box::new(f));
    }

    fn set(&self, mut new: Config) -> bool {
        new.sanitize();
        let old = self.get();
        if *old == new {
            return false;
        }
        let new = Arc::new(new);
        *self.0.config.borrow_mut() = new.clone();
        // Listeners may call back into the store (e.g. `get`), so iterate over
        // a snapshot-free borrow that doesn't hold the config borrow.
        for l in self.0.listeners.borrow().iter() {
            l(&old, &new);
        }
        true
    }

    /// Applies a change live and schedules saving it.
    pub fn update(&self, f: impl FnOnce(&mut Config)) {
        let mut c = (*self.get()).clone();
        f(&mut c);
        if self.set(c) {
            self.schedule_save();
        }
    }

    pub fn replace(&self, new: Config, save: bool) {
        let changed = self.set(new);
        let cfg = self.get();
        for l in self.0.replace_listeners.borrow().iter() {
            l(&cfg);
        }
        if changed && save {
            self.schedule_save();
        }
    }

    pub fn restore_defaults(&self) {
        self.replace(Config::default(), true);
    }

    fn schedule_save(&self) {
        if let Some(id) = self.0.pending_save.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
            this.0.pending_save.borrow_mut().take();
            this.save_now();
        });
        *self.0.pending_save.borrow_mut() = Some(id);
    }

    pub fn save_now(&self) {
        if let Some(id) = self.0.pending_save.borrow_mut().take() {
            id.remove();
        }
        if self.0.broken_on_disk.replace(false) {
            let backup = self.0.path.with_extension(format!("toml.broken-{}", crate::history::now_secs()));
            match std::fs::rename(&self.0.path, &backup) {
                Ok(()) => log::warn!("unparsable config backed up to {}", backup.display()),
                Err(e) => log::warn!("could not back up broken config: {e}"),
            }
        }
        let text = self.get().to_toml();
        match config::write_atomic(&self.0.path, &text) {
            Ok(()) => {
                *self.0.last_written.borrow_mut() = text;
                self.set_error(None);
            }
            Err(e) => {
                log::error!("saving config failed: {e:#}");
                self.set_error(Some(format!("Saving failed: {e:#}")));
            }
        }
    }

    fn set_error(&self, err: Option<String>) {
        *self.0.last_error.borrow_mut() = err.clone();
        for l in self.0.error_listeners.borrow().iter() {
            l(err.as_deref());
        }
    }

    /// Re-reads the file (e.g. `vela reload` or an external edit).
    pub fn reload_from_disk(&self) {
        let text = match std::fs::read_to_string(&self.0.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Deleted externally: recreate it from the current state.
                self.save_now();
                return;
            }
            Err(e) => {
                self.set_error(Some(format!("Cannot read config: {e}")));
                return;
            }
        };
        if text == *self.0.last_written.borrow() {
            return;
        }
        match Config::from_toml(&text) {
            Ok(cfg) => {
                log::info!("config reloaded from disk");
                *self.0.last_written.borrow_mut() = text;
                self.0.broken_on_disk.set(false);
                self.set_error(None);
                self.replace(cfg, false);
            }
            Err(e) => {
                log::warn!("ignoring invalid config edit: {e:#}");
                self.0.broken_on_disk.set(true);
                self.set_error(Some(format!("config.toml has an error, keeping previous settings: {e:#}")));
            }
        }
    }

    /// Watches the config directory (editors often replace the file).
    pub fn watch(&self) {
        let Some(dir) = self.0.path.parent() else { return };
        let file = gio::File::for_path(dir);
        let monitor = match file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("cannot watch config dir: {e}");
                return;
            }
        };
        let name = self.0.path.file_name().map(|n| n.to_owned());
        let weak = Rc::downgrade(&self.0);
        let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        monitor.connect_changed(move |_, f, other, _| {
            let relevant = [Some(f), other].into_iter().flatten().any(|f| f.basename().map(|b| b.into_os_string()) == name);
            if !relevant {
                return;
            }
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            let weak = weak.clone();
            let d = debounce.clone();
            *debounce.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(150), move || {
                d.borrow_mut().take();
                if let Some(inner) = weak.upgrade() {
                    ConfigStore(inner).reload_from_disk();
                }
            }));
        });
        *self.0.monitor.borrow_mut() = Some(monitor);
    }
}
