//! The launcher overlay: a layer-shell surface with a search field, an app
//! grid (empty query) and a unified result list (non-empty query).
//!
//! The search entry keeps keyboard focus at all times; navigation keys are
//! intercepted in the capture phase and move a selection in the grid/list.

use super::icons;
use super::style;
use crate::apps::catalog::{Catalog, Entry};
use crate::config::{Config, GridSource};
use crate::paths;
use crate::search::files::FileHit;
use crate::search::results::{self, Item};
use gtk::prelude::*;
use gtk::{gdk, glib, pango};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Margin around the panel inside the (transparent) surface, room for the shadow.
const PANEL_MARGIN: i32 = 18;
/// Horizontal space inside the panel that is not available for tiles.
/// (content padding + grid margin, which leaves room for the lifted,
/// glowing selected tile so it isn't clipped by the scroll view)
const CONTENT_CHROME: i32 = 2 * PANEL_MARGIN + 2 * 2 + 2 * 10 + 2;

type RequestHandler = Rc<dyn Fn(Request)>;

#[derive(Debug, Clone)]
pub enum Request {
    Query(u64, String),
    LaunchEntry(String),
    OpenPath { path: PathBuf, reveal: bool },
    Claude(String),
    OpenSettings,
    TogglePin(String),
    MovePin(String, i32),
    Hidden,
}

/// Columns of the app grid for the configured width.
pub fn grid_columns(cfg: &Config) -> usize {
    let a = &cfg.appearance;
    if a.columns > 0 {
        return a.columns as usize;
    }
    let avail = cfg.general.width as i32 - CONTENT_CHROME;
    let per = (a.tile_size + a.spacing) as i32;
    (((avail + a.spacing as i32) / per.max(1)).max(1)) as usize
}

/// Keyboard interactivity of the launcher surface.
///
/// Hyprland's `follow_mouse` hands keyboard focus to whatever the pointer
/// moves over, so an on-demand layer loses focus (and closed) as soon as the
/// mouse left it. An exclusive layer keeps focus; Hyprland then routes all
/// pointer input to it, clicks elsewhere arrive with coordinates outside the
/// panel, see `connect_signals`.
pub fn keyboard_mode(preview: bool, close_on_click_outside: bool) -> KeyboardMode {
    match (preview, close_on_click_outside) {
        (true, _) => KeyboardMode::None,
        (false, true) => KeyboardMode::Exclusive,
        (false, false) => KeyboardMode::OnDemand,
    }
}

/// Next selection index for an arrow key in a grid of `n` items.
pub fn grid_move(sel: usize, n: usize, cols: usize, key: gdk::Key) -> usize {
    if n == 0 {
        return 0;
    }
    let cols = cols.max(1);
    match key {
        gdk::Key::Right => (sel + 1).min(n - 1),
        gdk::Key::Left => sel.saturating_sub(1),
        gdk::Key::Down if sel + cols < n => sel + cols,
        // Down from the second-to-last row into a shorter last row.
        gdk::Key::Down if sel / cols < (n - 1) / cols => n - 1,
        gdk::Key::Up if sel >= cols => sel - cols,
        _ => sel,
    }
}

#[derive(Default)]
struct SearchState {
    query: String,
    catalog: Arc<Catalog>,
    app_hits: Vec<usize>,
    file_hits: Vec<FileHit>,
    files_pending: bool,
}

pub struct Launcher {
    pub window: gtk::Window,
    layer: bool,
    entry: gtk::Entry,
    stack: gtk::Stack,
    grid_scroller: gtk::ScrolledWindow,
    list_scroller: gtk::ScrolledWindow,
    list: gtk::ListBox,
    empty_label: gtk::Label,
    error: gtk::Label,
    error_revealer: gtk::Revealer,
    css: gtk::CssProvider,

    config: RefCell<Arc<Config>>,
    catalog: RefCell<Arc<Catalog>>,
    tiles: RefCell<Vec<(gtk::Button, String)>>,
    grid_sel: Cell<usize>,
    columns: Cell<usize>,
    items: RefCell<Vec<Item>>,
    state: RefCell<SearchState>,
    generation: Cell<u64>,
    had_focus: Cell<bool>,
    suppress_autohide: Cell<bool>,
    preview: Cell<bool>,
    shown_at: Cell<Option<Instant>>,
    request: RefCell<Option<RequestHandler>>,
    search_row: gtk::Box,
    chip_icons: gtk::Stack,
    mode_badge: gtk::Label,
    mode_revealer: gtk::Revealer,
    shift_held: Cell<bool>,
    anim_flip: Cell<bool>,
    closing: Rc<RefCell<Option<glib::SourceId>>>,
    /// Identity of the rows currently shown; only new rows animate in.
    shown_keys: RefCell<std::collections::HashSet<String>>,
}

impl Launcher {
    pub fn new(app: &adw::Application, config: Arc<Config>) -> Rc<Launcher> {
        let window = gtk::Window::builder().application(app).title("Vela").resizable(false).decorated(false).build();
        window.add_css_class("vela-launcher");
        let layer = gtk4_layer_shell::is_supported();
        if layer {
            window.init_layer_shell();
            window.set_layer(Layer::Overlay);
            window.set_namespace(Some("vela"));
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_anchor(Edge::Top, true);
            window.set_exclusive_zone(-1);
        } else {
            log::warn!("layer-shell not supported by the compositor; using a regular window");
        }

        let css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 10);
        }

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel.add_css_class("vela-panel");
        panel.set_overflow(gtk::Overflow::Hidden);

        // Search row
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        search_row.add_css_class("vela-search");
        // Icon chip: magnifier normally, Claude logo in Claude mode.
        let search_icon = gtk::Image::from_icon_name("system-search-symbolic");
        search_icon.set_pixel_size(20);
        search_icon.add_css_class("vela-search-icon");
        let claude_icon = gtk::Image::from_icon_name("vela-claude-symbolic");
        claude_icon.set_pixel_size(22);
        claude_icon.add_css_class("vela-mode-claude");
        let chip_icons = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(180)
            .build();
        chip_icons.add_named(&search_icon, Some("search"));
        chip_icons.add_named(&claude_icon, Some("claude"));
        let chip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        chip.add_css_class("vela-search-chip");
        chip.set_valign(gtk::Align::Center);
        chip.set_halign(gtk::Align::Start);
        chip.set_hexpand(false);
        chip_icons.set_halign(gtk::Align::Center);
        chip_icons.set_valign(gtk::Align::Center);
        chip.append(&chip_icons);
        let mode_badge = gtk::Label::new(Some("Claude"));
        mode_badge.add_css_class("vela-mode-badge");
        let mode_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .child(&mode_badge)
            .valign(gtk::Align::Center)
            .build();
        let entry = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text("Search apps, files, or ask Claude…")
            .build();
        entry.add_css_class("vela-entry");
        entry.set_truncate_multiline(false);
        let gear = gtk::Button::from_icon_name("emblem-system-symbolic");
        gear.add_css_class("vela-gear");
        gear.set_focusable(false);
        gear.set_tooltip_text(Some("Settings (Ctrl+,)"));
        gear.set_valign(gtk::Align::Center);
        search_row.append(&chip);
        search_row.append(&entry);
        search_row.append(&mode_revealer);
        search_row.append(&gear);

        let divider = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        divider.add_css_class("vela-divider");

        let grid_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .build();
        let list = gtk::ListBox::new();
        list.add_css_class("vela-results");
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        list.set_header_func(|row, before| {
            let section = row.widget_name();
            if before.is_none_or(|b| b.widget_name() != section) {
                let label = gtk::Label::new(Some(&section.to_uppercase()));
                label.set_xalign(0.0);
                label.add_css_class("vela-section");
                row.set_header(Some(&label));
            } else {
                row.set_header(None::<&gtk::Widget>);
            }
        });
        let list_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        let empty_label = gtk::Label::new(None);
        empty_label.add_css_class("vela-empty");
        empty_label.set_wrap(true);

        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        stack.set_hhomogeneous(true);
        stack.set_interpolate_size(false);
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.add_named(&grid_scroller, Some("grid"));
        stack.add_named(&list_scroller, Some("results"));
        stack.add_named(&empty_label, Some("empty"));
        stack.add_css_class("vela-content");

        let error = gtk::Label::new(None);
        error.add_css_class("vela-error");
        error.set_wrap(true);
        error.set_xalign(0.0);
        let error_revealer = gtk::Revealer::builder()
            .child(&error)
            .reveal_child(false)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .build();

        panel.append(&search_row);
        panel.append(&divider);
        panel.append(&stack);
        panel.append(&error_revealer);
        window.set_child(Some(&panel));

        let this = Rc::new(Launcher {
            window,
            layer,
            entry,
            stack,
            grid_scroller,
            list_scroller,
            list,
            empty_label,
            error,
            error_revealer,
            css,
            config: RefCell::new(config.clone()),
            catalog: RefCell::new(Arc::new(Catalog::default())),
            tiles: RefCell::default(),
            grid_sel: Cell::new(0),
            columns: Cell::new(1),
            items: RefCell::default(),
            state: RefCell::default(),
            generation: Cell::new(0),
            had_focus: Cell::new(false),
            suppress_autohide: Cell::new(false),
            preview: Cell::new(false),
            shown_at: Cell::new(None),
            request: RefCell::default(),
            search_row: search_row.clone(),
            chip_icons,
            mode_badge,
            mode_revealer,
            shift_held: Cell::new(false),
            anim_flip: Cell::new(false),
            closing: Rc::default(),
            shown_keys: RefCell::default(),
        });

        this.connect_signals(&gear);
        this.apply_config(&config, &config);
        this
    }

    pub fn set_request_handler(&self, f: impl Fn(Request) + 'static) {
        *self.request.borrow_mut() = Some(Rc::new(f));
        // Tiles capture the handler when they are built.
        self.rebuild_grid();
    }

    fn request(&self, r: Request) {
        let handler = self.request.borrow().clone();
        if let Some(h) = handler {
            h(r);
        }
    }

    fn connect_signals(self: &Rc<Self>, gear: &gtk::Button) {
        let weak = Rc::downgrade(self);
        gear.connect_clicked(move |_| {
            if let Some(l) = weak.upgrade() {
                l.request(Request::OpenSettings);
            }
        });

        let weak = Rc::downgrade(self);
        self.entry.connect_changed(move |e| {
            if let Some(l) = weak.upgrade() {
                l.on_query_changed(e.text().as_str());
            }
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, mods| match weak.upgrade() {
            Some(l) => {
                if matches!(key, gdk::Key::Shift_L | gdk::Key::Shift_R) {
                    l.shift_held.set(true);
                    l.update_mode();
                }
                l.on_key(key, mods)
            }
            None => glib::Propagation::Proceed,
        });
        let weak = Rc::downgrade(self);
        keys.connect_key_released(move |_, key, _, _| {
            if let Some(l) = weak.upgrade()
                && matches!(key, gdk::Key::Shift_L | gdk::Key::Shift_R)
            {
                l.shift_held.set(false);
                l.update_mode();
            }
        });
        let weak = Rc::downgrade(self);
        self.list.connect_selected_rows_changed(move |_| {
            if let Some(l) = weak.upgrade() {
                l.update_mode();
            }
        });
        self.window.add_controller(keys);

        let weak = Rc::downgrade(self);
        self.list.connect_row_activated(move |_, row| {
            if let Some(l) = weak.upgrade() {
                l.activate_item(row.index().max(0) as usize, gdk::ModifierType::empty());
            }
        });
        // With an exclusive layer, Hyprland delivers clicks anywhere on the
        // screen to the launcher; outside the panel they mean "click elsewhere".
        let click = gtk::GestureClick::builder().button(0).propagation_phase(gtk::PropagationPhase::Capture).build();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |g, _, x, y| {
            let Some(l) = weak.upgrade() else { return };
            let inside = l
                .window
                .child()
                .and_then(|panel| panel.compute_bounds(&l.window))
                .is_some_and(|b| b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32)));
            log::debug!("launcher click at {x:.0},{y:.0} inside={inside}");
            if !inside && l.is_visible() && !l.preview.get() && l.config.borrow().general.close_on_focus_loss {
                g.set_state(gtk::EventSequenceState::Claimed);
                l.hide();
            }
        });
        self.window.add_controller(click);

        let weak = Rc::downgrade(self);
        self.window.connect_is_active_notify(move |w| {
            let Some(l) = weak.upgrade() else { return };
            log::debug!("launcher active={} had_focus={} visible={}", w.is_active(), l.had_focus.get(), w.is_visible());
            if w.is_active() {
                l.had_focus.set(true);
                return;
            }
            let recently_shown = l.shown_at.get().is_some_and(|t| t.elapsed() < Duration::from_millis(150));
            if l.had_focus.get()
                && w.is_visible()
                && !l.suppress_autohide.get()
                && !l.preview.get()
                && !recently_shown
                && l.config.borrow().general.close_on_focus_loss
            {
                l.hide();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(l) = weak.upgrade() {
                l.hide();
            }
            glib::Propagation::Stop
        });
    }

    // ------------------------------------------------------------ visibility

    /// Visible and not in the middle of the closing animation.
    pub fn is_visible(&self) -> bool {
        self.window.is_visible() && self.closing.borrow().is_none()
    }

    fn cancel_closing(&self) {
        if let Some(id) = self.closing.borrow_mut().take() {
            id.remove();
            self.window.remove_css_class("closing");
            self.window.set_visible(false);
        }
    }

    /// Restarts the entrance animations by switching keyframe sets.
    fn restart_animations(&self) {
        let flip = !self.anim_flip.get();
        self.anim_flip.set(flip);
        let (on, off) = if flip { ("anim-a", "anim-b") } else { ("anim-b", "anim-a") };
        self.window.remove_css_class(off);
        self.window.add_css_class(on);
    }

    fn pick_monitor(&self) -> Option<gdk::Monitor> {
        let display = gdk::Display::default()?;
        let monitors = display.monitors();
        let all: Vec<gdk::Monitor> = (0..monitors.n_items())
            .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
            .collect();
        let name = crate::hyprland::focused_monitor();
        name.and_then(|n| all.iter().find(|m| m.connector().is_some_and(|c| c == n)).cloned())
    }

    fn place(&self) {
        if !self.layer {
            return;
        }
        let monitor = self.pick_monitor();
        self.window.set_monitor(monitor.as_ref());
        let height = monitor
            .clone()
            .or_else(|| gdk::Display::default().and_then(|d| d.monitors().item(0).and_downcast::<gdk::Monitor>()))
            .map_or(1080, |m| m.geometry().height());
        let pct = self.config.borrow().general.vertical_position as i32;
        self.window.set_margin(Edge::Top, (height * pct / 100 - PANEL_MARGIN).max(0));
    }

    pub fn show(&self, preview: bool) {
        self.preview.set(preview);
        if self.layer {
            let click_outside = self.config.borrow().general.close_on_focus_loss;
            self.window.set_keyboard_mode(keyboard_mode(preview, click_outside));
        }
        let reopening = self.closing.borrow().is_some();
        self.cancel_closing();
        if !self.window.is_visible() || reopening {
            self.restart_animations();
            self.entry.set_text("");
            self.hide_error();
            self.grid_sel.set(0);
            self.refresh_grid_selection();
            self.grid_scroller.vadjustment().set_value(0.0);
            self.place();
        }
        self.had_focus.set(false);
        self.shift_held.set(false);
        self.shown_at.set(Some(Instant::now()));
        self.show_mode();
        self.window.present();
        if !preview {
            self.entry.grab_focus();
        }
    }

    pub fn hide(&self) {
        if !self.is_visible() {
            return;
        }
        self.preview.set(false);
        self.had_focus.set(false);
        match style::motion(&self.config.borrow()) {
            Some(m) => {
                self.window.add_css_class("closing");
                let w = self.window.downgrade();
                let closing = self.closing.clone();
                let id = glib::timeout_add_local_once(Duration::from_millis(u64::from(m.close)), move || {
                    closing.borrow_mut().take();
                    if let Some(w) = w.upgrade() {
                        w.set_visible(false);
                        w.remove_css_class("closing");
                    }
                });
                *self.closing.borrow_mut() = Some(id);
            }
            None => self.window.set_visible(false),
        }
        self.request(Request::Hidden);
    }

    pub fn toggle(&self) {
        if self.is_visible() && !self.preview.get() {
            self.hide();
        } else {
            self.show(false);
        }
    }

    pub fn is_preview(&self) -> bool {
        self.preview.get() && self.is_visible()
    }

    pub fn show_error(&self, msg: &str) {
        self.error.set_text(msg);
        self.error_revealer.set_reveal_child(true);
        // Re-trigger the shake for repeated errors.
        self.error.remove_css_class("shake");
        let e = self.error.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(e) = e.upgrade() {
                e.add_css_class("shake");
            }
        });
    }

    fn hide_error(&self) {
        self.error_revealer.set_reveal_child(false);
    }

    // ---------------------------------------------------------------- config

    pub fn apply_config(&self, old: &Config, new: &Config) {
        self.css.load_from_string(&style::launcher_css(new));
        let crossfade = style::motion(new).map_or(0, |m| m.crossfade);
        self.stack.set_transition_duration(crossfade);
        self.error_revealer.set_transition_duration(crossfade + 40);
        *self.config.borrow_mut() = Arc::new(new.clone());
        // The surface is exactly `width` wide, shadow margin included.
        self.window.set_size_request(new.general.width as i32, -1);
        let chrome = 150;
        let max = (new.general.max_height as i32 - chrome).max(120);
        self.grid_scroller.set_max_content_height(max);
        self.list_scroller.set_max_content_height(max);
        if old.general.vertical_position != new.general.vertical_position && self.is_visible() {
            self.place();
        }
        self.rebuild_grid();
        if old.search != new.search || old.claude != new.claude || old.general.max_results != new.general.max_results || old.apps != new.apps {
            // Limits/sources changed: ask again so results reflect them.
            let q = self.state.borrow().query.clone();
            if !q.trim().is_empty() {
                self.send_query(&q);
            }
            self.rebuild_results();
        }
        // Shrink the surface when the content got smaller.
        self.window.set_default_size(-1, -1);
    }

    pub fn set_catalog(&self, catalog: Arc<Catalog>) {
        *self.catalog.borrow_mut() = catalog;
        self.rebuild_grid();
    }

    // ------------------------------------------------------------------ grid

    fn rebuild_grid(&self) {
        let cfg = self.config.borrow().clone();
        let catalog = self.catalog.borrow().clone();
        let entries: Vec<&Entry> = catalog.grid(&cfg.apps);
        let cols = grid_columns(&cfg);
        self.columns.set(cols);

        let grid = gtk::Grid::builder()
            .column_spacing(cfg.appearance.spacing as i32)
            .row_spacing(cfg.appearance.spacing as i32)
            .halign(gtk::Align::Center)
            .column_homogeneous(true)
            .build();
        grid.add_css_class("vela-grid");
        let mut tiles = Vec::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            let tile = self.make_tile(e, &cfg);
            tile.add_css_class(&format!("vela-d{}", i.min(style::MAX_STAGGER)));
            grid.attach(&tile, (i % cols) as i32, (i / cols) as i32, 1, 1);
            tiles.push((tile, e.key.clone()));
        }
        self.grid_scroller.set_child(Some(&grid));
        *self.tiles.borrow_mut() = tiles;
        let n = self.tiles.borrow().len();
        if self.grid_sel.get() >= n {
            self.grid_sel.set(n.saturating_sub(1));
        }
        self.refresh_grid_selection();
        if self.state.borrow().query.is_empty() {
            self.show_mode();
        }
    }

    fn make_tile(&self, e: &Entry, cfg: &Config) -> gtk::Button {
        let a = &cfg.appearance;
        let tile = gtk::Button::new();
        tile.add_css_class("vela-tile");
        tile.set_focusable(false);
        tile.set_size_request(a.tile_size as i32, a.tile_size as i32);
        tile.set_tooltip_text(Some(&if e.subtitle.is_empty() {
            e.name.clone()
        } else {
            format!("{}\n{}", e.name, e.subtitle)
        }));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_valign(gtk::Align::Center);
        content.append(&icons::app_image(e.icon.as_deref(), a.icon_size as i32));
        if a.show_labels {
            let label = gtk::Label::new(Some(&e.name));
            label.add_css_class("vela-tile-label");
            label.set_wrap(true);
            label.set_wrap_mode(pango::WrapMode::WordChar);
            label.set_lines(2);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.set_justify(gtk::Justification::Center);
            label.set_max_width_chars(((a.tile_size as i32 - 12) / 7).max(4));
            content.append(&label);
        }
        tile.set_child(Some(&content));

        // No Launcher reference is captured strongly by widgets it owns.
        let key = e.key.clone();
        let handler = self.request.borrow().clone();
        tile.connect_clicked(move |_| {
            if let Some(h) = &handler {
                h(Request::LaunchEntry(key.clone()));
            }
        });
        let key = e.key.clone();
        let handler = self.request.borrow().clone();
        let pinned = cfg.apps.pinned.contains(&e.key);
        let reorderable = pinned && cfg.apps.grid != GridSource::All;
        let click = gtk::GestureClick::builder().button(3).build();
        let tile_weak = tile.downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let (Some(tile), Some(h)) = (tile_weak.upgrade(), &handler) {
                context_menu(tile.upcast_ref(), &key, pinned, reorderable, h.clone());
            }
        });
        tile.add_controller(click);
        tile
    }

    fn refresh_grid_selection(&self) {
        let sel = self.grid_sel.get();
        for (i, (tile, _)) in self.tiles.borrow().iter().enumerate() {
            if i == sel {
                tile.add_css_class("selected");
            } else {
                tile.remove_css_class("selected");
            }
        }
        if let Some((tile, _)) = self.tiles.borrow().get(sel) {
            scroll_into_view(&self.grid_scroller, tile.upcast_ref());
        }
    }

    // --------------------------------------------------------------- search

    fn send_query(&self, text: &str) {
        let g = self.generation.get() + 1;
        self.generation.set(g);
        self.request(Request::Query(g, text.to_owned()));
    }

    fn on_query_changed(&self, text: &str) {
        self.hide_error();
        let words = crate::search::files::query_words(text);
        {
            let mut st = self.state.borrow_mut();
            let extends = !st.query.is_empty() && text.starts_with(st.query.as_str());
            st.query = text.to_owned();
            // Typing forward only narrows results: keep old file hits that
            // still match so the list doesn't flicker while plocate runs.
            if extends {
                st.file_hits.retain(|h| {
                    let p = h.path.to_string_lossy().to_lowercase();
                    words.iter().all(|w| p.contains(w.as_str()))
                });
            } else {
                st.file_hits.clear();
            }
            if text.trim().is_empty() {
                st.app_hits.clear();
                st.file_hits.clear();
            }
        }
        if text.trim().is_empty() {
            self.search_row.remove_css_class("active");
            self.shown_keys.borrow_mut().clear();
            self.show_mode();
            self.update_mode();
            return;
        }
        self.search_row.add_css_class("active");
        self.send_query(text);
        self.rebuild_results();
    }

    pub fn on_app_results(&self, generation: u64, catalog: Arc<Catalog>, hits: Vec<usize>) {
        if generation != self.generation.get() {
            return;
        }
        {
            let mut st = self.state.borrow_mut();
            st.catalog = catalog;
            st.app_hits = hits;
        }
        self.rebuild_results();
    }

    pub fn on_file_results(&self, generation: u64, hits: Vec<FileHit>, pending: bool) {
        if generation != self.generation.get() {
            return;
        }
        {
            let mut st = self.state.borrow_mut();
            st.file_hits = hits;
            st.files_pending = pending;
        }
        self.rebuild_results();
    }

    fn show_mode(&self) {
        let query_empty = self.state.borrow().query.trim().is_empty();
        let name = if query_empty {
            if self.tiles.borrow().is_empty() {
                self.empty_label.set_text("No applications to show yet.\nPin some in the settings (Ctrl+,).");
                "empty"
            } else {
                "grid"
            }
        } else if self.items.borrow().is_empty() {
            let pending = self.state.borrow().files_pending;
            self.empty_label.set_text(if pending { "Indexing files…" } else { "No results" });
            "empty"
        } else {
            "results"
        };
        self.stack.set_visible_child_name(name);
    }

    fn rebuild_results(&self) {
        let cfg = self.config.borrow().clone();
        let items = {
            let st = self.state.borrow();
            results::assemble(&cfg, &st.query, &st.catalog, &st.app_hits, &st.file_hits)
        };
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let query = self.state.borrow().query.clone();
        let mut shown = self.shown_keys.borrow_mut();
        let mut new_count = 0;
        let mut keys = std::collections::HashSet::with_capacity(items.len());
        for item in &items {
            let row = self.make_row(item, &query);
            let key = item.key();
            if !shown.contains(&key) {
                row.add_css_class("new");
                row.add_css_class(&format!("vela-d{}", new_count.min(style::MAX_STAGGER)));
                new_count += 1;
            }
            keys.insert(key);
            self.list.append(&row);
        }
        *shown = keys;
        drop(shown);
        let has = !items.is_empty();
        *self.items.borrow_mut() = items;
        if has {
            self.list.select_row(self.list.row_at_index(0).as_ref());
            self.list_scroller.vadjustment().set_value(0.0);
        }
        self.show_mode();
        self.update_mode();
    }

    fn make_row(&self, item: &Item, query: &str) -> gtk::ListBoxRow {
        let (icon, title, subtitle, badge): (gtk::Image, String, String, String) = match item {
            Item::App { catalog, index } => {
                let e = &catalog.entries[*index];
                let badge = if e.is_action { "Action" } else { "Application" };
                (icons::app_image(e.icon.as_deref(), 32), e.name.clone(), e.subtitle.clone(), badge.into())
            }
            Item::File(hit) => {
                let name = hit
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| hit.path.display().to_string());
                let parent = hit.path.parent().map(paths::display_path).unwrap_or_default();
                (
                    icons::file_image(&hit.path, hit.is_dir, 32),
                    name,
                    parent,
                    if hit.is_dir { "Folder".into() } else { "File".into() },
                )
            }
            Item::Path(p) => {
                let is_dir = p.is_dir();
                (
                    icons::file_image(p, is_dir, 32),
                    paths::display_path(p),
                    "Open this location".into(),
                    "Open".into(),
                )
            }
            Item::Claude => {
                let img = icons::named_image("vela-claude-symbolic", 26);
                img.add_css_class("vela-claude-icon");
                let preview: String = query.trim().replace('\n', " ⏎ ").chars().take(160).collect();
                let hint = if self.config.borrow().claude.shift_enter { "⇧↵" } else { "↵" };
                (img, "Ask Claude".into(), format!("“{preview}”"), hint.into())
            }
        };
        let row = gtk::ListBoxRow::new();
        row.set_focusable(false);
        row.set_widget_name(item.section());
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        icon.set_size_request(36, 36);
        b.append(&icon);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_valign(gtk::Align::Center);
        text.set_hexpand(true);
        let t = gtk::Label::new(Some(&title));
        t.add_css_class("vela-row-title");
        t.set_xalign(0.0);
        t.set_ellipsize(pango::EllipsizeMode::End);
        text.append(&t);
        if !subtitle.is_empty() {
            let s = gtk::Label::new(Some(&subtitle));
            s.add_css_class("vela-row-subtitle");
            s.set_xalign(0.0);
            s.set_ellipsize(if matches!(item, Item::File(_)) {
                pango::EllipsizeMode::Middle
            } else {
                pango::EllipsizeMode::End
            });
            text.append(&s);
        }
        b.append(&text);
        let badge_label = gtk::Label::new(Some(&badge));
        badge_label.add_css_class("vela-row-badge");
        badge_label.set_valign(gtk::Align::Center);
        b.append(&badge_label);
        row.set_child(Some(&b));

        if let Item::App { catalog, index } = item {
            let e = &catalog.entries[*index];
            if !e.is_action {
                let key = e.key.clone();
                let pinned = self.config.borrow().apps.pinned.contains(&key);
                let handler = self.request.borrow().clone();
                let click = gtk::GestureClick::builder().button(3).build();
                let row_weak = row.downgrade();
                click.connect_pressed(move |_, _, _, _| {
                    if let (Some(row), Some(h)) = (row_weak.upgrade(), &handler) {
                        context_menu(row.upcast_ref(), &key, pinned, false, h.clone());
                    }
                });
                row.add_controller(click);
            }
        }
        row
    }

    fn selected_index(&self) -> Option<usize> {
        self.list.selected_row().map(|r| r.index().max(0) as usize)
    }

    fn select_list(&self, idx: usize) {
        if let Some(row) = self.list.row_at_index(idx as i32) {
            self.list.select_row(Some(&row));
            scroll_into_view(&self.list_scroller, row.upcast_ref());
        }
    }

    /// Whether Enter / Shift+Enter would currently go to Claude.
    fn claude_mode(&self) -> bool {
        let cfg = self.config.borrow();
        let query = self.state.borrow().query.clone();
        if !cfg.search.claude || query.trim().is_empty() {
            return false;
        }
        if self.shift_held.get() && cfg.claude.shift_enter {
            return true;
        }
        let selected_claude = self.selected_index().is_some_and(|i| matches!(self.items.borrow().get(i), Some(Item::Claude)));
        selected_claude || (cfg.claude.prefer_for_questions && results::looks_like_question(&query))
    }

    fn update_mode(&self) {
        let claude = self.claude_mode();
        self.chip_icons.set_visible_child_name(if claude { "claude" } else { "search" });
        self.mode_badge.set_text(if self.config.borrow().claude.shift_enter && !self.shift_held.get() {
            "Claude  ⇧↵"
        } else {
            "Claude  ↵"
        });
        self.mode_revealer.set_reveal_child(claude);
        if claude {
            self.search_row.add_css_class("claude");
        } else {
            self.search_row.remove_css_class("claude");
        }
    }

    // ------------------------------------------------------------- keyboard

    fn on_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        use glib::Propagation::{Proceed, Stop};
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let query = self.entry.text().to_string();
        let grid_mode = query.trim().is_empty();

        match key {
            gdk::Key::Escape => {
                self.hide();
                Stop
            }
            gdk::Key::comma if ctrl => {
                self.request(Request::OpenSettings);
                Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                let cfg = self.config.borrow().clone();
                if shift && cfg.claude.shift_enter && !grid_mode {
                    self.request(Request::Claude(query));
                } else if grid_mode {
                    if let Some((_, key)) = self.tiles.borrow().get(self.grid_sel.get()) {
                        self.request(Request::LaunchEntry(key.clone()));
                    }
                } else if let Some(i) = self.selected_index() {
                    self.activate_item(i, mods);
                }
                Stop
            }
            gdk::Key::Up | gdk::Key::Down | gdk::Key::Left | gdk::Key::Right if grid_mode => {
                let n = self.tiles.borrow().len();
                self.grid_sel.set(grid_move(self.grid_sel.get(), n, self.columns.get(), key));
                self.refresh_grid_selection();
                Stop
            }
            gdk::Key::Down | gdk::Key::Tab | gdk::Key::Up | gdk::Key::ISO_Left_Tab | gdk::Key::Page_Down | gdk::Key::Page_Up if !grid_mode => {
                let n = self.items.borrow().len();
                if n > 0 {
                    let cur = self.selected_index().unwrap_or(0);
                    let next = match key {
                        gdk::Key::Down | gdk::Key::Tab if !shift => (cur + 1).min(n - 1),
                        gdk::Key::Page_Down => (cur + 5).min(n - 1),
                        gdk::Key::Page_Up => cur.saturating_sub(5),
                        _ => cur.saturating_sub(1),
                    };
                    self.select_list(next);
                }
                Stop
            }
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab => Stop,
            _ => {
                if !self.entry.has_focus() && !self.preview.get() {
                    self.entry.grab_focus_without_selecting();
                }
                Proceed
            }
        }
    }

    fn activate_item(&self, idx: usize, mods: gdk::ModifierType) {
        let item = self.items.borrow().get(idx).cloned();
        let reveal = mods.contains(gdk::ModifierType::CONTROL_MASK);
        match item {
            Some(Item::App { catalog, index }) => self.request(Request::LaunchEntry(catalog.entries[index].key.clone())),
            Some(Item::File(hit)) => self.request(Request::OpenPath { path: hit.path, reveal }),
            Some(Item::Path(p)) => self.request(Request::OpenPath { path: p, reveal }),
            Some(Item::Claude) => self.request(Request::Claude(self.entry.text().to_string())),
            None => {}
        }
    }
}

fn scroll_into_view(scroller: &gtk::ScrolledWindow, widget: &gtk::Widget) {
    let Some(child) = scroller.child() else { return };
    let Some(bounds) = widget.compute_bounds(&child) else { return };
    let adj = scroller.vadjustment();
    let (top, bottom) = (f64::from(bounds.y()), f64::from(bounds.y() + bounds.height()));
    if top < adj.value() {
        adj.set_value(top - 6.0);
    } else if bottom > adj.value() + adj.page_size() {
        adj.set_value(bottom - adj.page_size() + 6.0);
    }
}

/// Right-click menu for tiles and app rows.
fn context_menu(anchor: &gtk::Widget, key: &str, pinned: bool, reorderable: bool, handler: Rc<dyn Fn(Request)>) {
    let pop = gtk::Popover::new();
    pop.set_parent(anchor);
    pop.set_has_arrow(false);
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let add = |label: &str, req: Request| {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("flat");
        if let Some(l) = btn.child().and_downcast::<gtk::Label>() {
            l.set_xalign(0.0);
        }
        let h = handler.clone();
        let pw = pop.downgrade();
        btn.connect_clicked(move |_| {
            if let Some(p) = pw.upgrade() {
                p.popdown();
            }
            h(req.clone());
        });
        b.append(&btn);
    };
    add("Open", Request::LaunchEntry(key.to_owned()));
    add(if pinned { "Unpin from grid" } else { "Pin to grid" }, Request::TogglePin(key.to_owned()));
    if reorderable {
        add("Move left", Request::MovePin(key.to_owned(), -1));
        add("Move right", Request::MovePin(key.to_owned(), 1));
    }
    pop.set_child(Some(&b));
    pop.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    pop.popup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_columns_follow_width_and_tile_size() {
        let mut cfg = Config::default();
        cfg.general.width = 760;
        cfg.appearance.tile_size = 108;
        cfg.appearance.spacing = 8;
        assert_eq!(grid_columns(&cfg), 6);
        cfg.general.width = 400;
        assert_eq!(grid_columns(&cfg), 2);
        cfg.appearance.columns = 9;
        assert_eq!(grid_columns(&cfg), 9);
    }

    #[test]
    fn launcher_keeps_the_keyboard_while_it_closes_on_click_outside() {
        assert_eq!(keyboard_mode(false, true), KeyboardMode::Exclusive);
        assert_eq!(
            keyboard_mode(false, false),
            KeyboardMode::OnDemand,
            "without click-outside other windows stay usable"
        );
        assert_eq!(keyboard_mode(true, true), KeyboardMode::None, "settings preview never takes the keyboard");
    }

    #[test]
    fn arrow_navigation_in_grid() {
        use gdk::Key;
        // 7 items, 3 columns:  0 1 2 / 3 4 5 / 6
        assert_eq!(grid_move(0, 7, 3, Key::Right), 1);
        assert_eq!(grid_move(6, 7, 3, Key::Right), 6);
        assert_eq!(grid_move(0, 7, 3, Key::Left), 0);
        assert_eq!(grid_move(1, 7, 3, Key::Down), 4);
        assert_eq!(grid_move(4, 7, 3, Key::Down), 6);
        assert_eq!(grid_move(5, 7, 3, Key::Down), 6);
        assert_eq!(grid_move(6, 7, 3, Key::Down), 6);
        assert_eq!(grid_move(4, 7, 3, Key::Up), 1);
        assert_eq!(grid_move(1, 7, 3, Key::Up), 1);
        assert_eq!(grid_move(0, 0, 3, Key::Down), 0);
    }
}
