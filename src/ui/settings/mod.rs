//! Settings window. Every widget writes straight into the config store, so
//! changes apply to the launcher and the control center immediately and are
//! saved automatically.

mod apps_page;
mod binder;
mod hypr_pages;
mod hypr_rows;
mod pages;

use super::daemon::Daemon;
use super::marker::Marker;
use adw::prelude::*;
use binder::Binder;
use gtk::{gdk, glib};
use std::rc::Rc;

/// Sidebar entries: (section, page id, title, icon). Ids are `SETTINGS_PAGES`.
const ENTRIES: [(&str, &str, &str, &str); 11] = [
    ("Launcher", "general", "Launcher", "system-search-symbolic"),
    ("Launcher", "apps", "Applications", "view-grid-symbolic"),
    ("Launcher", "search", "Search", "edit-find-symbolic"),
    ("Launcher", "claude", "Claude", "vela-claude-symbolic"),
    ("Control center", "panel", "Panel", "view-dual-symbolic"),
    ("Control center", "notifications", "Notifications", "notifications-symbolic"),
    ("Control center", "power", "Power & idle", "system-shutdown-symbolic"),
    ("Hyprland", "hypr-windows", "Windows & gaps", "vela-windows-symbolic"),
    ("Hyprland", "hypr-effects", "Blur & effects", "vela-blur-symbolic"),
    ("Everywhere", "appearance", "Appearance", "applications-graphics-symbolic"),
    ("Everywhere", "system", "System", "preferences-system-symbolic"),
];

pub struct SettingsWindow {
    window: adw::ApplicationWindow,
    sidebar: gtk::ListBox,
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
        for (_, id, _, _) in ENTRIES {
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
                "appearance" => pages::appearance(&binder).upcast(),
                _ => pages::system(daemon, &binder).upcast(),
            };
            stack.add_named(&page, Some(id));
        }

        // Sidebar with a heading above each section.
        let sidebar = gtk::ListBox::builder().css_classes(["navigation-sidebar"]).build();
        for (_, _, title, icon) in ENTRIES {
            sidebar.append(&sidebar_row(title, icon));
        }
        sidebar.set_header_func(|row, before| {
            let i = row.index() as usize;
            let section = ENTRIES[i].0;
            if before.is_none_or(|b| ENTRIES[b.index() as usize].0 != section) {
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
        header.pack_end(&preview);
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
        let content_view = adw::ToolbarView::new();
        content_view.add_top_bar(&header);
        content_view.add_top_bar(&banner);
        content_view.add_top_bar(&hypr_banner);
        content_view.set_content(Some(&stack));
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
            let (stack, content_page, split, marker, store) = (stack.clone(), content_page.clone(), split.clone(), marker.clone(), daemon.store.clone());
            sidebar.connect_row_selected(move |_, row| {
                let Some(row) = row else { return };
                let (_, id, title, _) = ENTRIES[row.index() as usize];
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

        Rc::new(SettingsWindow { window, sidebar })
    }

    pub fn show_page(&self, name: &str) {
        if let Some(i) = ENTRIES.iter().position(|(_, id, _, _)| *id == name) {
            self.sidebar.select_row(self.sidebar.row_at_index(i as i32).as_ref());
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
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
