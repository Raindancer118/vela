//! System updates in the daemon: background checks, the running update and
//! "Fix with Claude". The settings page only shows this state; the control
//! center reads the same state from `updates.json` (`vela updates watch`).

use super::fingerprint::FingerprintPrompt;
use super::store::ConfigStore;
use crate::update::{self, Event, Failure, Mode, Status};
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// The log shown in the window; the file has everything.
const LOG_LIMIT: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeFix {
    Idle,
    /// Started; the address once its window showed up.
    Started(Option<String>),
}

#[derive(Debug, Clone)]
pub struct Run {
    pub label: String,
    /// (index, count, title) of the current step.
    pub step: (usize, usize, String),
    pub log: String,
    pub log_path: PathBuf,
    pub result: Option<Result<(), Failure>>,
    pub claude: ClaudeFix,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub status: Status,
    pub run: Option<Run>,
}

impl State {
    pub fn running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.result.is_none())
    }
}

type Listener = Box<dyn Fn(&State)>;
type LogListener = Box<dyn Fn(&str)>;

pub struct Updates {
    store: ConfigStore,
    use_scope: bool,
    state: RefCell<State>,
    listeners: RefCell<Vec<Listener>>,
    log_listeners: RefCell<Vec<LogListener>>,
    open_page: RefCell<Option<Rc<dyn Fn()>>>,
    /// Whether someone looks at the page (no desktop notification then).
    page_visible: Cell<bool>,
    fingerprint: RefCell<Option<Rc<FingerprintPrompt>>>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Updates {
    pub fn new(store: ConfigStore, use_scope: bool) -> Rc<Updates> {
        // The last known list right away; running/failed don't survive a restart.
        let mut status: Status = std::fs::read_to_string(update::status_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        status.checking = false;
        status.running = None;
        status.failed = None;
        let u = Rc::new(Updates {
            store,
            use_scope,
            state: RefCell::new(State { status, run: None }),
            listeners: RefCell::default(),
            log_listeners: RefCell::default(),
            open_page: RefCell::default(),
            page_visible: Cell::new(false),
            fingerprint: RefCell::default(),
        });
        u.write_status();
        let weak = Rc::downgrade(&u);
        // Due checks: shortly after login, then whenever the interval is over.
        glib::timeout_add_local(Duration::from_secs(20), move || {
            let Some(u) = weak.upgrade() else { return glib::ControlFlow::Break };
            let hours = u.store.get().updates.check_interval_hours;
            let checked = u.state.borrow().status.checked_at;
            if hours > 0 && now_ms() - checked >= i64::from(hours) * 3_600_000 {
                u.check();
            }
            glib::ControlFlow::Continue
        });
        u
    }

    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self, f: impl Fn(&State) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn subscribe_log(&self, f: impl Fn(&str) + 'static) {
        self.log_listeners.borrow_mut().push(Box::new(f));
    }

    pub fn set_open_page(&self, f: impl Fn() + 'static) {
        *self.open_page.borrow_mut() = Some(Rc::new(f));
    }

    pub fn set_page_visible(&self, v: bool) {
        self.page_visible.set(v);
    }

    fn changed(&self) {
        let state = self.state.borrow().clone();
        for l in self.listeners.borrow().iter() {
            l(&state);
        }
        self.write_status();
    }

    /// Synchronous: a few hundred bytes, and writer threads would race for
    /// the same temporary file.
    fn write_status(&self) {
        let json = serde_json::to_string(&self.state.borrow().status).unwrap_or_default();
        if let Err(e) = crate::config::write_atomic(&update::status_file(), &json) {
            log::warn!("updates: cannot write status: {e:#}");
        }
    }

    /// pam_fprintd's request shows the fingerprint prompt; the next line
    /// says whether the finger was accepted.
    fn on_output_lines(&self, text: &str) {
        for line in text.lines() {
            let waiting = self.state.borrow().status.fingerprint;
            if update::is_fingerprint_request(line) {
                self.prompt().show(line);
                if !waiting {
                    self.state.borrow_mut().status.fingerprint = true;
                    self.changed();
                }
            } else if waiting && !line.trim().is_empty() {
                self.prompt().finish(!update::fingerprint_failed(line));
                self.state.borrow_mut().status.fingerprint = false;
                self.changed();
            }
        }
    }

    fn prompt(&self) -> Rc<FingerprintPrompt> {
        self.fingerprint
            .borrow_mut()
            .get_or_insert_with(|| FingerprintPrompt::new(self.store.clone()))
            .clone()
    }

    fn end_fingerprint(&self) {
        if let Some(p) = self.fingerprint.borrow().as_ref() {
            p.hide();
        }
        self.state.borrow_mut().status.fingerprint = false;
    }

    /// Asks every source what is pending (on a worker thread).
    pub fn check(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            if s.status.checking || s.running() {
                return;
            }
            s.status.checking = true;
        }
        self.changed();
        let tools = update::Tools::detect(&self.store.get().updates);
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(update::check(&tools));
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok((pending, errors)) = rx.recv().await else { return };
            let Some(u) = weak.upgrade() else { return };
            {
                let mut s = u.state.borrow_mut();
                s.status.checking = false;
                s.status.checked_at = now_ms();
                s.status.pending = pending;
                s.status.check_errors = errors;
            }
            u.changed();
        });
    }

    /// Starts an update; the error is for the caller to show.
    pub fn start(self: &Rc<Self>, mode: Mode) -> Result<(), String> {
        if self.state.borrow().running() {
            return Err("An update is already running".into());
        }
        let steps = update::plan(&mode, &update::Tools::detect(&self.store.get().updates))?;
        // vela.service rarely has SUDO_ASKPASS (shells set it in their own config).
        let askpass = update::find_askpass(std::env::var_os("SUDO_ASKPASS").as_deref());
        let needs_sudo = steps.iter().any(|s| s.argv.iter().any(|a| a == "-A"));
        if needs_sudo && askpass.is_none() {
            return Err("No password dialog for sudo found: install ksshaskpass (or another askpass) or set SUDO_ASKPASS".into());
        }
        let env: Vec<(String, String)> = askpass
            .map(|p| ("SUDO_ASKPASS".to_owned(), p.to_string_lossy().into_owned()))
            .into_iter()
            .collect();
        let secs = (now_ms() / 1000) as u64;
        let log_path = update::new_log_path(&update::log_dir(), secs, 20);
        let label = mode.label().to_owned();
        {
            let mut s = self.state.borrow_mut();
            s.status.failed = None;
            s.status.running = steps.first().map(|st| st.title.clone());
            s.run = Some(Run {
                label: label.clone(),
                step: (0, steps.len(), steps.first().map(|st| st.title.clone()).unwrap_or_default()),
                log: String::new(),
                log_path: log_path.clone(),
                result: None,
                claude: ClaudeFix::Idle,
            });
        }
        self.changed();

        enum Msg {
            Event(Event),
            Done(Result<(), Failure>),
        }
        let (tx, rx) = async_channel::unbounded();
        let use_scope = self.use_scope;
        std::thread::spawn(move || {
            let result = update::run_with_env(&label, &steps, &log_path, use_scope, &env, &mut |e| {
                let _ = tx.send_blocking(Msg::Event(e));
            })
            .map_err(|f| *f);
            let _ = tx.send_blocking(Msg::Done(result));
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                let Some(u) = weak.upgrade() else { return };
                match msg {
                    Msg::Event(Event::Output(text)) => {
                        if let Some(run) = u.state.borrow_mut().run.as_mut() {
                            run.log.push_str(&text);
                            if run.log.len() > LOG_LIMIT {
                                let cut = run.log.len() - LOG_LIMIT / 2;
                                let cut = (cut..run.log.len()).find(|i| run.log.is_char_boundary(*i)).unwrap_or(0);
                                run.log.drain(..cut);
                            }
                        }
                        for l in u.log_listeners.borrow().iter() {
                            l(&text);
                        }
                        u.on_output_lines(&text);
                    }
                    Msg::Event(Event::Step { index, count, title }) => {
                        u.end_fingerprint();
                        {
                            let mut s = u.state.borrow_mut();
                            s.status.running = Some(title.clone());
                            if let Some(run) = s.run.as_mut() {
                                run.step = (index, count, title);
                            }
                        }
                        u.changed();
                    }
                    Msg::Done(result) => {
                        u.end_fingerprint();
                        {
                            let mut s = u.state.borrow_mut();
                            s.status.running = None;
                            s.status.failed = result.as_ref().err().map(|f| f.summary.clone());
                            if let Some(run) = s.run.as_mut() {
                                run.result = Some(result.clone());
                            }
                        }
                        u.changed();
                        u.notify_finished(&result);
                        // Fresh list: what is left (or what the database sync revealed).
                        u.check();
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    /// Desktop notification when nobody watches the page; a failure offers
    /// to open the page or to let Claude fix it right away.
    fn notify_finished(self: &Rc<Self>, result: &Result<(), Failure>) {
        if self.page_visible.get() {
            return;
        }
        let Some(notify) = crate::paths::find_executable("notify-send") else { return };
        let mut argv = vec![notify.to_string_lossy().into_owned(), "--app-name=Vela".into(), "--icon=vela".into()];
        match result {
            Ok(()) => argv.extend(["Updates installed".to_owned(), "Everything finished without errors.".to_owned()]),
            Err(f) => argv.extend([
                "-A".into(),
                "open=Show".into(),
                "-A".into(),
                "fix=Fix with Claude".into(),
                "--urgency=critical".into(),
                format!("{} failed", f.step),
                f.summary.lines().next().unwrap_or_default().to_owned(),
            ]),
        }
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let out = std::process::Command::new(&argv[0]).args(&argv[1..]).output();
            let action = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
            let _ = tx.send_blocking(action);
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(action) = rx.recv().await else { return };
            let Some(u) = weak.upgrade() else { return };
            let open = u.open_page.borrow().clone();
            match action.as_str() {
                "open" => {
                    if let Some(f) = open {
                        f();
                    }
                }
                "fix" => {
                    if let Err(e) = u.fix_with_claude() {
                        log::warn!("updates: Claude fix failed to start: {e}");
                        if let Some(f) = open {
                            f();
                        }
                    }
                }
                _ => {}
            }
        });
    }

    /// Starts Claude Code with the failure in a terminal on the configured
    /// workspace, without switching there.
    pub fn fix_with_claude(self: &Rc<Self>) -> Result<(), String> {
        let failure = match self.state.borrow().run.as_ref() {
            Some(r) if r.claude != ClaudeFix::Idle => return Err("Claude is already working on it".into()),
            Some(Run { result: Some(Err(f)), .. }) => f.clone(),
            _ => return Err("There is no failed update to fix".into()),
        };
        let log = std::fs::read_to_string(&failure.log_path).unwrap_or_default();
        let prompt = update::fix_prompt(&failure, update::tail(&log, 80).trim_end());
        let cfg = self.store.get();
        let askpass = update::find_askpass(std::env::var_os("SUDO_ASKPASS").as_deref());
        let workspace = cfg.updates.claude_workspace.clone();
        let command = update::claude_shell_command(&cfg.claude, &cfg.terminal, &prompt, askpass.as_deref())?;
        let before = crate::hyprland::clients_json().map(|j| update::windows_on(&j, &workspace));
        if let Err(e) = crate::hyprland::eval(&[update::exec_lua(&command, &workspace)]) {
            // Not under Hyprland: an ordinary window then.
            log::info!("updates: hyprland exec failed ({e}), starting Claude directly");
            let mut spec = crate::launch::claude_spec(&cfg.claude, &cfg.terminal, &prompt).map_err(|e| e.to_string())?;
            if let Some(p) = &askpass {
                spec.env.push(("SUDO_ASKPASS".into(), p.to_string_lossy().into_owned()));
            }
            crate::launch::spawn_detached(&spec, self.use_scope).map_err(|e| e.to_string())?;
        }
        if let Some(run) = self.state.borrow_mut().run.as_mut() {
            run.claude = ClaudeFix::Started(None);
        }
        self.changed();
        if let Some(before) = before.filter(|_| !workspace.is_empty()) {
            self.find_claude_window(before, workspace);
        }
        Ok(())
    }

    /// The new window on the workspace, so "Show" can bring it over.
    fn find_claude_window(self: &Rc<Self>, before: Vec<String>, workspace: String) {
        let weak = Rc::downgrade(self);
        let tries = Cell::new(0);
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let Some(u) = weak.upgrade() else { return glib::ControlFlow::Break };
            tries.set(tries.get() + 1);
            let new = crate::hyprland::clients_json()
                .map(|j| update::windows_on(&j, &workspace))
                .and_then(|now| now.into_iter().find(|a| !before.contains(a)));
            if let Some(address) = new {
                log::info!("updates: Claude's window is {address}");
                if let Some(run) = u.state.borrow_mut().run.as_mut() {
                    run.claude = ClaudeFix::Started(Some(address));
                }
                u.changed();
                return glib::ControlFlow::Break;
            }
            if tries.get() >= 40 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Brings the Claude window to the current workspace.
    pub fn show_claude(&self) -> Result<(), String> {
        let address = match self.state.borrow().run.as_ref().map(|r| r.claude.clone()) {
            Some(ClaudeFix::Started(Some(a))) => a,
            _ => {
                // Window not found (yet): show the workspace instead.
                let ws = self.store.get().updates.claude_workspace.clone();
                let Some(name) = ws.strip_prefix("special:") else {
                    return Err("Claude's window was not found".into());
                };
                return crate::hyprland::eval(&[format!("hl.dispatch(hl.dsp.workspace.toggle_special({}))", crate::hyprconf::lua_string(name))]);
            }
        };
        crate::hyprland::eval(&[update::show_window_lua(&address)])
    }
}
