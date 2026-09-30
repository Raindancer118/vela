//! Applications page: pinned apps (reorder via drag and drop or buttons),
//! hidden apps and custom actions.

use super::binder::{Binder, group, join_args, split_args};
use crate::apps::catalog::{CUSTOM_PREFIX, Catalog};
use crate::config::{Config, CustomAction, GridSource};
use crate::ui::daemon::Daemon;
use crate::ui::icons;
use crate::ui::store::ConfigStore;
use adw::prelude::*;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn build(daemon: &Rc<Daemon>, b: &Binder) -> adw::PreferencesPage {
    let p = adw::PreferencesPage::new();

    let grid = group("Grid", "What the launcher shows before you type.");
    let labels: Vec<&str> = GridSource::ALL.iter().map(|g| g.label()).collect();
    grid.add(&b.combo(
        "Show",
        "",
        &labels,
        |c| GridSource::ALL.iter().position(|g| *g == c.apps.grid).unwrap_or(0),
        |c, i| c.apps.grid = GridSource::ALL[i.min(2)],
    ));
    grid.add(&b.switch(
        "Desktop actions in search",
        "e.g. “New Private Window” for Firefox",
        |c| c.apps.desktop_actions,
        |c, v| c.apps.desktop_actions = v,
    ));
    p.add(&grid);

    let pinned = group(
        "Pinned applications",
        "Drag rows to reorder. Right-click a tile in the launcher to pin or unpin quickly.",
    );
    let add_pin = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Pin an application")
        .css_classes(["flat"])
        .build();
    pinned.set_header_suffix(Some(&add_pin));
    p.add(&pinned);

    let hidden = group("Hidden applications", "Never shown in the grid or in search results.");
    let add_hidden = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Hide an application")
        .css_classes(["flat"])
        .build();
    hidden.set_header_suffix(Some(&add_hidden));
    p.add(&hidden);

    let custom = group("Custom actions", "Your own commands, searchable and pinnable like applications.");
    let add_custom = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add a custom action")
        .css_classes(["flat"])
        .build();
    custom.set_header_suffix(Some(&add_custom));
    p.add(&custom);

    let lists = Rc::new(Lists {
        daemon: daemon.clone(),
        pinned,
        hidden,
        custom,
        rows: RefCell::default(),
    });
    lists.rebuild();

    let l = lists.clone();
    daemon.store.subscribe(move |old, new| {
        if old.apps != new.apps {
            l.rebuild();
        }
    });
    let w = Rc::downgrade(&lists);
    daemon.subscribe_catalog(move |_| {
        if let Some(l) = w.upgrade() {
            l.rebuild();
        }
    });

    let d = daemon.clone();
    add_pin.connect_clicked(move |btn| picker(&d, btn.upcast_ref(), Mode::Pin));
    let d = daemon.clone();
    add_hidden.connect_clicked(move |btn| picker(&d, btn.upcast_ref(), Mode::Hide));
    let d = daemon.clone();
    add_custom.connect_clicked(move |btn| edit_custom(&d.store, btn.upcast_ref(), None));
    p
}

struct Lists {
    daemon: Rc<Daemon>,
    pinned: adw::PreferencesGroup,
    hidden: adw::PreferencesGroup,
    custom: adw::PreferencesGroup,
    rows: RefCell<Vec<(adw::PreferencesGroup, gtk::Widget)>>,
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build()
}

fn move_pin(store: &ConfigStore, from: usize, to: usize) {
    store.update(|c| {
        let n = c.apps.pinned.len();
        if from < n && to < n && from != to {
            let k = c.apps.pinned.remove(from);
            c.apps.pinned.insert(to, k);
        }
    });
}

impl Lists {
    fn add(&self, group: &adw::PreferencesGroup, row: &impl IsA<gtk::Widget>) {
        group.add(row);
        self.rows.borrow_mut().push((group.clone(), row.clone().upcast()));
    }

    fn rebuild(&self) {
        for (g, r) in self.rows.borrow_mut().drain(..) {
            g.remove(&r);
        }
        let cfg = self.daemon.store.get();
        let catalog = self.daemon.catalog.borrow().clone();
        let store = self.daemon.store.clone();

        // Pinned
        let n = cfg.apps.pinned.len();
        if n == 0 {
            self.add(
                &self.pinned,
                &adw::ActionRow::builder()
                    .title("Nothing pinned")
                    .subtitle("The grid shows all applications until you pin some.")
                    .build(),
            );
        }
        for (i, key) in cfg.apps.pinned.iter().enumerate() {
            let row = entry_row(&catalog, key);
            row.add_prefix(&gtk::Image::from_icon_name("vela-drag-handle-symbolic"));
            let up = icon_button("go-up-symbolic", "Move up");
            up.set_sensitive(i > 0);
            let s = store.clone();
            up.connect_clicked(move |_| move_pin(&s, i, i - 1));
            let down = icon_button("go-down-symbolic", "Move down");
            down.set_sensitive(i + 1 < n);
            let s = store.clone();
            down.connect_clicked(move |_| move_pin(&s, i, i + 1));
            let del = icon_button("user-trash-symbolic", "Unpin");
            let s = store.clone();
            let k = key.clone();
            del.connect_clicked(move |_| s.update(|c| c.apps.pinned.retain(|p| *p != k)));
            row.add_suffix(&up);
            row.add_suffix(&down);
            row.add_suffix(&del);

            // Drag and drop reordering: the payload is the source index.
            let drag = gtk::DragSource::builder().actions(gdk::DragAction::MOVE).build();
            drag.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&(i as u32).to_value())));
            let rw = row.downgrade();
            drag.connect_drag_begin(move |src, _| {
                if let Some(r) = rw.upgrade() {
                    src.set_icon(Some(&gtk::WidgetPaintable::new(Some(&r))), 20, 20);
                }
            });
            row.add_controller(drag);
            let drop = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
            let s = store.clone();
            drop.connect_drop(move |_, value, _, _| match value.get::<u32>() {
                Ok(from) => {
                    let s = s.clone();
                    // Mutating the list rebuilds rows; do it after the drop completes.
                    glib::idle_add_local_once(move || move_pin(&s, from as usize, i));
                    true
                }
                Err(_) => false,
            });
            row.add_controller(drop);
            self.add(&self.pinned, &row);
        }

        // Hidden
        for key in &cfg.apps.hidden {
            let row = adw::ActionRow::builder().title(key.as_str()).build();
            let del = icon_button("view-reveal-symbolic", "Show again");
            let s = store.clone();
            let k = key.clone();
            del.connect_clicked(move |_| s.update(|c| c.apps.hidden.retain(|h| *h != k)));
            row.add_suffix(&del);
            self.add(&self.hidden, &row);
        }

        // Custom actions
        for action in &cfg.apps.custom {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&action.name).as_str())
                .subtitle(glib::markup_escape_text(&join_args(&action.command)).as_str())
                .build();
            row.add_prefix(&icons::app_image(Some(&action.icon).filter(|i| !i.is_empty()).map(String::as_str), 32));
            let edit = icon_button("document-edit-symbolic", "Edit");
            let s = store.clone();
            let a = action.clone();
            edit.connect_clicked(move |btn| edit_custom(&s, btn.upcast_ref(), Some(a.clone())));
            let del = icon_button("user-trash-symbolic", "Delete");
            let s = store.clone();
            let id = action.id.clone();
            del.connect_clicked(move |_| {
                let key = format!("{CUSTOM_PREFIX}{id}");
                s.update(|c| {
                    c.apps.custom.retain(|x| x.id != id);
                    c.apps.pinned.retain(|p| *p != key);
                })
            });
            row.add_suffix(&edit);
            row.add_suffix(&del);
            self.add(&self.custom, &row);
        }
    }
}

fn entry_row(catalog: &Arc<Catalog>, key: &str) -> adw::ActionRow {
    match catalog.get(key) {
        Some(e) => {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&e.name).as_str())
                .subtitle(glib::markup_escape_text(key).as_str())
                .build();
            row.add_prefix(&icons::app_image(e.icon.as_deref(), 32));
            row
        }
        None => adw::ActionRow::builder()
            .title(glib::markup_escape_text(key).as_str())
            .subtitle("Not installed — ignored")
            .build(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Pin,
    Hide,
}

/// Searchable list of installed applications.
fn picker(daemon: &Rc<Daemon>, parent: &gtk::Widget, mode: Mode) {
    let dialog = adw::Dialog::builder()
        .title(if mode == Mode::Pin { "Pin applications" } else { "Hide applications" })
        .content_width(480)
        .content_height(620)
        .build();
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search installed applications")
        .hexpand(true)
        .build();
    let header = adw::HeaderBar::new();
    let clamp = adw::Clamp::builder().maximum_size(420).child(&search).build();
    header.set_title_widget(Some(&clamp));
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_margin_top(12);
    list.set_margin_bottom(12);
    list.set_margin_start(12);
    list.set_margin_end(12);
    list.set_valign(gtk::Align::Start);
    let scroller = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&scroller));
    dialog.set_child(Some(&tv));

    let catalog = daemon.catalog.borrow().clone();
    let store = daemon.store.clone();
    let mut rows: Vec<(adw::ActionRow, String)> = Vec::new();
    for e in catalog.pinnable() {
        if mode == Mode::Hide && !e.key.ends_with(".desktop") {
            continue;
        }
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&e.name).as_str())
            .subtitle(glib::markup_escape_text(&e.subtitle).as_str())
            .build();
        row.add_prefix(&icons::app_image(e.icon.as_deref(), 32));
        let btn = gtk::ToggleButton::builder().valign(gtk::Align::Center).build();
        let is_on = |c: &Config, k: &str| {
            if mode == Mode::Pin {
                c.apps.pinned.iter().any(|p| p == k)
            } else {
                c.apps.hidden.iter().any(|p| p == k)
            }
        };
        let sync = {
            let btn = btn.clone();
            move |on: bool| {
                btn.set_active(on);
                btn.set_icon_name(if on { "object-select-symbolic" } else { "list-add-symbolic" });
            }
        };
        sync(is_on(&store.get(), &e.key));
        let s = store.clone();
        let key = e.key.clone();
        btn.connect_toggled(move |b| {
            let on = b.is_active();
            b.set_icon_name(if on { "object-select-symbolic" } else { "list-add-symbolic" });
            s.update(|c| {
                let list = if mode == Mode::Pin { &mut c.apps.pinned } else { &mut c.apps.hidden };
                list.retain(|k| *k != key);
                if on {
                    list.push(key.clone());
                }
            });
        });
        row.add_suffix(&btn);
        row.set_activatable_widget(Some(&btn));
        list.append(&row);
        let hay = format!("{} {} {}", e.name, e.key, e.keywords.join(" ")).to_lowercase();
        rows.push((row, hay));
    }
    search.connect_search_changed(move |s| {
        let q = s.text().to_lowercase();
        for (row, hay) in &rows {
            row.set_visible(q.split_whitespace().all(|w| hay.contains(w)));
        }
    });
    dialog.present(Some(parent));
    search.grab_focus();
}

/// Create or edit a custom action.
fn edit_custom(store: &ConfigStore, parent: &gtk::Widget, existing: Option<CustomAction>) {
    let is_new = existing.is_none();
    let action = existing.unwrap_or_default();
    let dialog = adw::Dialog::builder()
        .title(if is_new { "New custom action" } else { "Edit custom action" })
        .content_width(520)
        .build();
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).show_start_title_buttons(false).build();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    header.pack_start(&cancel);
    header.pack_end(&save);

    let page = adw::PreferencesPage::new();
    let g = group(
        "",
        "The command is split into arguments like a shell would, but it is never run through a shell. Use sh -c '…' explicitly if you need shell features.",
    );
    let name = adw::EntryRow::builder().title("Name").text(action.name.as_str()).build();
    let command = adw::EntryRow::builder().title("Command").text(join_args(&action.command)).build();
    let icon = adw::EntryRow::builder().title("Icon name or path").text(action.icon.as_str()).build();
    let keywords = adw::EntryRow::builder()
        .title("Keywords (comma separated)")
        .text(action.keywords.join(", "))
        .build();
    let terminal = adw::SwitchRow::builder().title("Run in terminal").active(action.terminal).build();
    let key = format!("{CUSTOM_PREFIX}{}", action.id);
    let pin = adw::SwitchRow::builder()
        .title("Pin to grid")
        .active(is_new || store.get().apps.pinned.contains(&key))
        .build();
    for r in [
        name.upcast_ref::<gtk::Widget>(),
        command.upcast_ref(),
        icon.upcast_ref(),
        keywords.upcast_ref(),
        terminal.upcast_ref(),
        pin.upcast_ref(),
    ] {
        g.add(r);
    }
    page.add(&g);
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&page));
    dialog.set_child(Some(&tv));

    let validate = {
        let (name, command, save) = (name.clone(), command.clone(), save.clone());
        move || {
            let cmd_ok = split_args(&command.text()).is_some_and(|v| !v.is_empty());
            if cmd_ok {
                command.remove_css_class("error")
            } else {
                command.add_css_class("error")
            }
            save.set_sensitive(!name.text().trim().is_empty() && cmd_ok);
        }
    };
    validate();
    let v = validate.clone();
    name.connect_changed(move |_| v());
    let v = validate.clone();
    command.connect_changed(move |_| v());

    let d = dialog.clone();
    cancel.connect_clicked(move |_| {
        d.close();
    });
    let store = store.clone();
    let d = dialog.clone();
    save.connect_clicked(move |_| {
        let Some(cmd) = split_args(&command.text()) else { return };
        let mut a = action.clone();
        a.name = name.text().trim().to_owned();
        a.command = cmd;
        a.icon = icon.text().trim().to_owned();
        a.keywords = keywords.text().split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        a.terminal = terminal.is_active();
        let pin_it = pin.is_active();
        store.update(|c| {
            if a.id.is_empty() {
                let mut n = c.apps.custom.len() + 1;
                while c.apps.custom.iter().any(|x| x.id == format!("action-{n}")) {
                    n += 1;
                }
                a.id = format!("action-{n}");
            }
            let key = format!("{CUSTOM_PREFIX}{}", a.id);
            match c.apps.custom.iter_mut().find(|x| x.id == a.id) {
                Some(x) => *x = a.clone(),
                None => c.apps.custom.push(a.clone()),
            }
            c.apps.pinned.retain(|p| *p != key);
            if pin_it {
                c.apps.pinned.push(key);
            }
        });
        d.close();
    });
    dialog.present(Some(parent));
}
