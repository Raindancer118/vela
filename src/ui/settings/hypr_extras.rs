//! Shortcuts, window rules and autostart: things vela adds to Hyprland.
//! Every change saves hyprland.toml and reloads Hyprland's config.

use super::binder::group;
use super::hypr_rows::Rows;
use crate::hyprextra::{self, ACTIONS, Arg, Autostart, DIRECTIONS, EFFECTS, EffectKind, Extras, Shortcut, WindowRule};
use crate::hyprland;
use crate::ui::hypr_store::HyprStore;
use adw::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn page(r: &Rows, description: &str) -> adw::PreferencesPage {
    let p = adw::PreferencesPage::new();
    if r.store.available() {
        p.set_description(description);
    } else {
        p.set_description("Hyprland is not reachable — these settings need a running Hyprland session.");
        p.set_sensitive(false);
    }
    p
}

/// Keycaps for a shortcut, or the plain text if GTK can't show it.
fn keys_widget(combo: &str) -> gtk::Widget {
    match hyprextra::gtk_accel(combo).filter(|a| gtk::accelerator_parse(a).is_some()) {
        Some(accel) => gtk::ShortcutLabel::builder().accelerator(accel).valign(gtk::Align::Center).build().upcast(),
        None => gtk::Label::builder()
            .label(combo)
            .css_classes(["monospace", "dim-label"])
            .valign(gtk::Align::Center)
            .build()
            .upcast(),
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build()
}

/// A list section that rebuilds itself whenever the extras change.
fn live_group(store: &HyprStore, g: &adw::PreferencesGroup, fill: impl Fn(&adw::PreferencesGroup) + 'static) {
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let rebuild = {
        let (g, rows) = (g.downgrade(), rows.clone());
        let fill = Rc::new(fill);
        move || {
            let Some(g) = g.upgrade() else { return };
            for w in rows.borrow_mut().drain(..) {
                g.remove(&w);
            }
            // Collect what `fill` adds, so the next rebuild can take it out.
            let probe = adw::PreferencesGroup::new();
            fill(&probe);
            let mut added = Vec::new();
            while let Some(row) = first_row(&probe) {
                probe.remove(&row);
                g.add(&row);
                added.push(row);
            }
            *rows.borrow_mut() = added;
        }
    };
    rebuild();
    store.subscribe(move |n| {
        if n.is_none_or(|n| n == "extras:*") {
            rebuild();
        }
    });
}

fn first_row(g: &adw::PreferencesGroup) -> Option<gtk::Widget> {
    // Rows sit in the group's list box.
    let mut stack = vec![g.clone().upcast::<gtk::Widget>()];
    while let Some(w) = stack.pop() {
        if let Some(list) = w.downcast_ref::<gtk::ListBox>() {
            return list.first_child();
        }
        let mut c = w.first_child();
        while let Some(x) = c {
            stack.push(x.clone());
            c = x.next_sibling();
        }
    }
    None
}

fn update(store: &HyprStore, f: impl FnOnce(&mut Extras)) {
    let mut e = store.extras();
    f(&mut e);
    store.set_extras(e);
}

// ---------------------------------------------------------------- Shortcuts

pub fn shortcuts(r: &Rows) -> adw::PreferencesPage {
    let p = page(
        r,
        "Shortcuts you add here work like the ones in hyprland.lua. Taking keys that hyprland.lua already uses replaces that shortcut.",
    );
    let store = r.store.clone();

    let mine = group("Your shortcuts", "");
    let add = adw::ButtonRow::builder().title("Add shortcut").start_icon_name("list-add-symbolic").build();
    {
        let store = store.clone();
        add.connect_activated(move |b| shortcut_dialog(b.upcast_ref(), &store, None));
    }
    live_group(&store, &mine, {
        let store = store.clone();
        move |g| {
            for (i, s) in store.extras().shortcuts.iter().enumerate() {
                let row = adw::ActionRow::builder().title(glib::markup_escape_text(&s.summary())).build();
                row.add_prefix(&keys_widget(&hyprextra::normalize(&s.keys).unwrap_or(s.keys.clone())));
                let on = gtk::Switch::builder()
                    .active(s.enabled)
                    .valign(gtk::Align::Center)
                    .tooltip_text("Active")
                    .build();
                {
                    let store = store.clone();
                    on.connect_active_notify(move |sw| {
                        let v = sw.is_active();
                        update(&store, |e| {
                            if let Some(s) = e.shortcuts.get_mut(i) {
                                s.enabled = v;
                            }
                        });
                    });
                }
                let edit = icon_button("document-edit-symbolic", "Change");
                {
                    let store = store.clone();
                    edit.connect_clicked(move |b| shortcut_dialog(b.upcast_ref(), &store, Some(i)));
                }
                let del = icon_button("user-trash-symbolic", "Remove");
                {
                    let store = store.clone();
                    del.connect_clicked(move |_| {
                        update(&store, |e| {
                            if i < e.shortcuts.len() {
                                e.shortcuts.remove(i);
                            }
                        })
                    });
                }
                row.add_suffix(&on);
                row.add_suffix(&edit);
                row.add_suffix(&del);
                g.add(&row);
            }
        }
    });
    p.add(&mine);
    let adding = group("", "");
    adding.add(&add);
    p.add(&adding);
    open_on_map(&p, "shortcut", &add);

    let theirs = group(
        "From hyprland.lua",
        "What they do is defined in Lua, so only the keys show here. Switch one off to free its keys.",
    );
    live_group(&store, &theirs, {
        let store = store.clone();
        move |g| {
            let extras = store.extras();
            let mut binds: Vec<hyprland::Bind> = store
                .binds()
                .into_iter()
                .filter(|b| b.submap.is_empty() && !b.description.starts_with("vela: "))
                .collect();
            binds.dedup_by(|a, b| a.combo == b.combo);
            let mut combos: Vec<(String, String, bool, bool)> = binds.iter().map(|b| (b.combo.clone(), b.description.clone(), b.mouse, true)).collect();
            // Switched off ones are gone from Hyprland but stay listed.
            for u in &extras.unbind {
                if !combos.iter().any(|(c, ..)| hyprextra::same_combo(c, u)) {
                    combos.push((u.clone(), String::new(), u.contains("mouse"), false));
                }
            }
            combos.sort_by(|a, b| a.0.cmp(&b.0));
            for (combo, desc, mouse, active) in combos {
                let title = if desc.is_empty() { "Set in hyprland.lua".to_owned() } else { desc };
                let row = adw::ActionRow::builder().title(glib::markup_escape_text(&title)).build();
                row.add_prefix(&keys_widget(&combo));
                if mouse {
                    row.set_subtitle("Mouse");
                }
                let on = gtk::Switch::builder().active(active).valign(gtk::Align::Center).build();
                let store = store.clone();
                on.connect_active_notify(move |sw| {
                    let v = sw.is_active();
                    let combo = combo.clone();
                    update(&store, |e| {
                        e.unbind.retain(|u| !hyprextra::same_combo(u, &combo));
                        if !v {
                            e.unbind.push(combo);
                        }
                    });
                });
                row.add_suffix(&on);
                g.add(&row);
            }
        }
    });
    p.add(&theirs);
    p
}

/// GTK key event → `SUPER + SHIFT + Q`, the key without Shift's effect
/// (Super+Shift+1 is "1", not "!"), as Hyprland matches it.
fn combo_from_event(keycode: u32, keyval: gdk::Key, state: gdk::ModifierType) -> Option<String> {
    let base = gdk::Display::default()
        .and_then(|d| d.map_keycode(keycode))
        .and_then(|keys| keys.into_iter().find(|(k, _)| k.group() == 0 && k.level() == 0).map(|(_, v)| v))
        .unwrap_or(keyval);
    let name = base.name()?.to_string();
    if name.starts_with("Super")
        || name.starts_with("Control")
        || name.starts_with("Alt")
        || name.starts_with("Shift")
        || name.starts_with("Meta")
        || name.starts_with("ISO_")
    {
        return None;
    }
    let mut mods = Vec::new();
    for (mask, m) in [
        (gdk::ModifierType::SUPER_MASK, "SUPER"),
        (gdk::ModifierType::CONTROL_MASK, "CTRL"),
        (gdk::ModifierType::ALT_MASK, "ALT"),
        (gdk::ModifierType::SHIFT_MASK, "SHIFT"),
    ] {
        if state.contains(mask) {
            mods.push(m);
        }
    }
    Some(hyprextra::combo(&mods, &name))
}

struct Recorder {
    on: Cell<bool>,
    timeout: RefCell<Option<glib::SourceId>>,
}

impl Recorder {
    fn start(self: &Rc<Self>, on_timeout: impl Fn() + 'static) {
        self.on.set(true);
        if let Err(e) = hyprland::eval(&hyprextra::record_start_lua()) {
            log::warn!("recording without the empty submap: {e}");
        }
        let this = self.clone();
        *self.timeout.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_secs(10), move || {
            this.timeout.borrow_mut().take();
            this.stop();
            on_timeout();
        }));
    }

    fn stop(&self) {
        if !self.on.replace(false) {
            return;
        }
        if let Some(id) = self.timeout.borrow_mut().take() {
            id.remove();
        }
        let _ = hyprland::eval(&[hyprextra::RECORD_STOP_LUA.to_owned()]);
    }
}

fn shortcut_dialog(anchor: &gtk::Widget, store: &HyprStore, index: Option<usize>) {
    let extras = store.extras();
    let editing = index.and_then(|i| extras.shortcuts.get(i).cloned());
    let s = editing.clone().unwrap_or(Shortcut {
        keys: String::new(),
        action: "exec".into(),
        arg: String::new(),
        repeat: false,
        locked: false,
        description: String::new(),
        enabled: true,
    });
    let dialog = adw::Dialog::builder()
        .title(if editing.is_some() { "Change shortcut" } else { "Add shortcut" })
        .content_width(560)
        .build();
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).show_start_title_buttons(false).build();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    header.pack_start(&cancel);
    header.pack_end(&save);

    let keys = Rc::new(RefCell::new(hyprextra::normalize(&s.keys).unwrap_or_default()));
    let keys_row = adw::ActionRow::builder().title("Keys").build();
    let keys_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    keys_box.set_valign(gtk::Align::Center);
    let record = gtk::Button::builder().label("Record").valign(gtk::Align::Center).build();
    keys_row.add_suffix(&keys_box);
    keys_row.add_suffix(&record);

    let action = adw::ComboRow::builder()
        .title("Action")
        .model(&gtk::StringList::new(&ACTIONS.iter().map(|a| a.label).collect::<Vec<_>>()))
        .selected(ACTIONS.iter().position(|a| a.id == s.action).unwrap_or(0) as u32)
        .build();
    let arg = adw::EntryRow::builder().title("Command").text(&s.arg).build();
    let dir = adw::ComboRow::builder()
        .title("Direction")
        .model(&gtk::StringList::new(&DIRECTIONS.iter().map(|(_, l)| *l).collect::<Vec<_>>()))
        .selected(DIRECTIONS.iter().position(|(d, _)| *d == s.arg).unwrap_or(0) as u32)
        .build();
    let mut monitors: Vec<(String, String)> = vec![("+1".into(), "Next".into()), ("-1".into(), "Previous".into())];
    monitors.extend(store.monitors().iter().map(|m| (m.name.clone(), m.title())));
    let monitor = adw::ComboRow::builder()
        .title("Monitor")
        .model(&gtk::StringList::new(&monitors.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>()))
        .selected(monitors.iter().position(|(v, _)| *v == s.arg).unwrap_or(0) as u32)
        .build();
    let repeat = adw::SwitchRow::builder().title("Repeat while held").active(s.repeat).build();
    let locked = adw::SwitchRow::builder()
        .title("Also on the lock screen")
        .subtitle("For volume, brightness and media keys")
        .active(s.locked)
        .build();
    let desc = adw::EntryRow::builder().title("Name (optional)").text(&s.description).build();

    let g = adw::PreferencesGroup::new();
    for w in [
        keys_row.upcast_ref::<gtk::Widget>(),
        action.upcast_ref(),
        arg.upcast_ref(),
        dir.upcast_ref(),
        monitor.upcast_ref(),
    ] {
        g.add(w);
    }
    let g2 = adw::PreferencesGroup::new();
    g2.add(&repeat);
    g2.add(&locked);
    g2.add(&desc);
    let pg = adw::PreferencesPage::new();
    pg.add(&g);
    pg.add(&g2);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&pg));
    dialog.set_child(Some(&view));

    // Which shortcut the keys would collide with.
    let theirs: Vec<String> = store
        .binds()
        .into_iter()
        .filter(|b| b.submap.is_empty() && !b.description.starts_with("vela: "))
        .map(|b| b.combo)
        .collect();
    let refresh = {
        let (keys, keys_box, keys_row, save, action, arg, dir, monitor, extras) = (
            keys.clone(),
            keys_box.downgrade(),
            keys_row.downgrade(),
            save.downgrade(),
            action.downgrade(),
            arg.downgrade(),
            dir.downgrade(),
            monitor.downgrade(),
            extras.clone(),
        );
        Rc::new(move || {
            let (Some(kb), Some(kr), Some(save), Some(action), Some(arg), Some(dir), Some(monitor)) = (
                keys_box.upgrade(),
                keys_row.upgrade(),
                save.upgrade(),
                action.upgrade(),
                arg.upgrade(),
                dir.upgrade(),
                monitor.upgrade(),
            ) else {
                return;
            };
            while let Some(c) = kb.first_child() {
                kb.remove(&c);
            }
            let k = keys.borrow().clone();
            if k.is_empty() {
                kb.append(&gtk::Label::builder().label("Not set").css_classes(["dim-label"]).build());
            } else {
                kb.append(&keys_widget(&k));
            }
            let clash = extras
                .shortcuts
                .iter()
                .enumerate()
                .find(|(i, x)| Some(*i) != index && !k.is_empty() && hyprextra::same_combo(&x.keys, &k));
            let replaces = !k.is_empty() && theirs.iter().any(|t| hyprextra::same_combo(t, &k));
            kr.set_subtitle(&match (clash, replaces) {
                (Some((_, other)), _) => format!("Already used by “{}”", glib::markup_escape_text(&other.summary())),
                (None, true) => "Replaces the shortcut from hyprland.lua on these keys".into(),
                _ => String::new(),
            });
            let def = &ACTIONS[action.selected() as usize % ACTIONS.len()];
            arg.set_visible(matches!(def.arg, Arg::Text(_)));
            if let Arg::Text(t) = def.arg {
                arg.set_title(t);
            }
            dir.set_visible(def.arg == Arg::Direction);
            monitor.set_visible(def.arg == Arg::Monitor);
            let arg_ok = match def.arg {
                Arg::Text(_) => !arg.text().trim().is_empty(),
                _ => true,
            };
            save.set_sensitive(!k.is_empty() && clash.is_none() && arg_ok);
        })
    };
    refresh();
    for c in [action.upcast_ref::<glib::Object>(), dir.upcast_ref(), monitor.upcast_ref()] {
        let r = refresh.clone();
        c.connect_notify_local(Some("selected"), move |_, _| r());
    }
    {
        let r = refresh.clone();
        arg.connect_changed(move |_| r());
    }

    let recorder = Rc::new(Recorder {
        on: Cell::new(false),
        timeout: RefCell::default(),
    });
    {
        let (recorder, rb) = (recorder.clone(), record.downgrade());
        record.connect_clicked(move |b| {
            if recorder.on.get() {
                recorder.stop();
                b.set_label("Record");
                return;
            }
            b.set_label("Press the keys… (Esc cancels)");
            let rb = rb.clone();
            recorder.start(move || {
                if let Some(b) = rb.upgrade() {
                    b.set_label("Record");
                }
            });
        });
    }
    let keyctl = gtk::EventControllerKey::new();
    keyctl.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let (recorder, keys, refresh, rb) = (recorder.clone(), keys.clone(), refresh.clone(), record.downgrade());
        keyctl.connect_key_pressed(move |_, keyval, keycode, state| {
            if !recorder.on.get() {
                return glib::Propagation::Proceed;
            }
            let plain = !state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK);
            if keyval == gdk::Key::Escape && plain {
                recorder.stop();
            } else if let Some(c) = combo_from_event(keycode, keyval, state) {
                *keys.borrow_mut() = c;
                recorder.stop();
            } else {
                // A modifier alone: wait for the key.
                return glib::Propagation::Stop;
            }
            if let Some(b) = rb.upgrade() {
                b.set_label("Record");
            }
            refresh();
            glib::Propagation::Stop
        });
    }
    dialog.add_controller(keyctl);
    {
        let recorder = recorder.clone();
        dialog.connect_closed(move |_| recorder.stop());
    }
    {
        let d = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
    }
    {
        let (store, d) = (store.clone(), dialog.downgrade());
        save.connect_clicked(move |_| {
            let def = &ACTIONS[action.selected() as usize % ACTIONS.len()];
            let arg = match def.arg {
                Arg::None => String::new(),
                Arg::Text(_) => arg.text().trim().to_owned(),
                Arg::Direction => DIRECTIONS[dir.selected() as usize % DIRECTIONS.len()].0.to_owned(),
                Arg::Monitor => monitors.get(monitor.selected() as usize).map(|(v, _)| v.clone()).unwrap_or_default(),
            };
            let new = Shortcut {
                keys: keys.borrow().clone(),
                action: def.id.to_owned(),
                arg,
                repeat: repeat.is_active(),
                locked: locked.is_active(),
                description: desc.text().trim().to_owned(),
                enabled: true,
            };
            if new.bind_lua().is_none() {
                return;
            }
            update(&store, |e| match index {
                Some(i) if i < e.shortcuts.len() => e.shortcuts[i] = new,
                _ => e.shortcuts.push(new),
            });
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
    }
    dialog.present(Some(anchor));
}

// ------------------------------------------------------------- Window rules

fn effects_summary(r: &WindowRule) -> String {
    r.effects
        .iter()
        .filter_map(|(k, v)| {
            let (_, label, kind) = hyprextra::effect(k)?;
            let short = label.split(" (").next().unwrap_or(label);
            Some(match kind {
                EffectKind::Bool => short.to_owned(),
                _ => format!("{short} {}", v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())),
            })
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn window_rules(r: &Rows) -> adw::PreferencesPage {
    let p = page(
        r,
        "Rules apply to windows of an app (by its class) or with a certain title, e.g. float a dialog or send an app to a workspace.",
    );
    let store = r.store.clone();
    let g = group("Your rules", "");
    live_group(&store, &g, {
        let store = store.clone();
        move |g| {
            for (i, rule) in store.extras().rules.iter().enumerate() {
                let who = [rule.class.as_str(), rule.title.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" · ");
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&who))
                    .subtitle(glib::markup_escape_text(&effects_summary(rule)))
                    .build();
                let on = gtk::Switch::builder().active(rule.enabled).valign(gtk::Align::Center).build();
                {
                    let store = store.clone();
                    on.connect_active_notify(move |sw| {
                        let v = sw.is_active();
                        update(&store, |e| {
                            if let Some(r) = e.rules.get_mut(i) {
                                r.enabled = v;
                            }
                        });
                    });
                }
                let edit = icon_button("document-edit-symbolic", "Change");
                {
                    let store = store.clone();
                    edit.connect_clicked(move |b| rule_dialog(b.upcast_ref(), &store, Some(i)));
                }
                let del = icon_button("user-trash-symbolic", "Remove");
                {
                    let store = store.clone();
                    del.connect_clicked(move |_| {
                        update(&store, |e| {
                            if i < e.rules.len() {
                                e.rules.remove(i);
                            }
                        })
                    });
                }
                row.add_suffix(&on);
                row.add_suffix(&edit);
                row.add_suffix(&del);
                g.add(&row);
            }
        }
    });
    let add = adw::ButtonRow::builder().title("Add rule").start_icon_name("list-add-symbolic").build();
    {
        let store = store.clone();
        add.connect_activated(move |b| rule_dialog(b.upcast_ref(), &store, None));
    }
    p.add(&g);
    let adding = group("", "");
    adding.add(&add);
    p.add(&adding);
    open_on_map(&p, "rule", &add);
    p
}

/// Test hook (smoke test, screenshots): `VELA_SETTINGS_DIALOG=shortcut|rule`
/// opens that dialog once the page shows.
fn open_on_map(p: &adw::PreferencesPage, which: &'static str, button: &adw::ButtonRow) {
    if std::env::var("VELA_SETTINGS_DIALOG").ok().as_deref() != Some(which) {
        return;
    }
    let (b, done) = (button.downgrade(), Cell::new(false));
    p.connect_map(move |_| {
        if !done.replace(true)
            && let Some(b) = b.upgrade()
        {
            glib::idle_add_local_once(move || b.emit_by_name::<()>("activated", &[]));
        }
    });
}

fn rule_dialog(anchor: &gtk::Widget, store: &HyprStore, index: Option<usize>) {
    let extras = store.extras();
    let editing = index.and_then(|i| extras.rules.get(i).cloned());
    let rule = editing.clone().unwrap_or(WindowRule {
        name: String::new(),
        enabled: true,
        class: String::new(),
        title: String::new(),
        effects: Vec::new(),
    });
    let dialog = adw::Dialog::builder()
        .title(if editing.is_some() { "Change rule" } else { "Add rule" })
        .content_width(600)
        .content_height(720)
        .build();
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).show_start_title_buttons(false).build();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    header.pack_start(&cancel);
    header.pack_end(&save);

    let class = adw::EntryRow::builder().title("App class (regex)").text(&rule.class).build();
    let title = adw::EntryRow::builder().title("Window title (regex, optional)").text(&rule.title).build();
    // Pick an open window instead of guessing its class.
    let pick = gtk::MenuButton::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text("Pick an open window")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let windows = hyprland::open_windows();
    for (c, t) in &windows {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(c))
            .subtitle(glib::markup_escape_text(t))
            .activatable(true)
            .build();
        list.append(&row);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(260)
        .max_content_height(360)
        .propagate_natural_height(true)
        .min_content_width(360)
        .build();
    let pop = gtk::Popover::builder().child(&scroll).build();
    pick.set_popover(Some(&pop));
    {
        let (class, pop) = (class.downgrade(), pop.downgrade());
        list.connect_row_activated(move |_, row| {
            if let (Some(c), Some(p), Some((cls, _))) = (class.upgrade(), pop.upgrade(), windows.get(row.index() as usize)) {
                c.set_text(&hyprextra::exact_regex(cls));
                p.popdown();
            }
        });
    }
    class.add_suffix(&pick);
    let who = group("Which windows", "");
    who.add(&class);
    who.add(&title);

    let what = group("What happens", "");
    type Getter = Box<dyn Fn() -> Option<(String, toml::Value)>>;
    let mut getters: Vec<Getter> = Vec::new();
    let current = |k: &str| rule.effects.iter().find(|(e, _)| e == k).map(|(_, v)| v.clone());
    for (key, label, kind) in EFFECTS {
        let key = (*key).to_owned();
        match kind {
            EffectKind::Bool => {
                let row = adw::SwitchRow::builder()
                    .title(*label)
                    .active(current(&key).and_then(|v| v.as_bool()).unwrap_or(false))
                    .build();
                what.add(&row);
                getters.push(Box::new(move || row.is_active().then(|| (key.clone(), true.into()))));
            }
            EffectKind::Int | EffectKind::Float => {
                let float = *kind == EffectKind::Float;
                let v = current(&key);
                let exp = adw::ExpanderRow::builder()
                    .title(*label)
                    .show_enable_switch(true)
                    .enable_expansion(v.is_some())
                    .expanded(false)
                    .build();
                let (lo, hi, step, val) = if float {
                    (0.05, 1.0, 0.05, v.as_ref().and_then(|v| v.as_float()).unwrap_or(0.9))
                } else {
                    (0.0, 40.0, 1.0, v.as_ref().and_then(|v| v.as_integer()).unwrap_or(0) as f64)
                };
                let spin = adw::SpinRow::builder()
                    .title("Value")
                    .adjustment(&gtk::Adjustment::new(val, lo, hi, step, step * 5.0, 0.0))
                    .digits(if float { 2 } else { 0 })
                    .build();
                exp.add_row(&spin);
                what.add(&exp);
                getters.push(Box::new(move || {
                    exp.enables_expansion().then(|| {
                        let v: toml::Value = if float { spin.value().into() } else { (spin.value().round() as i64).into() };
                        (key.clone(), v)
                    })
                }));
            }
            EffectKind::Text => {
                let v = current(&key).and_then(|v| v.as_str().map(str::to_owned));
                let exp = adw::ExpanderRow::builder()
                    .title(*label)
                    .show_enable_switch(true)
                    .enable_expansion(v.is_some())
                    .expanded(v.is_some())
                    .build();
                let entry = adw::EntryRow::builder().title("Value").text(v.unwrap_or_default()).build();
                exp.add_row(&entry);
                what.add(&exp);
                getters.push(Box::new(move || {
                    let t = entry.text().trim().to_owned();
                    (exp.enables_expansion() && !t.is_empty()).then(|| (key.clone(), t.into()))
                }));
            }
        }
    }

    let pg = adw::PreferencesPage::new();
    pg.add(&who);
    pg.add(&what);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&pg));
    dialog.set_child(Some(&view));

    let sync = {
        let (save, class, title) = (save.downgrade(), class.downgrade(), title.downgrade());
        move || {
            if let (Some(s), Some(c), Some(t)) = (save.upgrade(), class.upgrade(), title.upgrade()) {
                s.set_sensitive(!c.text().trim().is_empty() || !t.text().trim().is_empty());
            }
        }
    };
    sync();
    {
        let s = sync.clone();
        class.connect_changed(move |_| s());
    }
    title.connect_changed(move |_| sync());
    {
        let d = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
    }
    {
        let (store, d) = (store.clone(), dialog.downgrade());
        save.connect_clicked(move |b| {
            let effects: Vec<(String, toml::Value)> = getters.iter().filter_map(|g| g()).collect();
            let (c, t) = (class.text().trim().to_owned(), title.text().trim().to_owned());
            if effects.is_empty() {
                b.set_label("Choose what happens first");
                return;
            }
            let base = hyprextra::rule_name(if c.is_empty() { &t } else { c.trim_matches(['^', '$']) });
            update(&store, |e| {
                let mut name = editing.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| base.clone());
                // Unique among the other rules.
                let mut n = 2;
                while e.rules.iter().enumerate().any(|(i, r)| Some(i) != index && r.name == name) {
                    name = format!("{base}-{n}");
                    n += 1;
                }
                let new = WindowRule {
                    name,
                    enabled: true,
                    class: c,
                    title: t,
                    effects,
                };
                match index {
                    Some(i) if i < e.rules.len() => e.rules[i] = new,
                    _ => e.rules.push(new),
                }
            });
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
    }
    dialog.present(Some(anchor));
}

// ---------------------------------------------------------------- Autostart

pub fn autostart(r: &Rows) -> adw::PreferencesPage {
    let p = page(
        r,
        "Commands that run when Hyprland starts, after hyprland.lua. What hyprland.lua starts itself isn't listed here.",
    );
    let store = r.store.clone();
    let g = group("Started with Hyprland", "");
    live_group(&store, &g, {
        let store = store.clone();
        move |g| {
            for (i, a) in store.extras().autostart.iter().enumerate() {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&a.command))
                    .css_classes(["monospace"])
                    .build();
                let on = gtk::Switch::builder().active(a.enabled).valign(gtk::Align::Center).build();
                {
                    let store = store.clone();
                    on.connect_active_notify(move |sw| {
                        let v = sw.is_active();
                        update(&store, |e| {
                            if let Some(a) = e.autostart.get_mut(i) {
                                a.enabled = v;
                            }
                        });
                    });
                }
                let run = icon_button("media-playback-start-symbolic", "Start now");
                {
                    let (store, cmd) = (store.clone(), a.command.clone());
                    run.connect_clicked(move |b| {
                        if let Err(e) = store.run_command(&cmd) {
                            b.set_tooltip_text(Some(&e));
                        }
                    });
                }
                let del = icon_button("user-trash-symbolic", "Remove");
                {
                    let store = store.clone();
                    del.connect_clicked(move |_| {
                        update(&store, |e| {
                            if i < e.autostart.len() {
                                e.autostart.remove(i);
                            }
                        })
                    });
                }
                row.add_suffix(&on);
                row.add_suffix(&run);
                row.add_suffix(&del);
                g.add(&row);
            }
        }
    });
    let add = adw::EntryRow::builder()
        .title("Add a command, e.g. nm-applet --indicator")
        .show_apply_button(true)
        .build();
    {
        let store = store.clone();
        add.connect_apply(move |e| {
            let cmd = e.text().trim().to_owned();
            if cmd.is_empty() {
                return;
            }
            update(&store, |x| x.autostart.push(Autostart { command: cmd, enabled: true }));
            e.set_text("");
        });
    }
    p.add(&g);
    let adding = group("", "");
    adding.add(&add);
    p.add(&adding);
    p
}
