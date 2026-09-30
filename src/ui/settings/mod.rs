//! Settings window. Every widget writes straight into the config store, so
//! changes apply to the launcher immediately and are saved automatically.

mod apps_page;
mod binder;
mod pages;

use super::daemon::Daemon;
use adw::prelude::*;
use binder::Binder;
use gtk::glib;
use std::rc::Rc;

pub struct SettingsWindow {
    window: adw::ApplicationWindow,
    stack: adw::ViewStack,
}

impl SettingsWindow {
    pub fn new(daemon: &Rc<Daemon>) -> Rc<SettingsWindow> {
        let window = adw::ApplicationWindow::builder()
            .application(&daemon.app)
            .title("Vela Settings")
            .default_width(820)
            .default_height(760)
            .icon_name("vela")
            .build();
        let binder = Binder::new(daemon.store.clone());

        let stack = adw::ViewStack::new();
        let general = pages::general(daemon, &binder);
        let appearance = pages::appearance(&binder);
        let apps = apps_page::build(daemon, &binder);
        let search = pages::search(daemon, &binder);
        let claude = pages::claude(&binder);
        stack.add_titled_with_icon(&general, Some("general"), "General", "preferences-system-symbolic");
        stack.add_titled_with_icon(&appearance, Some("appearance"), "Appearance", "applications-graphics-symbolic");
        stack.add_titled_with_icon(&apps, Some("apps"), "Applications", "view-grid-symbolic");
        stack.add_titled_with_icon(&search, Some("search"), "Search", "system-search-symbolic");
        stack.add_titled_with_icon(&claude, Some("claude"), "Claude", "vela-claude-symbolic");

        let header = adw::HeaderBar::new();
        let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
        header.set_title_widget(Some(&switcher));
        let preview = gtk::ToggleButton::builder()
            .icon_name("view-reveal-symbolic")
            .tooltip_text("Show a live preview of the launcher")
            .build();
        header.pack_start(&preview);

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

        let bottom = adw::ViewSwitcherBar::builder().stack(&stack).build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_top_bar(&banner);
        toolbar.set_content(Some(&stack));
        toolbar.add_bottom_bar(&bottom);
        window.set_content(Some(&toolbar));

        // Narrow windows: switcher moves to the bottom.
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 640sp").expect("valid breakpoint"));
        bp.add_setter(&switcher, "visible", Some(&false.to_value()));
        bp.add_setter(&bottom, "reveal", Some(&true.to_value()));
        window.add_breakpoint(bp);

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

        Rc::new(SettingsWindow { window, stack })
    }

    pub fn show_page(&self, name: &str) {
        self.stack.set_visible_child_name(name);
    }

    pub fn present(&self) {
        self.window.present();
    }
}
