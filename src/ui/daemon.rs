//! The long-running process: owns the config store, the application catalog,
//! the search engine, the launcher overlay and the settings window, and
//! reacts to IPC commands.

use super::hypr_store::HyprStore;
use super::icons;
use super::launcher::{Launcher, Request};
use super::settings::SettingsWindow;
use super::store::ConfigStore;
use crate::apps::catalog::{Catalog, SETTINGS_KEY, Target};
use crate::apps::index::{self, App, ScanOptions};
use crate::history::{self, History};
use crate::ipc::{self, Command};
use crate::launch;
use crate::paths;
use crate::search::engine::{Engine, IndexStatus, Update};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

type StatusListener = Box<dyn Fn(&IndexStatus)>;
type CatalogListener = Box<dyn Fn(&Arc<Catalog>)>;

pub struct Daemon {
    pub app: adw::Application,
    pub store: ConfigStore,
    pub hypr: HyprStore,
    pub launcher: Rc<Launcher>,
    pub engine: Engine,
    pub catalog: RefCell<Arc<Catalog>>,
    scanned: RefCell<Arc<Vec<App>>>,
    history: RefCell<History>,
    pub index_status: RefCell<Option<IndexStatus>>,
    settings: RefCell<Option<Rc<SettingsWindow>>>,
    status_listeners: RefCell<Vec<StatusListener>>,
    catalog_listeners: RefCell<Vec<CatalogListener>>,
    app_monitors: RefCell<Vec<gio::FileMonitor>>,
    rescan_pending: RefCell<Option<glib::SourceId>>,
    use_scope: bool,
}

thread_local! {
    static DAEMON: RefCell<Option<Rc<Daemon>>> = const { RefCell::new(None) };
}

pub struct Options {
    pub initial: Option<Command>,
}

pub fn run(opts: Options) -> glib::ExitCode {
    let socket = paths::socket_path();
    let listener = match ipc::bind(&socket) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("vela: {e:#}");
            return glib::ExitCode::FAILURE;
        }
    };

    let app = adw::Application::builder()
        .application_id("io.github.raindancer118.Vela")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<Command>();
    ipc::serve(listener, cmd_tx);
    crate::claude_usage::spawn_poller();

    let cmd_rx = RefCell::new(Some(cmd_rx));
    app.connect_startup(move |app| {
        // A daemon without windows must not quit.
        std::mem::forget(app.hold());
        // The launcher has its own look; keep the settings window in the
        // system's colour scheme.
        icons::install_bundled();
        let daemon = Daemon::new(app);
        if let Some(rx) = cmd_rx.borrow_mut().take() {
            let d = daemon.clone();
            glib::spawn_future_local(async move {
                while let Ok(cmd) = rx.recv().await {
                    d.handle(cmd);
                }
            });
        }
        if let Some(cmd) = opts.initial {
            daemon.handle(cmd);
        }
        // Keep the daemon alive for the lifetime of the application.
        DAEMON.with(|d| *d.borrow_mut() = Some(daemon));
    });
    app.connect_activate(|_| {});

    let code = app.run_with_args::<&str>(&[]);
    let _ = std::fs::remove_file(&socket);
    code
}

impl Daemon {
    fn new(app: &adw::Application) -> Rc<Daemon> {
        let store = ConfigStore::load(paths::config_file());
        store.watch();
        let hypr = HyprStore::load();
        hypr.watch();
        let config = store.get();
        let (upd_tx, upd_rx) = async_channel::unbounded();
        let engine = Engine::new(upd_tx);
        engine.set_config(config.clone());
        let history = History::load(&paths::state_dir().join("history.json"));
        engine.set_history(Arc::new(history.clone()));

        let launcher = Launcher::new(app, config.clone());
        let use_scope = launch::systemd_scope_available();
        log::info!("systemd scopes for launched apps: {}", if use_scope { "available" } else { "unavailable" });

        let d = Rc::new(Daemon {
            app: app.clone(),
            store: store.clone(),
            hypr,
            launcher: launcher.clone(),
            engine,
            catalog: RefCell::new(Arc::new(Catalog::default())),
            scanned: RefCell::new(Arc::new(Vec::new())),
            history: RefCell::new(history),
            index_status: RefCell::default(),
            settings: RefCell::default(),
            status_listeners: RefCell::default(),
            catalog_listeners: RefCell::default(),
            app_monitors: RefCell::default(),
            rescan_pending: RefCell::default(),
            use_scope,
        });

        let weak = Rc::downgrade(&d);
        launcher.set_request_handler(move |req| {
            if let Some(d) = weak.upgrade() {
                d.on_request(req);
            }
        });

        let weak = Rc::downgrade(&d);
        store.subscribe(move |old, new| {
            let Some(d) = weak.upgrade() else { return };
            d.launcher.apply_config(old, new);
            d.engine.set_config(Arc::new(new.clone()));
            if old.apps.custom != new.apps.custom || old.apps.hidden != new.apps.hidden || old.apps.desktop_actions != new.apps.desktop_actions {
                d.rebuild_catalog();
            }
        });

        let weak = Rc::downgrade(&d);
        glib::spawn_future_local(async move {
            while let Ok(update) = upd_rx.recv().await {
                let Some(d) = weak.upgrade() else { break };
                match update {
                    Update::Apps { generation, catalog, hits } => d.launcher.on_app_results(generation, catalog, hits),
                    Update::Files { generation, hits, pending } => d.launcher.on_file_results(generation, hits, pending),
                    Update::Status(s) => {
                        for l in d.status_listeners.borrow().iter() {
                            l(&s);
                        }
                        *d.index_status.borrow_mut() = Some(s);
                    }
                }
            }
        });

        d.rescan_apps();
        d.watch_app_dirs();
        d
    }

    pub fn subscribe_status(&self, f: impl Fn(&IndexStatus) + 'static) {
        self.status_listeners.borrow_mut().push(Box::new(f));
    }

    pub fn subscribe_catalog(&self, f: impl Fn(&Arc<Catalog>) + 'static) {
        self.catalog_listeners.borrow_mut().push(Box::new(f));
    }

    pub fn handle(self: &Rc<Self>, cmd: Command) {
        match cmd {
            Command::Toggle => self.launcher.toggle(),
            Command::Show => self.launcher.show(false),
            Command::Hide => self.launcher.hide(),
            Command::Settings(page) => self.open_settings_page(page),
            Command::Reload => {
                self.store.reload_from_disk();
                self.rescan_apps();
                self.engine.rebuild_index();
            }
            Command::Quit => {
                self.store.save_now();
                self.app.quit();
            }
            Command::Ping => {}
        }
        if matches!(cmd, Command::Toggle | Command::Show) && self.launcher.is_visible() {
            self.engine.refresh_index_if_older(Duration::from_secs(60));
        }
    }

    // ---------------------------------------------------------------- apps

    /// Scans desktop entries on a worker thread.
    pub fn rescan_apps(self: &Rc<Self>) {
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let report = index::scan(&paths::data_dirs(), &ScanOptions::from_env());
            let _ = tx.send_blocking(report);
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(report) = rx.recv().await else { return };
            let Some(d) = weak.upgrade() else { return };
            for (path, err) in &report.malformed {
                log::debug!("skipping {}: {err}", path.display());
            }
            log::info!(
                "found {} applications ({} malformed entries skipped)",
                report.apps.len(),
                report.malformed.len()
            );
            *d.scanned.borrow_mut() = Arc::new(report.apps);
            d.rebuild_catalog();
            // First start: the default pin list names common apps; keep
            // only those that are actually installed.
            if d.store.take_created() {
                let catalog = d.catalog.borrow().clone();
                d.store.update(|c| c.apps.pinned.retain(|k| catalog.get(k).is_some()));
            }
        });
    }

    fn rebuild_catalog(&self) {
        let cfg = self.store.get();
        let catalog = Arc::new(Catalog::build(&self.scanned.borrow(), &cfg.apps));
        *self.catalog.borrow_mut() = catalog.clone();
        self.engine.set_catalog(catalog.clone());
        self.launcher.set_catalog(catalog.clone());
        for l in self.catalog_listeners.borrow().iter() {
            l(&catalog);
        }
    }

    fn watch_app_dirs(self: &Rc<Self>) {
        let mut monitors = Vec::new();
        for dir in paths::data_dirs() {
            let apps = dir.join("applications");
            if !apps.is_dir() {
                continue;
            }
            let Ok(m) = gio::File::for_path(&apps).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) else {
                continue;
            };
            let weak = Rc::downgrade(self);
            m.connect_changed(move |_, _, _, _| {
                let Some(d) = weak.upgrade() else { return };
                // Package installs touch many files at once: debounce.
                if let Some(id) = d.rescan_pending.borrow_mut().take() {
                    id.remove();
                }
                let weak = Rc::downgrade(&d);
                let id = glib::timeout_add_local_once(Duration::from_millis(800), move || {
                    if let Some(d) = weak.upgrade() {
                        d.rescan_pending.borrow_mut().take();
                        d.rescan_apps();
                    }
                });
                *d.rescan_pending.borrow_mut() = Some(id);
            });
            monitors.push(m);
        }
        *self.app_monitors.borrow_mut() = monitors;
    }

    // -------------------------------------------------------------- actions

    fn on_request(self: &Rc<Self>, req: Request) {
        match req {
            Request::OnMonitor(monitor, inner) => {
                if !crate::hyprland::focus_monitor(&monitor) {
                    log::warn!("could not focus monitor {monitor}");
                }
                self.perform(*inner);
            }
            Request::LaunchEntry(_) | Request::OpenPath { .. } | Request::Claude(_) => {
                // Apps open where the launcher is, also when that is the main
                // monitor rather than the focused one.
                if let Some(m) = self.launcher.monitor()
                    && crate::hyprland::focused_monitor().is_some_and(|f| f != m)
                    && !crate::hyprland::focus_monitor(&m)
                {
                    log::warn!("could not focus monitor {m}");
                }
                self.perform(req);
            }
            req => self.perform(req),
        }
    }

    fn perform(self: &Rc<Self>, req: Request) {
        match req {
            Request::Query(g, text) => self.engine.query(g, &text),
            Request::LaunchEntry(key) => self.launch_entry(&key),
            Request::OpenPath { path, reveal } => {
                let target = if reveal {
                    path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
                } else {
                    path
                };
                self.spawn(launch::open_path_spec(&target), None);
            }
            Request::Claude(prompt) => {
                let cfg = self.store.get();
                if !cfg.search.claude {
                    return;
                }
                self.spawn(launch::claude_spec(&cfg.claude, &cfg.terminal, &prompt), None);
            }
            Request::OpenSettings => self.open_settings(),
            Request::TogglePin(key) => self.store.update(|c| {
                if let Some(pos) = c.apps.pinned.iter().position(|k| *k == key) {
                    c.apps.pinned.remove(pos);
                } else {
                    c.apps.pinned.push(key);
                }
            }),
            Request::MovePin(key, delta) => self.store.update(|c| {
                if let Some(pos) = c.apps.pinned.iter().position(|k| *k == key) {
                    let new = (pos as i64 + i64::from(delta)).clamp(0, c.apps.pinned.len() as i64 - 1) as usize;
                    let k = c.apps.pinned.remove(pos);
                    c.apps.pinned.insert(new, k);
                }
            }),
            Request::Hidden => {}
            Request::OnMonitor(monitor, inner) => self.on_request(Request::OnMonitor(monitor, inner)),
        }
    }

    fn launch_entry(self: &Rc<Self>, key: &str) {
        let catalog = self.catalog.borrow().clone();
        let Some(entry) = catalog.get(key) else {
            self.launcher.show_error("This application is no longer installed.");
            return;
        };
        if matches!(entry.target, Target::Settings) || key == SETTINGS_KEY {
            self.open_settings();
            return;
        }
        let cfg = self.store.get();
        self.spawn(launch::entry_spec(entry, &cfg.terminal), Some(key));
    }

    fn spawn(self: &Rc<Self>, spec: Result<launch::SpawnSpec, launch::LaunchError>, history_key: Option<&str>) {
        let mut spec = match spec {
            Ok(s) => s,
            Err(e) => {
                log::warn!("launch failed: {e}");
                self.launcher.show_error(&e.to_string());
                return;
            }
        };
        // Lets the compositor focus the new window (xdg-activation).
        if let Some(display) = gtk::gdk::Display::default() {
            let ctx = display.app_launch_context();
            if let Some(token) = ctx.startup_notify_id(None::<&gio::AppInfo>, &[]) {
                spec.env.push(("XDG_ACTIVATION_TOKEN".into(), token.to_string()));
                spec.env.push(("DESKTOP_STARTUP_ID".into(), token.to_string()));
            }
        }
        let use_scope = self.use_scope && self.store.get().general.systemd_scope;
        match launch::spawn_detached(&spec, use_scope) {
            Ok(()) => {
                log::info!("launched {:?}", spec.argv.first());
                if let Some(key) = history_key {
                    let snapshot = {
                        let mut h = self.history.borrow_mut();
                        h.record(key, history::now_secs());
                        h.clone()
                    };
                    self.engine.set_history(Arc::new(snapshot.clone()));
                    // Persist off the UI thread.
                    std::thread::spawn(move || {
                        if let Err(e) = snapshot.save(&paths::state_dir().join("history.json")) {
                            log::warn!("saving history failed: {e:#}");
                        }
                    });
                }
                self.launcher.hide();
            }
            Err(e) => {
                log::warn!("launch failed: {e}");
                self.launcher.show_error(&e.to_string());
            }
        }
    }

    /// Starts Claude Code with a prompt from the settings search; the error
    /// is for the caller to show (the launcher is hidden then).
    pub fn ask_claude(self: &Rc<Self>, prompt: &str) -> Result<(), String> {
        let cfg = self.store.get();
        // The vela tools without a permission prompt each; before any `--`.
        let mut claude = cfg.claude.clone();
        let at = claude.args.iter().position(|a| a == "--").unwrap_or(claude.args.len());
        claude.args.splice(at..at, ["--allowedTools".to_owned(), "mcp__vela".to_owned()]);
        let spec = launch::claude_spec(&claude, &cfg.terminal, prompt).map_err(|e| e.to_string())?;
        self.spawn(Ok(spec), None);
        Ok(())
    }

    pub fn open_settings(self: &Rc<Self>) {
        self.open_settings_page(None);
    }

    pub fn open_settings_page(self: &Rc<Self>, page: Option<&str>) {
        self.launcher.hide();
        let existing = self.settings.borrow().clone();
        let win = match existing {
            Some(w) => w,
            None => {
                let w = SettingsWindow::new(self);
                *self.settings.borrow_mut() = Some(w.clone());
                w
            }
        };
        // Without a page always the search, also when the window was only hidden.
        win.show_page(page.unwrap_or("home"));
        win.present();
    }
}
