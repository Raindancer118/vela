//! Settings → Updates: update everything, only the package database, or
//! selected apps; live output; a failed run can be handed to Claude.

use super::binder::{Binder, group};
use crate::ui::daemon::Daemon;
use crate::ui::updates::{ClaudeFix, State, Updates};
use crate::update::{Mode, Pending, PkgState, Source, Tracker};
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

fn ago(ms: i64) -> String {
    if ms <= 0 {
        return "Not checked yet".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let min = (now - ms).max(0) / 60_000;
    match min {
        0 => "Checked just now".into(),
        1..=59 => format!("Checked {min} min ago"),
        60..=2879 => format!("Checked {} h ago", min / 60),
        _ => format!("Checked {} days ago", min / 1440),
    }
}

fn summary_line(s: &State) -> String {
    let st = &s.status;
    if st.checking {
        return "Looking for updates…".into();
    }
    let count = |src: Source| st.pending.iter().filter(|p| p.source == src).count();
    let parts: Vec<String> = [Source::Repo, Source::Aur, Source::Flatpak]
        .into_iter()
        .filter(|s| count(*s) > 0)
        .map(|s| format!("{} {}", count(s), s.label()))
        .collect();
    let what = match st.pending.len() {
        _ if st.checked_at <= 0 => String::new(),
        0 => "Everything is up to date · ".into(),
        n => format!("{n} update{} ({}) · ", if n == 1 { "" } else { "s" }, parts.join(", ")),
    };
    format!("{what}{}", ago(st.checked_at))
}

fn toast(widget: &impl IsA<gtk::Widget>, text: &str) {
    if let Some(overlay) = widget.ancestor(adw::ToastOverlay::static_type()).and_downcast::<adw::ToastOverlay>() {
        overlay.add_toast(adw::Toast::new(text));
    }
}

fn button(label: &str, classes: &[&str]) -> gtk::Button {
    gtk::Button::builder()
        .label(label)
        .valign(gtk::Align::Center)
        .css_classes(classes.to_vec())
        .build()
}

/// Claude's logo and a label, in Claude's orange.
fn claude_button() -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&gtk::Image::from_icon_name("vela-claude-symbolic"));
    content.append(&gtk::Label::new(Some("Fix with Claude")));
    gtk::Button::builder()
        .child(&content)
        .valign(gtk::Align::Center)
        .css_classes(["pill", "vela-claude-button"])
        .tooltip_text("Claude Code looks into the error and repairs it in the background")
        .build()
}

/// Progress of one package during a run, next to its source badge.
#[derive(Clone)]
struct RowStatus {
    pending: Pending,
    root: gtk::Box,
    spinner: adw::Spinner,
    icon: gtk::Image,
    label: gtk::Label,
}

impl RowStatus {
    fn new(pending: &Pending) -> RowStatus {
        let root = gtk::Box::builder().spacing(6).valign(gtk::Align::Center).visible(false).build();
        let spinner = adw::Spinner::new();
        let icon = gtk::Image::new();
        let label = gtk::Label::builder().css_classes(["caption"]).build();
        root.append(&spinner);
        root.append(&icon);
        root.append(&label);
        RowStatus {
            pending: pending.clone(),
            root,
            spinner,
            icon,
            label,
        }
    }

    fn show(&self, tracker: Option<&Tracker>) {
        let Some(state) = tracker.and_then(|t| t.state(&self.pending)) else {
            self.root.set_visible(false);
            return;
        };
        self.root.set_visible(true);
        self.label.set_label(state.label());
        let busy = matches!(state, PkgState::Downloading | PkgState::Building | PkgState::Installing);
        self.spinner.set_visible(busy);
        let icon = match state {
            PkgState::Downloaded => Some("folder-download-symbolic"),
            PkgState::Installed => Some("emblem-ok-symbolic"),
            _ => None,
        };
        self.icon.set_visible(icon.is_some());
        self.icon.set_icon_name(icon);
        for c in ["dim-label", "success", "accent"] {
            self.root.remove_css_class(c);
        }
        match state {
            PkgState::Queued | PkgState::Skipped => self.root.add_css_class("dim-label"),
            PkgState::Installed => self.root.add_css_class("success"),
            PkgState::Downloaded => self.root.add_css_class("accent"),
            _ => {}
        }
    }
}

fn open_log(path: &std::path::Path) {
    if let Ok(spec) = crate::launch::open_path_spec(path) {
        let _ = crate::launch::spawn_detached(&spec, false);
    }
}

pub fn build(daemon: &Rc<Daemon>, b: &Binder) -> adw::PreferencesPage {
    let updates = daemon.updates.clone();
    let page = adw::PreferencesPage::new();

    // ------------------------------------------------------------ actions
    let actions = group("System updates", "");
    let full_btn = button("Update", &["suggested-action", "pill"]);
    let full = adw::ActionRow::builder()
        .title("Update everything")
        .subtitle("Repository packages, AUR and Flatpak apps")
        .activatable_widget(&full_btn)
        .build();
    full.add_prefix(&gtk::Image::from_icon_name("vela-updates-symbolic"));
    full.add_suffix(&full_btn);
    actions.add(&full);

    let db_btn = button("Refresh", &["flat"]);
    let db = adw::ActionRow::builder()
        .title("Refresh package database")
        .subtitle("Syncs pacman's database and Flatpak's app info, installs nothing")
        .activatable_widget(&db_btn)
        .build();
    db.add_prefix(&gtk::Image::from_icon_name("view-refresh-symbolic"));
    db.add_suffix(&db_btn);
    actions.add(&db);

    let check_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .tooltip_text("Check for updates")
        .build();
    let check_spinner = adw::Spinner::new();
    let check = adw::ActionRow::builder().title("Check for updates").activatable_widget(&check_btn).build();
    check.add_prefix(&gtk::Image::from_icon_name("system-search-symbolic"));
    check.add_suffix(&check_spinner);
    check.add_suffix(&check_btn);
    actions.add(&check);
    page.add(&actions);

    // ----------------------------------------------------------- progress
    let progress = group("Progress", "");
    progress.set_visible(false);
    let run_row = adw::ActionRow::new();
    let run_spinner = adw::Spinner::new();
    let run_icon = gtk::Image::new();
    run_row.add_prefix(&run_spinner);
    run_row.add_prefix(&run_icon);
    progress.add(&run_row);

    let error_row = adw::ActionRow::builder()
        .title("What went wrong")
        .subtitle_lines(0)
        .subtitle_selectable(true)
        .css_classes(["vela-update-error"])
        .build();
    error_row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
    let fix_btn = claude_button();
    let log_btn = button("Open log", &["flat"]);
    let suffix = gtk::Box::new(gtk::Orientation::Vertical, 6);
    suffix.set_valign(gtk::Align::Center);
    suffix.append(&fix_btn);
    suffix.append(&log_btn);
    error_row.add_suffix(&suffix);
    progress.add(&error_row);

    let claude_row = adw::ActionRow::builder().title("Claude is fixing it").subtitle_lines(0).build();
    claude_row.add_prefix(&gtk::Image::from_icon_name("vela-claude-symbolic"));
    let show_btn = button("Show", &["flat"]);
    claude_row.add_suffix(&show_btn);
    progress.add(&claude_row);

    let buffer = gtk::TextBuffer::new(None);
    let view = gtk::TextView::builder()
        .buffer(&buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(10)
        .right_margin(10)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(240)
        .max_content_height(240)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .css_classes(["card"])
        .build();
    let expander = gtk::Expander::builder().label("Output").child(&scroll).margin_top(12).build();
    progress.add(&expander);
    page.add(&progress);

    // ------------------------------------------------------------ pending
    let list = group("Available updates", "");
    let selected_btn = button("Update selected", &["flat"]);
    selected_btn.set_sensitive(false);
    list.set_header_suffix(Some(&selected_btn));
    let warning = adw::ActionRow::builder()
        .title("Partial upgrade")
        .subtitle(
            "Updating single repository packages can break programs that need newer libraries. \
             Arch only supports updating everything.",
        )
        .subtitle_lines(0)
        .visible(false)
        .build();
    warning.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    list.add(&warning);
    page.add(&list);

    // ----------------------------------------------------------- settings
    let settings = group("Settings", "");
    settings.add(&b.spin(
        "Check every",
        "Hours between background checks, 0 = never",
        0.0,
        720.0,
        1.0,
        0,
        |c| c.updates.check_interval_hours.into(),
        |c, v| c.updates.check_interval_hours = v as u32,
    ));
    settings.add(&b.switch("AUR packages", "Through paru or yay", |c| c.updates.aur, |c, v| c.updates.aur = v));
    settings.add(&b.switch("Flatpak apps", "", |c| c.updates.flatpak, |c, v| c.updates.flatpak = v));
    settings.add(&b.switch(
        "Show in the control center",
        "A tile with the number of updates",
        |c| c.panel.updates_tile,
        |c, v| c.panel.updates_tile = v,
    ));
    settings.add(&b.entry(
        "Workspace for Claude's window (empty = open normally)",
        |c| c.updates.claude_workspace.clone(),
        |c, v| c.updates.claude_workspace = v,
    ));
    page.add(&settings);

    // -------------------------------------------------------------- logic
    let selection: Rc<RefCell<HashSet<(Source, String)>>> = Rc::default();
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let statuses: Rc<RefCell<Vec<RowStatus>>> = Rc::default();
    let shown: Rc<RefCell<Option<Vec<Pending>>>> = Rc::default();
    let shown_errors: Rc<RefCell<Vec<String>>> = Rc::default();
    let shown_log: Rc<RefCell<Option<PathBuf>>> = Rc::default();

    let sync_selection = {
        let (selection, selected_btn, warning, updates) = (selection.clone(), selected_btn.clone(), warning.clone(), updates.clone());
        Rc::new(move || {
            let sel = selection.borrow();
            let running = updates.state().running();
            selected_btn.set_sensitive(!sel.is_empty() && !running);
            selected_btn.set_label(&if sel.is_empty() {
                "Update selected".into()
            } else {
                format!("Update {} selected", sel.len())
            });
            warning.set_visible(sel.iter().any(|(s, _)| *s == Source::Repo));
        })
    };

    let render = {
        let statuses = statuses.clone();
        let (list, rows, selection, shown, shown_errors, sync_selection) = (
            list.clone(),
            rows.clone(),
            selection.clone(),
            shown.clone(),
            shown_errors.clone(),
            sync_selection.clone(),
        );
        move |s: &State| {
            let pending = &s.status.pending;
            if shown.borrow().as_ref() == Some(pending) && *shown_errors.borrow() == s.status.check_errors {
                return;
            }
            *shown.borrow_mut() = Some(pending.clone());
            *shown_errors.borrow_mut() = s.status.check_errors.clone();
            for r in rows.borrow_mut().drain(..) {
                list.remove(&r);
            }
            statuses.borrow_mut().clear();
            // Keep only what is still pending.
            selection
                .borrow_mut()
                .retain(|(src, id)| pending.iter().any(|p| p.source == *src && p.id == *id));
            let mut new_rows: Vec<gtk::Widget> = Vec::new();
            for err in &s.status.check_errors {
                let r = adw::ActionRow::builder()
                    .title("Could not check")
                    .subtitle(glib::markup_escape_text(err).as_str())
                    .subtitle_lines(0)
                    .build();
                r.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
                new_rows.push(r.upcast());
            }
            if pending.is_empty() && s.status.checked_at > 0 {
                let r = adw::ActionRow::builder().title("Everything is up to date").build();
                r.add_prefix(&gtk::Image::from_icon_name("emblem-ok-symbolic"));
                new_rows.push(r.upcast());
            }
            for p in pending {
                let tick = gtk::CheckButton::builder().valign(gtk::Align::Center).build();
                tick.set_active(selection.borrow().contains(&(p.source, p.id.clone())));
                let version = match (p.old.is_empty(), p.new.is_empty()) {
                    (false, false) => format!("{} → {}", p.old, p.new),
                    (true, false) => p.new.clone(),
                    _ => "New version".into(),
                };
                let r = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&p.name).as_str())
                    .subtitle(glib::markup_escape_text(&version).as_str())
                    .activatable_widget(&tick)
                    .build();
                r.add_prefix(&tick);
                let status = RowStatus::new(p);
                status.show(s.run.as_ref().map(|r| &r.tracker));
                r.add_suffix(&status.root);
                statuses.borrow_mut().push(status);
                r.add_suffix(
                    &gtk::Label::builder()
                        .label(p.source.label())
                        .valign(gtk::Align::Center)
                        .css_classes(["vela-source-badge", "caption"])
                        .build(),
                );
                let key = (p.source, p.id.clone());
                let (selection, sync) = (selection.clone(), sync_selection.clone());
                tick.connect_toggled(move |t| {
                    if t.is_active() {
                        selection.borrow_mut().insert(key.clone());
                    } else {
                        selection.borrow_mut().remove(&key);
                    }
                    sync();
                });
                new_rows.push(r.upcast());
            }
            for r in &new_rows {
                list.add(r);
            }
            *rows.borrow_mut() = new_rows;
            sync_selection();
        }
    };

    let apply = {
        let (actions, full_btn, db_btn, check_btn, check_spinner, check) = (
            actions.clone(),
            full_btn.clone(),
            db_btn.clone(),
            check_btn.clone(),
            check_spinner.clone(),
            check.clone(),
        );
        let (progress, run_row, run_spinner, run_icon, error_row, fix_btn, claude_row, show_btn, expander, buffer, shown_log) = (
            progress.clone(),
            run_row.clone(),
            run_spinner.clone(),
            run_icon.clone(),
            error_row.clone(),
            fix_btn.clone(),
            claude_row.clone(),
            show_btn.clone(),
            expander.clone(),
            buffer.clone(),
            shown_log.clone(),
        );
        let (workspace_of, sync_selection) = (b.store.clone(), sync_selection.clone());
        move |s: &State| {
            let running = s.running();
            let busy = running || s.status.checking;
            actions.set_description(Some(&summary_line(s)));
            check.set_subtitle(&ago(s.status.checked_at));
            full_btn.set_sensitive(!running);
            db_btn.set_sensitive(!running);
            check_btn.set_sensitive(!busy);
            check_btn.set_visible(!s.status.checking);
            check_spinner.set_visible(s.status.checking);
            sync_selection();
            render(s);

            let Some(run) = &s.run else {
                progress.set_visible(false);
                return;
            };
            progress.set_visible(true);
            if shown_log.borrow().as_ref() != Some(&run.log_path) {
                *shown_log.borrow_mut() = Some(run.log_path.clone());
                buffer.set_text(&run.log);
            }
            run_row.set_title(&run.label);
            run_spinner.set_visible(running);
            run_icon.set_visible(!running);
            match &run.result {
                None => {
                    let (i, n, title) = &run.step;
                    let step = if *n > 1 { format!("Step {} of {n} · {title}", i + 1) } else { title.clone() };
                    let step = if s.status.fingerprint {
                        format!("{step} · touch the fingerprint reader")
                    } else {
                        step
                    };
                    run_row.set_subtitle(&glib::markup_escape_text(&step));
                    error_row.set_visible(false);
                    claude_row.set_visible(false);
                }
                Some(Ok(())) => {
                    run_row.set_subtitle("Finished without errors");
                    run_icon.set_icon_name(Some("emblem-ok-symbolic"));
                    error_row.set_visible(false);
                    claude_row.set_visible(false);
                }
                Some(Err(f)) => {
                    let code = f.code.map_or(String::new(), |c| format!(" (exit code {c})"));
                    run_row.set_subtitle(&glib::markup_escape_text(&format!("{} failed{code}", f.step)));
                    run_icon.set_icon_name(Some("dialog-error-symbolic"));
                    error_row.set_visible(true);
                    error_row.set_subtitle(&glib::markup_escape_text(&f.summary));
                    expander.set_expanded(true);
                    let started = matches!(run.claude, ClaudeFix::Started(_));
                    fix_btn.set_visible(!started);
                    claude_row.set_visible(started);
                    let ws = workspace_of.get().updates.claude_workspace.clone();
                    claude_row.set_subtitle(&glib::markup_escape_text(&if ws.is_empty() {
                        "In its own terminal window".to_owned()
                    } else if ws == "special:minimized" {
                        "In the background with your minimized windows (Super+↑ or “Show” brings it here)".to_owned()
                    } else {
                        format!("In the background on {ws}")
                    }));
                    show_btn.set_visible(!ws.is_empty());
                }
            }
        }
    };
    let apply = Rc::new(apply);
    apply(&updates.state());
    {
        let apply = apply.clone();
        updates.subscribe(move |s| apply(&s.clone()));
    }
    {
        let st = statuses.clone();
        updates.subscribe_progress(move |t| {
            for row in st.borrow().iter() {
                row.show(Some(t));
            }
        });
        // A new run or none: every row follows the current state.
        let st = statuses.clone();
        updates.subscribe(move |s| {
            for row in st.borrow().iter() {
                row.show(s.run.as_ref().map(|r| &r.tracker));
            }
        });
    }
    {
        let (buffer, view) = (buffer.clone(), view.clone());
        updates.subscribe_log(move |text| {
            let mut end = buffer.end_iter();
            buffer.insert(&mut end, text);
            // Long builds: keep the buffer (and GTK) fast.
            if buffer.char_count() > 400_000 {
                let mut start = buffer.start_iter();
                let mut cut = buffer.iter_at_offset(buffer.char_count() - 300_000);
                buffer.delete(&mut start, &mut cut);
            }
            let mark = buffer.create_mark(None, &buffer.end_iter(), false);
            view.scroll_mark_onscreen(&mark);
            buffer.delete_mark(&mark);
        });
    }
    {
        // "Checked 5 min ago" stays right while the page is open.
        let (apply, updates, weak) = (apply.clone(), updates.clone(), page.downgrade());
        glib::timeout_add_seconds_local(30, move || {
            let Some(page) = weak.upgrade() else { return glib::ControlFlow::Break };
            if page.is_mapped() {
                apply(&updates.state());
            }
            glib::ControlFlow::Continue
        });
    }

    let start = |updates: &Rc<Updates>, widget: &gtk::Button, mode: Mode| {
        if let Err(e) = updates.start(mode) {
            toast(widget, &e);
        }
    };
    {
        let u = updates.clone();
        full_btn.connect_clicked(move |w| start(&u, w, Mode::Full));
    }
    {
        let u = updates.clone();
        db_btn.connect_clicked(move |w| start(&u, w, Mode::Database));
    }
    {
        let u = updates.clone();
        check_btn.connect_clicked(move |_| u.check());
    }
    {
        let (u, selection) = (updates.clone(), selection.clone());
        selected_btn.connect_clicked(move |w| {
            let sel = selection.borrow().clone();
            let items: Vec<Pending> = u
                .state()
                .status
                .pending
                .into_iter()
                .filter(|p| sel.contains(&(p.source, p.id.clone())))
                .collect();
            start(&u, w, Mode::Selected(items));
        });
    }
    {
        let u = updates.clone();
        fix_btn.connect_clicked(move |w| {
            if let Err(e) = u.fix_with_claude() {
                toast(w, &e);
            }
        });
    }
    {
        let u = updates.clone();
        show_btn.connect_clicked(move |w| {
            if let Err(e) = u.show_claude() {
                toast(w, &e);
            }
        });
    }
    {
        let u = updates.clone();
        log_btn.connect_clicked(move |_| {
            if let Some(run) = u.state().run {
                open_log(&run.log_path);
            }
        });
    }
    test_hooks(&updates);
    {
        let u = updates.clone();
        page.connect_map(move |_| u.set_page_visible(true));
        let u = updates.clone();
        page.connect_unmap(move |_| u.set_page_visible(false));
    }
    page
}

/// Test hooks (smoke test, screenshots): `VELA_UPDATES_RUN=full|database|all-selected`
/// starts a run once the page exists, `VELA_UPDATES_FIX=1` hands a failure to Claude.
fn test_hooks(updates: &Rc<Updates>) {
    if let Ok(run) = std::env::var("VELA_UPDATES_RUN") {
        let u = updates.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            let mode = match run.as_str() {
                "full" => Mode::Full,
                "database" => Mode::Database,
                "all-selected" => Mode::Selected(u.state().status.pending),
                _ => return,
            };
            if let Err(e) = u.start(mode) {
                log::warn!("VELA_UPDATES_RUN: {e}");
            }
        });
    }
    if std::env::var("VELA_UPDATES_FIX").is_ok_and(|v| v == "1") {
        let weak = Rc::downgrade(updates);
        updates.subscribe(move |s| {
            let failed = s.run.as_ref().is_some_and(|r| matches!(r.result, Some(Err(_))) && r.claude == ClaudeFix::Idle);
            if let (true, Some(u)) = (failed, weak.upgrade()) {
                // Not from inside the listener loop.
                glib::idle_add_local_once(move || {
                    if let Err(e) = u.fix_with_claude() {
                        log::warn!("VELA_UPDATES_FIX: {e}");
                    }
                });
            }
        });
    }
}
