//! Settings window. Every widget writes straight into the config store, so
//! changes apply to the launcher and the control center immediately and are
//! saved automatically.

mod apps_page;
mod binder;
mod hypr_extras;
mod hypr_monitors;
mod hypr_pages;
mod hypr_rows;
mod pages;
mod search_page;
mod updates_page;

use super::daemon::Daemon;
use super::marker::Marker;
use crate::components::{Component, Installed};
use adw::prelude::*;
use binder::Binder;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Sidebar entries: (section, page id, title, icon). Ids are `SETTINGS_PAGES`.
const ENTRIES: [(&str, &str, &str, &str); 22] = [
    ("", "home", "Search", "system-search-symbolic"),
    ("Launcher", "general", "Launcher", "system-search-symbolic"),
    ("Launcher", "apps", "Applications", "view-grid-symbolic"),
    ("Launcher", "search", "Search", "edit-find-symbolic"),
    ("Launcher", "claude", "Claude", "vela-claude-symbolic"),
    ("Control center", "panel", "Panel", "view-dual-symbolic"),
    ("Control center", "notifications", "Notifications", "notifications-symbolic"),
    ("Control center", "power", "Power & idle", "system-shutdown-symbolic"),
    ("Hyprland", "hypr-windows", "Windows & gaps", "vela-windows-symbolic"),
    ("Hyprland", "hypr-effects", "Blur & effects", "vela-blur-symbolic"),
    ("Hyprland", "hypr-animations", "Animations", "vela-animations-symbolic"),
    ("Hyprland", "hypr-input", "Input", "input-keyboard-symbolic"),
    ("Hyprland", "hypr-monitors", "Monitors", "vela-monitors-symbolic"),
    ("Hyprland", "hypr-shortcuts", "Shortcuts", "vela-shortcuts-symbolic"),
    ("Hyprland", "hypr-rules", "Window rules", "vela-rules-symbolic"),
    ("Hyprland", "hypr-autostart", "Autostart", "vela-autostart-symbolic"),
    ("Hyprland", "hypr-layouts", "Layouts", "vela-layouts-symbolic"),
    ("Hyprland", "hypr-behaviour", "Behaviour", "vela-behaviour-symbolic"),
    ("Hyprland", "hypr-all", "All options", "vela-all-options-symbolic"),
    ("Everywhere", "appearance", "Appearance", "applications-graphics-symbolic"),
    ("Everywhere", "updates", "Updates", "vela-updates-symbolic"),
    ("Everywhere", "system", "System", "preferences-system-symbolic"),
];

type Entry = (&'static str, &'static str, &'static str, &'static str);

/// Whether a page belongs to something installed (install.sh's selection).
fn page_visible(id: &str, installed: &Installed) -> bool {
    let needs: &[Component] = match id {
        "general" | "apps" | "search" => &[Component::Launcher],
        "claude" => &[Component::Claude],
        "panel" | "notifications" => &[Component::Panel],
        // Night light belongs to the panel, the rest to idle.
        "power" => &[Component::Panel, Component::Idle],
        "updates" => &[Component::Updates],
        _ if id.starts_with("hypr-") => &[Component::Hyprland],
        _ => return true,
    };
    needs.iter().any(|c| installed.has(*c))
}

fn visible_entries(installed: &Installed) -> Vec<Entry> {
    ENTRIES.into_iter().filter(|e| page_visible(e.1, installed)).collect()
}

pub struct SettingsWindow {
    window: adw::ApplicationWindow,
    entries: Rc<Vec<Entry>>,
    sidebar: gtk::ListBox,
    home: Rc<search_page::Search>,
}

fn sidebar_row(title: &str, icon: &str) -> gtk::ListBoxRow {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    b.append(&gtk::Image::from_icon_name(icon));
    b.append(&gtk::Label::builder().label(title).xalign(0.0).build());
    gtk::ListBoxRow::builder().child(&b).build()
}

impl SettingsWindow {
    pub fn new(daemon: &Rc<Daemon>) -> Rc<SettingsWindow> {
        install_css();
        let window = adw::ApplicationWindow::builder()
            .application(&daemon.app)
            .title("Vela Settings")
            .default_width(980)
            .default_height(780)
            .icon_name("vela")
            .build();
        window.add_css_class("vela-settings");
        let binder = Binder::new(daemon.store.clone());
        daemon.hypr.ensure_loaded();
        let hypr = hypr_rows::Rows::new(daemon.hypr.clone());

        // Old page fades out quickly while the new one rises in (CSS page-in),
        // like StoneIntelligence's area switch.
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(140)
            .build();
        let entries = Rc::new(visible_entries(crate::components::installed()));
        let home = search_page::Search::new(daemon);
        stack.add_named(&home.widget, Some("home"));
        for &(_, id, title, _) in entries.iter().skip(1) {
            let page: gtk::Widget = match id {
                "general" => pages::launcher(&binder).upcast(),
                "apps" => apps_page::build(daemon, &binder).upcast(),
                "search" => pages::search(daemon, &binder).upcast(),
                "claude" => pages::claude(&binder).upcast(),
                "panel" => pages::panel(&binder).upcast(),
                "notifications" => pages::notifications(&binder).upcast(),
                "power" => pages::power(&binder).upcast(),
                "hypr-windows" => hypr_pages::windows(&hypr).upcast(),
                "hypr-effects" => hypr_pages::effects(&hypr).upcast(),
                "hypr-animations" => hypr_pages::animations(&hypr).upcast(),
                "hypr-input" => hypr_pages::input(&hypr).upcast(),
                "hypr-monitors" => hypr_monitors::monitors(&hypr).upcast(),
                "hypr-shortcuts" => hypr_extras::shortcuts(&hypr).upcast(),
                "hypr-rules" => hypr_extras::window_rules(&hypr).upcast(),
                "hypr-autostart" => hypr_extras::autostart(&hypr).upcast(),
                "hypr-layouts" => hypr_pages::layouts(&hypr).upcast(),
                "hypr-behaviour" => hypr_pages::behaviour(&hypr).upcast(),
                "hypr-all" => {
                    let (p, fill) = hypr_pages::all_options(&hypr);
                    home.before_index(fill);
                    p.upcast()
                }
                "appearance" => pages::appearance(&binder).upcast(),
                "updates" => updates_page::build(daemon, &binder).upcast(),
                _ => pages::system(daemon, &binder).upcast(),
            };
            home.add_page(id, title, &page);
            stack.add_named(&page, Some(id));
        }

        // Sidebar with a heading above each section.
        let sidebar = gtk::ListBox::builder().css_classes(["navigation-sidebar"]).build();
        for (_, _, title, icon) in entries.iter() {
            sidebar.append(&sidebar_row(title, icon));
        }
        let e = entries.clone();
        sidebar.set_header_func(move |row, before| {
            let i = row.index() as usize;
            let section = e[i].0;
            if !section.is_empty() && before.is_none_or(|b| e[b.index() as usize].0 != section) {
                let label = gtk::Label::builder().label(section).xalign(0.0).css_classes(["vela-sidebar-heading"]).build();
                row.set_header(Some(&label));
            } else {
                row.set_header(None::<&gtk::Widget>);
            }
        });
        // Selection marker below the rows so it can glide between them.
        let (overlay, marker) = Marker::below(&sidebar, "vela-sidebar-pill", false);
        let sidebar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&overlay)
            .vexpand(true)
            .build();
        let version = gtk::Label::builder()
            .label(format!("vela {}", env!("CARGO_PKG_VERSION")))
            .css_classes(["dim-label", "caption"])
            .margin_bottom(12)
            .build();
        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&sidebar_scroll);
        sidebar_box.append(&version);
        let sidebar_view = adw::ToolbarView::new();
        sidebar_view.add_top_bar(&adw::HeaderBar::new());
        sidebar_view.set_content(Some(&sidebar_box));
        let sidebar_page = adw::NavigationPage::builder().title("Vela").child(&sidebar_view).build();

        // Content: page title in the header, config errors as a banner.
        let header = adw::HeaderBar::new();
        let preview = gtk::ToggleButton::builder()
            .icon_name("view-reveal-symbolic")
            .tooltip_text("Show a live preview of the launcher")
            .build();
        if crate::components::has(Component::Launcher) {
            header.pack_end(&preview);
        }
        let banner = adw::Banner::new("");
        banner.set_button_label(Some("Reload file"));
        let set_banner = {
            let banner = banner.clone();
            move |err: Option<&str>| {
                banner.set_title(&glib::markup_escape_text(err.unwrap_or("")));
                banner.set_revealed(err.is_some());
            }
        };
        set_banner(daemon.store.last_error().as_deref());
        daemon.store.subscribe_errors(set_banner);
        let store = daemon.store.clone();
        banner.connect_button_clicked(move |_| store.reload_from_disk());
        let hypr_banner = adw::Banner::new("");
        {
            let b = hypr_banner.clone();
            daemon.hypr.subscribe_errors(move |err| {
                b.set_title(&glib::markup_escape_text(err.unwrap_or("")));
                b.set_revealed(err.is_some());
            });
        }
        let toasts = adw::ToastOverlay::new();
        let nixos_banner = nixos_banner(daemon, &window, &toasts);
        let content_view = adw::ToolbarView::new();
        content_view.add_top_bar(&header);
        content_view.add_top_bar(&banner);
        content_view.add_top_bar(&hypr_banner);
        content_view.add_top_bar(&nixos_banner);
        toasts.set_child(Some(&stack));
        content_view.set_content(Some(&toasts));
        let content_page = adw::NavigationPage::builder().title(ENTRIES[0].2).child(&content_view).build();

        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar_page)
            .content(&content_page)
            .min_sidebar_width(210.0)
            .max_sidebar_width(260.0)
            .build();
        window.set_content(Some(&split));

        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 640sp").expect("valid breakpoint"));
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);

        let marker = Rc::new(marker);
        let flip = Rc::new(std::cell::Cell::new(false));
        {
            let (stack, content_page, split, marker, store, home, entries) = (
                stack.clone(),
                content_page.clone(),
                split.clone(),
                marker.clone(),
                daemon.store.clone(),
                home.clone(),
                entries.clone(),
            );
            sidebar.connect_row_selected(move |_, row| {
                let Some(row) = row else { return };
                let (_, id, title, _) = entries[row.index() as usize];
                if id != "home" {
                    // Borrowed rows go back to their page before it shows.
                    home.restore();
                }
                let animate = store.get().appearance.animations;
                stack.set_visible_child_name(id);
                if let Some(page) = stack.visible_child() {
                    // Two identical keyframe sets: switching restarts the animation.
                    let on = if flip.replace(!flip.get()) { "vela-page-in-a" } else { "vela-page-in-b" };
                    for c in ["vela-page-in-a", "vela-page-in-b"] {
                        page.remove_css_class(c);
                    }
                    if animate {
                        page.add_css_class(on);
                    }
                }
                marker.move_to(row, animate);
                content_page.set_title(title);
                split.set_show_content(true);
            });
        }
        sidebar.select_row(sidebar.row_at_index(0).as_ref());
        {
            let (sb, entries) = (sidebar.downgrade(), entries.clone());
            home.set_open_page(Rc::new(move |id: &str| {
                if let (Some(sb), Some(i)) = (sb.upgrade(), entries.iter().position(|(_, x, _, _)| *x == id)) {
                    sb.select_row(sb.row_at_index(i as i32).as_ref());
                }
            }));
        }
        {
            // Ctrl+F anywhere: back to the search.
            let (sb, home) = (sidebar.downgrade(), home.clone());
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            keys.connect_key_pressed(move |_, key, _, state| {
                if state.contains(gdk::ModifierType::CONTROL_MASK) && matches!(key, gdk::Key::f | gdk::Key::F) {
                    if let Some(sb) = sb.upgrade() {
                        sb.select_row(sb.row_at_index(0).as_ref());
                    }
                    home.focus();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(keys);
        }
        {
            // Rows have no size before the first allocation.
            let (marker, sidebar) = (marker.clone(), sidebar.clone());
            overlay.connect_map(move |_| {
                let (marker, sidebar) = (marker.clone(), sidebar.clone());
                glib::idle_add_local_once(move || {
                    if let Some(row) = sidebar.selected_row() {
                        marker.move_to(&row, false);
                    }
                });
            });
        }

        let launcher = daemon.launcher.clone();
        preview.connect_toggled(move |b| {
            if b.is_active() {
                launcher.show(true);
            } else if launcher.is_preview() {
                launcher.hide();
            }
        });
        let launcher = daemon.launcher.clone();
        let preview_btn = preview.clone();
        window.connect_close_request(move |w| {
            preview_btn.set_active(false);
            if launcher.is_preview() {
                launcher.hide();
            }
            w.set_visible(false);
            glib::Propagation::Stop
        });

        // External changes (file edit, restore defaults) refresh all widgets.
        let b = binder.clone();
        daemon.store.subscribe_replace(move |cfg| b.refresh(cfg));

        Rc::new(SettingsWindow {
            window,
            entries,
            sidebar,
            home,
        })
    }

    pub fn show_page(&self, name: &str) {
        if let Some(i) = self.entries.iter().position(|(_, id, _, _)| *id == name) {
            self.sidebar.select_row(self.sidebar.row_at_index(i as i32).as_ref());
        }
        if name == "home" {
            self.home.clear();
            self.home.focus();
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

/// NixOS: offers to commit and rebuild what vela changed in the repository,
/// or says that changes stay outside the NixOS configuration.
fn nixos_banner(daemon: &Rc<Daemon>, window: &adw::ApplicationWindow, toasts: &adw::ToastOverlay) -> adw::Banner {
    use crate::nixos::{self, Status};
    let banner = adw::Banner::new("");
    let mode = match nixos::status() {
        Status::NotNixos => return banner,
        Status::Unconfigured => {
            banner.set_title("NixOS detected: changes are not saved to your NixOS configuration (see README → NixOS)");
            banner.set_revealed(true);
            return banner;
        }
        Status::Active(mode) => mode,
    };
    banner.set_title("Changes not yet applied to NixOS");
    banner.set_button_label(Some("Apply & rebuild"));
    let refresh = {
        let banner = banner.clone();
        move || banner.set_revealed(nixos::dirty(mode) == Some(true))
    };
    refresh();
    // Saves happen shortly after a change; look again once they're written.
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let later = {
        let refresh = refresh.clone();
        move || {
            if let Some(id) = pending.borrow_mut().take() {
                id.remove();
            }
            let (refresh, slot) = (refresh.clone(), pending.clone());
            let id = glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                slot.borrow_mut().take();
                refresh();
            });
            *pending.borrow_mut() = Some(id);
        }
    };
    {
        let later = later.clone();
        daemon.store.subscribe(move |_, _| later());
    }
    daemon.hypr.subscribe(move |_| later());
    // Coming back from the rebuild terminal.
    window.connect_is_active_notify(move |w| {
        if w.is_active() {
            refresh();
        }
    });
    let (store, toasts) = (daemon.store.clone(), toasts.clone());
    banner.connect_button_clicked(move |_| {
        let result = nixos::rebuild_spec(mode, &store.get().terminal).and_then(|spec| crate::launch::spawn_detached(&spec, false));
        if let Err(e) = result {
            toasts.add_toast(adw::Toast::new(&glib::markup_escape_text(&format!("Rebuild: {e}"))));
        }
    });
    banner
}

fn install_css() {
    thread_local! { static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    let Some(display) = gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&crate::ui::style::settings_css(&pages::ACCENTS));
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(installed: &Installed) -> Vec<&'static str> {
        visible_entries(installed).into_iter().map(|e| e.1).collect()
    }

    #[test]
    fn everything_installed_shows_every_page() {
        assert_eq!(ids(&Installed::all()).len(), ENTRIES.len());
    }

    #[test]
    fn pages_follow_the_installed_components() {
        let minimal = ids(&Installed::only(&[Component::Launcher]));
        assert_eq!(minimal, ["home", "general", "apps", "search", "appearance", "system"]);
        let panel = ids(&Installed::only(&[Component::Panel, Component::Updates]));
        assert_eq!(panel, ["home", "panel", "notifications", "power", "appearance", "updates", "system"]);
        assert!(ids(&Installed::only(&[Component::Idle])).contains(&"power"));
        let hypr = ids(&Installed::only(&[Component::Hyprland]));
        assert_eq!(hypr.iter().filter(|i| i.starts_with("hypr-")).count(), 11);
        assert!(!hypr.contains(&"claude"));
    }
}
