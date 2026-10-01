//! Settings → System: vela's own version and updates, and the choice of
//! components (profile plus a switch per part), applied by install.sh.

use super::binder::{Binder, group};
use crate::components::{self, Component, Installed};
use crate::selfupdate::{self, How, VERSION};
use crate::ui::daemon::Daemon;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

pub fn updates_group(daemon: &Rc<Daemon>, b: &Binder) -> adw::PreferencesGroup {
    let su = daemon.self_update.clone();
    let g = group("Vela", "");
    let row = adw::ActionRow::builder().title(format!("Version {VERSION}")).build();
    let check = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Look for a new version")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let log = gtk::Button::builder()
        .icon_name("text-x-generic-symbolic")
        .tooltip_text("Show the log")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let update = gtk::Button::builder()
        .label(if su.how == How::Nix { "Update & rebuild" } else { "Update" })
        .valign(gtk::Align::Center)
        .css_classes(["suggested-action"])
        .build();
    row.add_suffix(&log);
    row.add_suffix(&check);
    row.add_suffix(&update);
    g.add(&row);
    g.add(&b.switch(
        "Notify about new versions",
        "Once per release, with “Update now” for installs made by install.sh",
        |c| c.self_update.notify,
        |c, v| c.self_update.notify = v,
    ));

    let error: Rc<RefCell<Option<String>>> = Rc::default();
    let show = {
        let (row, check, log, update, how, error) = (
            row.downgrade(),
            check.downgrade(),
            log.downgrade(),
            update.downgrade(),
            su.how.clone(),
            error.clone(),
        );
        move |s: &crate::ui::selfupdate::State| {
            let (Some(row), Some(check), Some(log), Some(update)) = (row.upgrade(), check.upgrade(), log.upgrade(), update.upgrade()) else {
                return;
            };
            let text = error
                .borrow_mut()
                .take()
                .unwrap_or_else(|| selfupdate::describe(&s.status, s.checking, s.step.as_ref(), &how));
            row.set_subtitle(&glib::markup_escape_text(&text));
            check.set_sensitive(!s.checking && !s.running());
            log.set_visible(s.status.failed.is_some() || s.running());
            update.set_visible(s.available().is_some() && how != How::Package);
            update.set_sensitive(!s.running());
        }
    };
    show(&su.state());
    {
        let show = show.clone();
        su.subscribe(move |s| show(s));
    }
    {
        let su = su.clone();
        check.connect_clicked(move |_| su.check());
    }
    {
        let su = su.clone();
        log.connect_clicked(move |_| {
            let s = su.state().status;
            let Some(path) = s.failed.map(|f| f.1).or(s.installing.map(|i| i.1)) else {
                return;
            };
            if let Ok(spec) = crate::launch::open_path_spec(&path) {
                let _ = crate::launch::spawn_detached(&spec, false);
            }
        });
    }
    {
        let su = su.clone();
        update.connect_clicked(move |_| {
            let result = if su.how == How::Nix { su.update_nix() } else { su.update() };
            if let Err(e) = result {
                *error.borrow_mut() = Some(e);
                show(&su.state());
            }
        });
    }
    // A check when the page is first shown and nothing is known yet.
    if su.state().status.checked_at == 0 {
        su.check();
    }
    g
}

/// Profile and switches when install.sh made this install; what Home
/// Manager or a package installed is shown read-only.
pub fn components_group(daemon: &Rc<Daemon>) -> adw::PreferencesGroup {
    let su = daemon.self_update.clone();
    let installed = components::installed().clone();
    let nixos = crate::nixos::running_nixos();
    let editable = matches!(su.how, How::Script { .. });
    let how = match su.how {
        How::Script { .. } => "What vela offers. Applying runs install.sh of this version with the new choice; settings stay.",
        How::Nix => "Set programs.vela.components in your Home Manager configuration to add or remove parts of vela.",
        How::Package => "Installed by a package: everything is there. install.sh installs only some parts.",
    };
    let g = group("Features", how);
    let profiles = &components::catalog().profiles;
    let mut labels: Vec<String> = profiles.iter().map(|(_, d)| d.to_string()).collect();
    labels.push("Custom: picked by hand".into());
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let profile_row = adw::ComboRow::builder()
        .title("Profile")
        .model(&gtk::StringList::new(&label_refs))
        .sensitive(editable)
        .build();
    g.add(&profile_row);

    let draft = Rc::new(RefCell::new(installed.clone()));
    let syncing = Rc::new(std::cell::Cell::new(false));
    let mut switches = Vec::new();
    for c in Component::ALL {
        let (title, desc) = c.description().split_once(": ").unwrap_or((c.id(), c.description()));
        let mut subtitle: String = desc.chars().take(1).flat_map(char::to_uppercase).chain(desc.chars().skip(1)).collect();
        if nixos && c == Component::Updates {
            subtitle.push_str(" — pacman, so off on NixOS by default");
        }
        let sw = adw::SwitchRow::builder().title(title).subtitle(subtitle).sensitive(editable).build();
        g.add(&sw);
        switches.push((c, sw));
    }
    let apply = adw::ButtonRow::builder()
        .title("Apply")
        .start_icon_name("emblem-ok-symbolic")
        .css_classes(["suggested-action"])
        .visible(editable)
        .build();
    g.add(&apply);

    let index_of = move |i: &Installed| {
        let p = i.profile.as_deref().unwrap_or("full");
        profiles.iter().position(|(n, _)| *n == p).unwrap_or(profiles.len())
    };
    let sync = {
        let (draft, syncing, switches, profile_row, apply) = (draft.clone(), syncing.clone(), switches.clone(), profile_row.clone(), apply.clone());
        move || {
            syncing.set(true);
            let d = draft.borrow();
            profile_row.set_selected(index_of(&d) as u32);
            for (c, sw) in &switches {
                sw.set_active(d.has(*c));
            }
            let changed = Component::ALL.iter().any(|c| d.has(*c) != installed.has(*c));
            apply.set_sensitive(changed && !su.state().running());
            syncing.set(false);
        }
    };
    sync();
    {
        let (draft, syncing, sync) = (draft.clone(), syncing.clone(), sync.clone());
        profile_row.connect_selected_notify(move |r| {
            if syncing.get() {
                return;
            }
            // "Custom" keeps the switches as they are.
            if let Some((name, _)) = profiles.get(r.selected() as usize) {
                *draft.borrow_mut() = Installed::profile(name, nixos);
            }
            sync();
        });
    }
    for (c, sw) in &switches {
        let (c, draft, syncing, sync) = (*c, draft.clone(), syncing.clone(), sync.clone());
        sw.connect_active_notify(move |sw| {
            if syncing.get() {
                return;
            }
            draft.borrow_mut().set_nixos(c, sw.is_active(), nixos);
            sync();
        });
    }
    {
        let su = daemon.self_update.clone();
        apply.connect_activated(move |r| {
            let target = draft.borrow().clone();
            match su.change_components(&target) {
                Ok(()) => r.set_sensitive(false),
                Err(e) => r.set_title(&glib::markup_escape_text(&format!("Apply — {e}"))),
            }
        });
    }
    g
}
