//! Start page of the settings: a search field in the middle; matching
//! settings show up right below it and work there. The rows aren't copies:
//! they are borrowed from their pages and put back when the search changes
//! or another page is opened, so every setting is searchable as it is.
//! Free text can go to Claude (Ctrl+Enter), which changes it through the
//! vela MCP server.

use crate::search_terms;
use crate::ui::daemon::Daemon;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

const MAX_RESULTS: usize = 40;

type OpenPage = Rc<dyn Fn(&str)>;

/// A row that can be found: where it lives and what it is about.
struct Item {
    row: glib::WeakRef<adw::PreferencesRow>,
    page: String,
    page_title: String,
    /// Group (and expander) titles above it.
    context: Vec<String>,
}

struct Borrowed {
    row: adw::PreferencesRow,
    parent: gtk::ListBox,
    index: i32,
    visible: bool,
}

pub struct Search {
    pub widget: gtk::Widget,
    entry: gtk::SearchEntry,
    results: gtk::Box,
    items: RefCell<Option<Vec<Item>>>,
    borrowed: RefCell<Vec<Borrowed>>,
    /// (page id, page title, page widget) of every other page.
    pages: RefCell<Vec<(String, String, gtk::Widget)>>,
    before_index: RefCell<Vec<Rc<dyn Fn()>>>,
    open_page: RefCell<Option<OpenPage>>,
    daemon: Rc<Daemon>,
}

fn row_text(row: &adw::PreferencesRow) -> (String, String) {
    let title = row.title().to_string();
    let subtitle = row
        .downcast_ref::<adw::ActionRow>()
        .and_then(|r| r.subtitle())
        .or_else(|| row.downcast_ref::<adw::ExpanderRow>().map(|r| r.subtitle()))
        .map(|s| s.to_string())
        .unwrap_or_default();
    (title, subtitle)
}

/// Collects findable rows below `w`. Expanders count themselves and their
/// inner rows; other rows aren't looked into.
fn walk(w: &gtk::Widget, ctx: &mut Vec<String>, out: &mut Vec<(adw::PreferencesRow, Vec<String>)>) {
    let mut pushed = false;
    if let Some(g) = w.downcast_ref::<adw::PreferencesGroup>() {
        ctx.push(g.title().to_string());
        pushed = true;
    }
    if let Some(row) = w.downcast_ref::<adw::PreferencesRow>() {
        if row.parent().is_some_and(|p| p.is::<gtk::ListBox>()) && !row.title().is_empty() {
            out.push((row.clone(), ctx.clone()));
        }
        if !row.is::<adw::ExpanderRow>() {
            return;
        }
        ctx.push(row.title().to_string());
        pushed = true;
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        walk(&c, ctx, out);
        child = c.next_sibling();
    }
    if pushed {
        ctx.pop();
    }
}

/// How well a row matches; None if it doesn't. Lower is better.
/// `groups` from `search_terms::groups` (German words mapped to English).
fn score(groups: &[Vec<String>], title: &str, hay: &str) -> Option<u8> {
    if !search_terms::matches(groups, hay) {
        return None;
    }
    let t = title.to_lowercase();
    Some(if groups[0].iter().any(|a| t.starts_with(a.as_str())) {
        0
    } else if search_terms::matches(groups, &t) {
        1
    } else {
        2
    })
}

/// What Claude gets: the request plus where to do it.
pub fn claude_prompt(request: &str) -> String {
    format!(
        "Change my desktop settings: “{}”\n\nUse the vela MCP tools (search_hyprland_options, set_hyprland_options, \
         set_hyprland_animation, list_monitors, set_monitor, get_vela_settings, set_vela_setting) to do it right away; \
         changes apply live and are kept. Only ask back if the request is ambiguous or would turn a screen off. \
         Finish with one short line saying what you changed.",
        request.trim()
    )
}

impl Search {
    pub fn new(daemon: &Rc<Daemon>) -> Rc<Search> {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Search settings, or describe a change for Claude")
            .css_classes(["vela-home-search"])
            .hexpand(true)
            .build();
        let icon = gtk::Image::builder().icon_name("vela").pixel_size(64).build();
        let title = gtk::Label::builder().label("What do you want to change?").css_classes(["title-2"]).build();
        let hint = gtk::Label::builder()
            .label("Every vela and Hyprland setting is in here. Describe a change and press Ctrl+Enter to let Claude make it.")
            .css_classes(["dim-label"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        // Same height above and below the field: with the column centred,
        // the field sits exactly in the middle of the window.
        const AROUND: i32 = 150;
        let head = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .valign(gtk::Align::End)
            .vexpand(true)
            .build();
        head.append(&icon);
        head.append(&title);
        let head_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
        head_area.set_size_request(-1, AROUND);
        head_area.append(&head);
        let head_rev = gtk::Revealer::builder()
            .child(&head_area)
            .reveal_child(true)
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(220)
            .build();
        hint.set_valign(gtk::Align::Start);
        let hint_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
        hint_area.set_size_request(-1, AROUND);
        hint_area.append(&hint);
        let hint_rev = gtk::Revealer::builder()
            .child(&hint_area)
            .reveal_child(true)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(220)
            .build();
        let results = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .valign(gtk::Align::Center)
            .margin_top(24)
            .margin_bottom(32)
            .margin_start(18)
            .margin_end(18)
            .build();
        column.append(&head_rev);
        column.append(&entry);
        column.append(&hint_rev);
        column.append(&results);
        let clamp = adw::Clamp::builder().maximum_size(720).child(&column).vexpand(true).build();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .vexpand(true)
            .build();

        let search = Rc::new(Search {
            widget: scroll.upcast(),
            entry: entry.clone(),
            results,
            items: RefCell::default(),
            borrowed: RefCell::default(),
            pages: RefCell::default(),
            before_index: RefCell::default(),
            open_page: RefCell::default(),
            daemon: daemon.clone(),
        });
        {
            let (s, head_rev, hint_rev, column) = (Rc::downgrade(&search), head_rev.downgrade(), hint_rev.downgrade(), column.downgrade());
            entry.connect_search_changed(move |e| {
                let Some(s) = s.upgrade() else { return };
                let empty = e.text().trim().is_empty();
                if let (Some(h), Some(hint), Some(c)) = (head_rev.upgrade(), hint_rev.upgrade(), column.upgrade()) {
                    h.set_reveal_child(empty);
                    hint.set_reveal_child(empty);
                    c.set_valign(if empty { gtk::Align::Center } else { gtk::Align::Start });
                }
                s.run(&e.text());
            });
        }
        {
            let s = Rc::downgrade(&search);
            let keys = gtk::EventControllerKey::new();
            keys.connect_key_pressed(move |_, key, _, state| {
                let enter = matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter);
                if enter
                    && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                    && let Some(s) = s.upgrade()
                {
                    s.ask_claude();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            entry.add_controller(keys);
        }
        let e = entry.downgrade();
        // Test hook (smoke test, screenshots): start with this query.
        let preset = std::cell::Cell::new(std::env::var("VELA_SETTINGS_SEARCH").ok());
        search.widget.connect_map(move |_| {
            if let Some(e) = e.upgrade() {
                e.grab_focus();
                if let Some(q) = preset.take() {
                    e.set_text(&q);
                }
            }
        });
        search
    }

    /// Pages to search; `fill` builds pages that create their rows lazily.
    pub fn add_page(&self, id: &str, title: &str, page: &gtk::Widget) {
        self.pages.borrow_mut().push((id.to_owned(), title.to_owned(), page.clone()));
    }

    pub fn before_index(&self, fill: Rc<dyn Fn()>) {
        self.before_index.borrow_mut().push(fill);
    }

    pub fn set_open_page(&self, f: OpenPage) {
        *self.open_page.borrow_mut() = Some(f);
    }

    pub fn focus(&self) {
        self.entry.grab_focus();
    }

    fn index(&self) {
        if self.items.borrow().is_some() {
            return;
        }
        for fill in self.before_index.borrow().iter() {
            fill();
        }
        let mut items = Vec::new();
        for (id, title, page) in self.pages.borrow().iter() {
            let mut found = Vec::new();
            walk(page, &mut Vec::new(), &mut found);
            for (row, context) in found {
                items.push(Item {
                    row: row.downgrade(),
                    page: id.clone(),
                    page_title: title.clone(),
                    context: context.into_iter().filter(|c| !c.is_empty()).collect(),
                });
            }
        }
        // A curated Hyprland row wins over the same option in "All options".
        let curated: std::collections::HashSet<String> = items
            .iter()
            .filter(|i| i.page != "hypr-all")
            .filter_map(|i| i.row.upgrade())
            .map(|r| r.widget_name().to_string())
            .filter(|n| n.contains(':'))
            .collect();
        items.retain(|i| i.page != "hypr-all" || i.row.upgrade().is_some_and(|r| !curated.contains(r.widget_name().as_str())));
        *self.items.borrow_mut() = Some(items);
    }

    /// Puts every borrowed row back where it came from.
    pub fn restore(&self) {
        let mut borrowed = std::mem::take(&mut *self.borrowed.borrow_mut());
        for b in &borrowed {
            if let Some(p) = b.row.parent().and_downcast::<gtk::ListBox>() {
                p.remove(&b.row);
            }
        }
        // Ascending per list: each index was taken with the earlier rows present.
        borrowed.sort_by_key(|b| b.index);
        for b in borrowed {
            b.parent.insert(&b.row, b.index);
            b.row.set_visible(b.visible);
        }
        while let Some(c) = self.results.first_child() {
            self.results.remove(&c);
        }
    }

    fn run(self: &Rc<Self>, text: &str) {
        self.restore();
        if text.trim().is_empty() {
            return;
        }
        let groups = search_terms::groups(text);
        self.index();
        let items = self.items.borrow();
        let mut hits: Vec<(u8, usize, adw::PreferencesRow)> = Vec::new();
        for (i, item) in items.iter().flatten().enumerate() {
            let Some(row) = item.row.upgrade() else { continue };
            let (title, subtitle) = row_text(&row);
            // Not the page title: "blur" would find every row on "Blur & effects".
            let hay = format!(
                "{title} {subtitle} {} {}",
                item.context.join(" "),
                row.widget_name().replace([':', '_', '.', '-'], " ")
            )
            .to_lowercase();
            if let Some(sc) = score(&groups, &title, &hay) {
                hits.push((sc, i, row));
            }
        }
        // An expander that matches brings its rows along.
        let expanders: Vec<adw::PreferencesRow> = hits.iter().filter(|(_, _, r)| r.is::<adw::ExpanderRow>()).map(|(_, _, r)| r.clone()).collect();
        hits.retain(|(_, _, r)| !expanders.iter().any(|e| e != r && r.is_ancestor(e)));
        hits.sort_by_key(|(sc, i, _)| (*sc, *i));
        hits.truncate(MAX_RESULTS);

        // A sentence reads like a request: Claude first.
        let wants_claude = text.split_whitespace().count() >= 3 || hits.is_empty();
        if wants_claude {
            self.results.append(&self.claude_card(text));
        }
        // Grouped by where they live; the group with the best hit first,
        // inside a group the page order.
        let group_of = |i: usize| {
            let it = &items.as_ref().unwrap()[i];
            format!("{}\u{1f}{}", it.page, it.context.join(" › "))
        };
        let mut best: std::collections::HashMap<String, (u8, usize)> = std::collections::HashMap::new();
        for (sc, i, _) in &hits {
            let e = best.entry(group_of(*i)).or_insert((*sc, *i));
            *e = (*e).min((*sc, *i));
        }
        hits.sort_by_key(|(_, i, _)| (best[&group_of(*i)], *i));
        // Positions before anything moves: removing a row shifts the next.
        let places: Vec<(i32, bool)> = hits.iter().map(|(_, _, r)| (r.index(), r.is_visible())).collect();
        let mut current: Option<(String, gtk::ListBox)> = None;
        for ((_, i, row), (index, visible)) in hits.into_iter().zip(places) {
            let item = &items.as_ref().unwrap()[i];
            let key = format!("{}\u{1f}{}", item.page, item.context.join(" › "));
            if current.as_ref().is_none_or(|(k, _)| *k != key) {
                let list = gtk::ListBox::builder()
                    .css_classes(["boxed-list"])
                    .selection_mode(gtk::SelectionMode::None)
                    .build();
                self.results.append(&self.heading(item));
                self.results.append(&list);
                current = Some((key, list));
            }
            let Some(parent) = row.parent().and_downcast::<gtk::ListBox>() else {
                continue;
            };
            parent.remove(&row);
            if let Some((_, list)) = &current {
                list.append(&row);
            }
            row.set_visible(true);
            self.borrowed.borrow_mut().push(Borrowed { row, parent, index, visible });
        }
        if !wants_claude {
            self.results.append(&self.claude_card(text));
        }
    }

    fn heading(self: &Rc<Self>, item: &Item) -> gtk::Widget {
        let mut path = vec![item.page_title.clone()];
        path.extend(item.context.iter().cloned());
        let label = gtk::Label::builder()
            .label(path.join(" › "))
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["heading"])
            .build();
        let open = gtk::Button::builder()
            .label("Open page")
            .css_classes(["flat", "caption"])
            .tooltip_text(format!("Go to {}", item.page_title))
            .build();
        let (s, page) = (Rc::downgrade(self), item.page.clone());
        open.connect_clicked(move |_| {
            if let Some(f) = s.upgrade().and_then(|s| s.open_page.borrow().clone()) {
                f(&page);
            }
        });
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        b.append(&label);
        b.append(&open);
        b.upcast()
    }

    fn claude_card(self: &Rc<Self>, text: &str) -> gtk::Widget {
        let row = adw::ActionRow::builder()
            .title("Ask Claude to do it")
            .subtitle(format!("“{}”", text.trim()))
            .activatable(true)
            .build();
        row.add_prefix(
            &gtk::Image::builder()
                .icon_name("vela-claude-symbolic")
                .pixel_size(24)
                .css_classes(["vela-claude-mark"])
                .build(),
        );
        row.add_suffix(&gtk::Label::builder().label("Ctrl+↵").css_classes(["dim-label", "caption", "numeric"]).build());
        let s = Rc::downgrade(self);
        row.connect_activated(move |_| {
            if let Some(s) = s.upgrade() {
                s.ask_claude();
            }
        });
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list", "vela-claude-card"])
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list.append(&row);
        list.upcast()
    }

    fn ask_claude(&self) {
        let text = self.entry.text().to_string();
        if text.trim().is_empty() {
            return;
        }
        match self.daemon.ask_claude(&claude_prompt(&text)) {
            Ok(()) => self.entry.set_text(""),
            Err(e) => {
                let toast = adw::Toast::new(&e);
                if let Some(overlay) = self.widget.ancestor(adw::ToastOverlay::static_type()).and_downcast::<adw::ToastOverlay>() {
                    overlay.add_toast(toast);
                } else {
                    log::warn!("asking Claude failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_rank_before_descriptions() {
        let g = search_terms::groups;
        assert_eq!(score(&g("blur"), "Blurriness", "blurriness radius of the blur"), Some(0));
        assert_eq!(score(&g("gaps windows"), "Gaps between windows", "gaps between windows"), Some(0));
        assert_eq!(score(&g("windows"), "Gaps between windows", "gaps between windows"), Some(1));
        assert_eq!(score(&g("radius"), "Blurriness", "blurriness radius of the blur"), Some(2));
        assert_eq!(score(&g("radius nope"), "Blurriness", "blurriness radius"), None);
        assert_eq!(score(&g(""), "x", "x"), None);
        // German: "Abstand" → gap, "Fenstern" → window.
        assert_eq!(
            score(&g("Abstand zwischen Fenstern"), "Gaps between windows", "gaps between windows space"),
            Some(0)
        );
    }

    #[test]
    fn claude_gets_the_request_and_the_tools() {
        let p = claude_prompt("  smaller gaps ");
        assert!(p.starts_with("Change my desktop settings: “smaller gaps”"));
        assert!(p.contains("set_hyprland_options"));
    }
}
