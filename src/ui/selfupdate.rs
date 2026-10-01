//! vela's own updates in the daemon: background check for a new release,
//! a desktop notification once per version, and the run of install.sh
//! (which restarts this daemon at its end; the next one reports the result).

use super::store::ConfigStore;
use crate::components::Installed;
use crate::selfupdate::{self, How, Outcome, Release, Status, VERSION};
use crate::update::{self, Event};
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub status: Status,
    pub checking: bool,
    /// (index, count, title) of the running step.
    pub step: Option<(usize, usize, String)>,
}

impl State {
    pub fn available(&self) -> Option<&Release> {
        self.status.latest.as_ref().filter(|r| selfupdate::is_newer(&r.version, VERSION))
    }

    pub fn running(&self) -> bool {
        self.step.is_some()
    }
}

type Listener = Box<dyn Fn(&State)>;

pub struct SelfUpdater {
    store: ConfigStore,
    use_scope: bool,
    pub how: How,
    state: RefCell<State>,
    listeners: RefCell<Vec<Listener>>,
    open_page: RefCell<Option<Rc<dyn Fn()>>>,
}

/// NixOS mode: Home Manager leaves xdph.conf to vela, which follows the
/// selection (the picker's store path changes with every update).
fn sync_share_picker() {
    let picker = crate::components::has(crate::components::Component::SharePicker)
        .then(|| std::env::current_exe().ok().map(|e| e.with_file_name("vela-share-picker")))
        .flatten();
    match crate::xdph::sync(&crate::xdph::path(), picker.as_deref()) {
        Ok(false) => {}
        Ok(true) => {
            log::info!("share picker: updated {}", crate::xdph::path().display());
            // xdph reads its config only at start.
            std::thread::spawn(|| {
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "try-restart", "xdg-desktop-portal-hyprland.service"])
                    .stdin(std::process::Stdio::null())
                    .status();
            });
        }
        Err(e) => log::warn!("share picker: {e}"),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// An install that left no word for 15 minutes has ended.
fn log_is_stale(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > Duration::from_secs(15 * 60))
}

impl SelfUpdater {
    pub fn new(store: ConfigStore, use_scope: bool) -> Rc<SelfUpdater> {
        let status: Status = std::fs::read_to_string(selfupdate::status_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let u = Rc::new(SelfUpdater {
            store,
            use_scope,
            how: selfupdate::detect(),
            state: RefCell::new(State { status, ..State::default() }),
            listeners: RefCell::default(),
            open_page: RefCell::default(),
        });
        u.report_restart();
        if u.how == How::Nix && crate::nixos::active().is_some() {
            sync_share_picker();
        }
        let weak = Rc::downgrade(&u);
        glib::timeout_add_local(Duration::from_secs(60), move || {
            let Some(u) = weak.upgrade() else { return glib::ControlFlow::Break };
            let hours = u.store.get().self_update.check_interval_hours;
            let checked = u.state.borrow().status.checked_at;
            if hours > 0 && now_ms() - checked >= i64::from(hours) * 3_600_000 {
                u.check();
            }
            glib::ControlFlow::Continue
        });
        u
    }

    /// install.sh restarted the daemon that started it: tell how it went.
    fn report_restart(self: &Rc<Self>) {
        let installing = self.state.borrow().status.installing.clone();
        let finished = installing.as_ref().is_some_and(|(_, log)| log_is_stale(log));
        match selfupdate::outcome(installing.as_ref(), VERSION, finished) {
            Outcome::None => return,
            Outcome::Updated(v) => {
                log::info!("self-update: now running vela {v}");
                self.notify(&format!("vela {v} installed"), "vela was updated and restarted.", &[]);
            }
            Outcome::Failed(v, log) => {
                log::warn!("self-update to {v} did not finish, see {}", log.display());
                self.state.borrow_mut().status.failed = Some((format!("Installing vela {v} did not finish"), log));
            }
        }
        self.state.borrow_mut().status.installing = None;
        self.changed();
    }

    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self, f: impl Fn(&State) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn set_open_page(&self, f: impl Fn() + 'static) {
        *self.open_page.borrow_mut() = Some(Rc::new(f));
    }

    fn changed(&self) {
        let state = self.state.borrow().clone();
        for l in self.listeners.borrow().iter() {
            l(&state);
        }
        let json = serde_json::to_string(&state.status).unwrap_or_default();
        if let Err(e) = crate::config::write_atomic(&selfupdate::status_file(), &json) {
            log::warn!("self-update: cannot write status: {e:#}");
        }
    }

    /// Asks GitHub for the latest release (on a worker thread).
    pub fn check(self: &Rc<Self>) {
        if std::mem::replace(&mut self.state.borrow_mut().checking, true) {
            return;
        }
        self.changed();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(selfupdate::fetch_latest());
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(result) = rx.recv().await else { return };
            let Some(u) = weak.upgrade() else { return };
            {
                let mut s = u.state.borrow_mut();
                s.checking = false;
                s.status.checked_at = now_ms();
                match result {
                    Ok(r) => {
                        s.status.latest = Some(r);
                        s.status.check_error = None;
                    }
                    Err(e) => s.status.check_error = Some(e),
                }
            }
            u.changed();
            u.notify_available();
        });
    }

    /// Once per new version.
    fn notify_available(self: &Rc<Self>) {
        let Some(version) = self.state.borrow().available().map(|r| r.version.clone()) else {
            return;
        };
        if !self.store.get().self_update.notify || self.state.borrow().status.notified.as_deref() == Some(version.as_str()) {
            return;
        }
        self.state.borrow_mut().status.notified = Some(version.clone());
        self.changed();
        let (body, actions): (&str, &[(&str, &str)]) = match self.how {
            How::Script { .. } => ("Your components and settings stay as they are.", &[("update", "Update now"), ("open", "Show")]),
            How::Nix => ("Update the vela input of your flake and rebuild.", &[("open", "Show")]),
            How::Package => ("Update it with your package manager.", &[("open", "Show")]),
        };
        self.notify(&format!("vela {version} is available"), body, actions);
    }

    /// notify-send in a thread; the chosen action comes back here.
    fn notify(self: &Rc<Self>, title: &str, body: &str, actions: &[(&str, &str)]) {
        let Some(notify) = crate::paths::find_executable("notify-send") else { return };
        let mut argv = vec![notify.to_string_lossy().into_owned(), "--app-name=Vela".into(), "--icon=vela".into()];
        for (id, label) in actions {
            argv.extend(["-A".to_owned(), format!("{id}={label}")]);
        }
        argv.extend([title.to_owned(), body.to_owned()]);
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let out = std::process::Command::new(&argv[0]).args(&argv[1..]).output();
            let _ = tx.send_blocking(out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default());
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(action) = rx.recv().await else { return };
            let Some(u) = weak.upgrade() else { return };
            let open = u.open_page.borrow().clone();
            match action.as_str() {
                "update" => {
                    if let Err(e) = u.update() {
                        log::warn!("self-update: {e}");
                        if let Some(f) = open {
                            f();
                        }
                    }
                }
                "open" => {
                    if let Some(f) = open {
                        f();
                    }
                }
                _ => {}
            }
        });
    }

    /// The latest release with the installed selection.
    pub fn update(self: &Rc<Self>) -> Result<(), String> {
        let release = self.state.borrow().available().cloned().ok_or("vela is up to date")?;
        self.install(release, None)
    }

    /// This version again, with another selection of components. In NixOS
    /// mode the selection is saved in the state directory and applied as is.
    pub fn change_components(self: &Rc<Self>, target: &Installed) -> Result<(), String> {
        if self.how == How::Nix {
            let mode = crate::nixos::active().ok_or("Set programs.vela.nixos.stateDir to choose the parts here")?;
            if self.state.borrow().running() {
                return Err("vela is already being installed".into());
            }
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let vela = exe.with_file_name("vela");
            let in_hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
            let steps = selfupdate::nix_component_steps(crate::components::installed(), target, &vela, in_hyprland);
            let path = crate::components::save_state_manifest(mode, target)?;
            log::info!("components: saved {}", path.display());
            return self.run(VERSION.to_owned(), steps);
        }
        self.install(selfupdate::release_of(VERSION, std::env::consts::ARCH), Some(target))
    }

    fn install(self: &Rc<Self>, release: Release, target: Option<&Installed>) -> Result<(), String> {
        let How::Script { nixos } = self.how else {
            return Err("vela was not installed with install.sh".into());
        };
        if self.state.borrow().running() {
            return Err("vela is already being installed".into());
        }
        let dir = selfupdate::work_dir();
        // A download cache only: unpacked releases from earlier runs.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let steps = selfupdate::plan(&release, nixos, &dir, target)?;
        self.run(release.version, steps)
    }

    /// Runs the steps on a worker thread; the last one usually restarts vela.
    fn run(self: &Rc<Self>, version: String, steps: Vec<update::Step>) -> Result<(), String> {
        let log_path = update::new_log_path(&crate::paths::state_dir().join("self-update-logs"), (now_ms() / 1000) as u64, 10);
        {
            let mut s = self.state.borrow_mut();
            s.status.failed = None;
            s.status.installing = Some((version.clone(), log_path.clone()));
            s.step = Some((0, steps.len(), steps[0].title.clone()));
        }
        self.changed();

        enum Msg {
            Step(usize, usize, String),
            Done(Result<(), update::Failure>),
        }
        let (tx, rx) = async_channel::unbounded();
        let use_scope = self.use_scope;
        std::thread::spawn(move || {
            let result = update::run(&format!("vela {}", version), &steps, &log_path, use_scope, &mut |e| {
                if let Event::Step { index, count, title } = e {
                    let _ = tx.send_blocking(Msg::Step(index, count, title));
                }
            });
            let _ = tx.send_blocking(Msg::Done(result.map_err(|f| *f)));
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                let Some(u) = weak.upgrade() else { return };
                match msg {
                    Msg::Step(i, n, title) => {
                        u.state.borrow_mut().step = Some((i, n, title));
                        u.changed();
                    }
                    // Usually not reached on success: install.sh restarts vela first.
                    Msg::Done(result) => {
                        {
                            let mut s = u.state.borrow_mut();
                            s.step = None;
                            s.status.installing = None;
                            s.status.failed = result.as_ref().err().map(|f| (format!("{} failed: {}", f.step, f.summary), f.log_path.clone()));
                        }
                        u.changed();
                        match result {
                            Ok(()) => u.notify("vela installed", "Restart vela (or log in again) to use it.", &[]),
                            Err(f) => u.notify(
                                &format!("Updating vela failed: {}", f.step),
                                f.summary.lines().next().unwrap_or_default(),
                                &[("open", "Show")],
                            ),
                        }
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    /// Home Manager: `nix flake update vela` and the rebuild, in a terminal,
    /// in the NixOS repository that holds vela's state directory.
    pub fn update_nix(&self) -> Result<(), String> {
        let mode = crate::nixos::active().ok_or("Set programs.vela.nixos.stateDir, or update the vela input of your flake and rebuild")?;
        let words = shlex::split(&mode.rebuild).filter(|w| !w.is_empty()).ok_or("invalid rebuild command")?;
        let mut command = vec!["sh".to_owned(), "-c".to_owned(), selfupdate::NIX_UPDATE_SCRIPT.to_owned(), "sh".to_owned()];
        command.extend(words);
        let spec = crate::launch::SpawnSpec {
            argv: crate::launch::in_terminal(&self.store.get().terminal, command).map_err(|e| e.to_string())?,
            cwd: Some(mode.state_dir.clone()),
            env: Vec::new(),
            name: "vela-update".into(),
        };
        crate::launch::spawn_detached(&spec, false).map_err(|e| e.to_string())
    }
}
