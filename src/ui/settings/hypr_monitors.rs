//! Monitors page: drag the arrangement, pick mode, scale, rotation and VRR.
//! Every change that can leave a screen dark asks to be kept and reverts on
//! its own after a few seconds.

use super::binder::group;
use super::hypr_rows::Rows;
use crate::hyprmon::{self, MonitorInfo, MonitorRule, Rect};
use crate::ui::hypr_store::HyprStore;
use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const KEEP_SECONDS: u32 = 15;

/// The rule vela would write for a monitor now: its override if any, else
/// what it does at the moment.
fn rule_for(store: &HyprStore, m: &MonitorInfo) -> MonitorRule {
    let mut r = store.monitor_overrides().get(&m.output()).cloned().unwrap_or_else(|| m.current_rule());
    // Follow what Hyprland made of it (positions shift when others change).
    if !m.disabled {
        r.position = (m.x, m.y);
    }
    r
}

/// Every monitor's rule as it is now, for undoing a change.
fn snapshot(store: &HyprStore) -> (std::collections::BTreeMap<String, MonitorRule>, Vec<MonitorRule>) {
    (store.monitor_overrides(), store.monitors().iter().map(|m| m.current_rule()).collect())
}

/// Applies rules, then asks whether to keep them.
fn apply_and_confirm(anchor: &gtk::Widget, store: &HyprStore, rules: Vec<MonitorRule>) {
    let (before, undo) = snapshot(store);
    store.set_monitors(rules);
    let left = Rc::new(Cell::new(KEEP_SECONDS));
    let body = |s: u32| format!("The previous settings come back in {s} s unless you keep these.");
    let dialog = adw::AlertDialog::builder()
        .heading("Keep these display settings?")
        .body(body(KEEP_SECONDS))
        .build();
    dialog.add_responses(&[("revert", "Revert"), ("keep", "Keep")]);
    dialog.set_response_appearance("keep", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("revert");
    let done = Rc::new(Cell::new(false));
    let revert = {
        let (store, done) = (store.clone(), done.clone());
        let state = RefCell::new(Some((before, undo)));
        Rc::new(move || {
            if done.replace(true) {
                return;
            }
            if let Some((before, undo)) = state.borrow_mut().take() {
                store.restore_monitors(before, undo);
            }
        })
    };
    {
        let (revert, done) = (revert.clone(), done.clone());
        dialog.connect_response(None, move |_, resp| {
            if resp == "keep" {
                done.set(true);
            } else {
                revert();
            }
        });
    }
    let weak = dialog.downgrade();
    glib::timeout_add_seconds_local(1, move || {
        let Some(d) = weak.upgrade() else { return glib::ControlFlow::Break };
        if done.get() {
            return glib::ControlFlow::Break;
        }
        let s = left.get().saturating_sub(1);
        left.set(s);
        if s == 0 {
            revert();
            d.close();
            return glib::ControlFlow::Break;
        }
        d.set_body(&body(s));
        glib::ControlFlow::Continue
    });
    dialog.present(Some(anchor));
}

/// Keeps a row of monitors touching after one changed size: everything that
/// started at its old right (bottom) edge moves by the difference.
fn reflow(monitors: &[MonitorInfo], changed: &MonitorInfo, new_size: (i32, i32), rules: &mut Vec<MonitorRule>, store: &HyprStore) {
    let (ow, oh) = changed.logical_size();
    let (dw, dh) = (new_size.0 - ow, new_size.1 - oh);
    for m in monitors.iter().filter(|m| !m.disabled && m.name != changed.name) {
        let mut r = rule_for(store, m);
        let mut moved = false;
        if dw != 0 && m.x >= changed.x + ow {
            r.position.0 += dw;
            moved = true;
        }
        if dh != 0 && m.y >= changed.y + oh && m.x < changed.x + ow && changed.x < m.x + m.logical_size().0 {
            r.position.1 += dh;
            moved = true;
        }
        if moved {
            rules.push(r);
        }
    }
}

const ROTATIONS: [&str; 8] = ["Normal", "90°", "180°", "270°", "Flipped", "Flipped 90°", "Flipped 180°", "Flipped 270°"];
const VRR: [(Option<i64>, &str); 5] = [
    (None, "Like the global setting"),
    (Some(0), "Off"),
    (Some(1), "On"),
    (Some(2), "Fullscreen only"),
    (Some(3), "Fullscreen games only"),
];

fn arrangement(store: &HyprStore) -> gtk::Widget {
    let fixed = gtk::Fixed::new();
    let frame = gtk::Box::builder().css_classes(["vela-monitor-canvas"]).halign(gtk::Align::Fill).build();
    frame.append(&fixed);
    let hint = gtk::Label::builder()
        .label("Drag the monitors to arrange them.")
        .css_classes(["dim-label", "caption"])
        .margin_top(6)
        .xalign(0.0)
        .build();
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.append(&frame);
    outer.append(&hint);

    let build = {
        let (store, fixed) = (store.clone(), fixed.downgrade());
        Rc::new(move || {
            let Some(fixed) = fixed.upgrade() else { return };
            while let Some(c) = fixed.first_child() {
                fixed.remove(&c);
            }
            let monitors: Vec<MonitorInfo> = store.monitors().into_iter().filter(|m| !m.disabled).collect();
            if monitors.is_empty() {
                return;
            }
            let rects: Vec<Rect> = monitors
                .iter()
                .map(|m| {
                    let (w, h) = m.logical_size();
                    Rect { x: m.x, y: m.y, w, h }
                })
                .collect();
            let (min_x, min_y) = (rects.iter().map(|r| r.x).min().unwrap_or(0), rects.iter().map(|r| r.y).min().unwrap_or(0));
            let (max_x, max_y) = (
                rects.iter().map(|r| r.x + r.w).max().unwrap_or(1),
                rects.iter().map(|r| r.y + r.h).max().unwrap_or(1),
            );
            // Room to drag a monitor next to the others on every side.
            let span_w = f64::from(max_x - min_x) * 1.6;
            let span_h = f64::from(max_y - min_y) * 1.6;
            let factor = (600.0 / span_w).min(240.0 / span_h);
            let off_x = (f64::from(max_x - min_x) * 0.3) as i32 - min_x;
            let off_y = (f64::from(max_y - min_y) * 0.3) as i32 - min_y;
            fixed.set_size_request((span_w * factor) as i32, (span_h * factor) as i32);
            let to_px = move |x: i32, y: i32| (f64::from(x + off_x) * factor, f64::from(y + off_y) * factor);
            let rects = Rc::new(RefCell::new(rects));
            for (i, m) in monitors.iter().enumerate() {
                let r = rects.borrow()[i];
                let tile = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .valign(gtk::Align::Center)
                    .css_classes(["vela-monitor-tile"])
                    .tooltip_text(m.title())
                    .build();
                tile.set_size_request((f64::from(r.w) * factor) as i32 - 4, (f64::from(r.h) * factor) as i32 - 4);
                let inner = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .vexpand(true)
                    .valign(gtk::Align::Center)
                    .build();
                inner.append(&gtk::Label::builder().label(&m.name).css_classes(["title"]).build());
                inner.append(
                    &gtk::Label::builder()
                        .label(format!("{}×{}", m.mode.width, m.mode.height))
                        .css_classes(["caption"])
                        .build(),
                );
                tile.append(&inner);
                tile.set_cursor_from_name(Some("grab"));
                let (px, py) = to_px(r.x, r.y);
                fixed.put(&tile, px, py);

                let drag = gtk::GestureDrag::new();
                let start = Rc::new(Cell::new((0, 0)));
                {
                    let (rects, start, tile) = (rects.clone(), start.clone(), tile.downgrade());
                    drag.connect_drag_begin(move |_, _, _| {
                        let r = rects.borrow()[i];
                        start.set((r.x, r.y));
                        if let Some(t) = tile.upgrade() {
                            t.add_css_class("dragging");
                            t.set_cursor_from_name(Some("grabbing"));
                        }
                    });
                }
                {
                    let (rects, start, tile, fixed) = (rects.clone(), start.clone(), tile.downgrade(), fixed.downgrade());
                    drag.connect_drag_update(move |_, dx, dy| {
                        let (sx, sy) = start.get();
                        let (x, y) = (sx + (dx / factor) as i32, sy + (dy / factor) as i32);
                        let threshold = (24.0 / factor) as i32;
                        let (x, y) = hyprmon::snap(&rects.borrow(), i, x, y, threshold);
                        rects.borrow_mut()[i].x = x;
                        rects.borrow_mut()[i].y = y;
                        if let (Some(t), Some(f)) = (tile.upgrade(), fixed.upgrade()) {
                            let (px, py) = to_px(x, y);
                            f.move_(&t, px, py);
                        }
                    });
                }
                {
                    let (rects, store, monitors, tile) = (rects.clone(), store.clone(), monitors.clone(), tile.downgrade());
                    drag.connect_drag_end(move |_, _, _| {
                        let Some(t) = tile.upgrade() else { return };
                        t.remove_css_class("dragging");
                        t.set_cursor_from_name(Some("grab"));
                        let mut placed = rects.borrow().clone();
                        hyprmon::normalize(&mut placed);
                        let rules: Vec<MonitorRule> = monitors
                            .iter()
                            .zip(&placed)
                            .filter(|(m, r)| (m.x, m.y) != (r.x, r.y))
                            .map(|(m, r)| MonitorRule {
                                position: (r.x, r.y),
                                ..rule_for(&store, m)
                            })
                            .collect();
                        if !rules.is_empty() {
                            apply_and_confirm(t.upcast_ref(), &store, rules);
                        }
                    });
                }
                tile.add_controller(drag);
            }
        })
    };
    build();
    store.subscribe(move |n| {
        if n.is_none_or(|n| n.starts_with("monitor:")) {
            build();
        }
    });
    outer.upcast()
}

fn monitor_group(r: &Rows, m: &MonitorInfo) -> adw::PreferencesGroup {
    let store = r.store.clone();
    let g = group(&m.title(), "");
    let reset = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .tooltip_text("Back to the rules from hyprland.lua")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .visible(store.is_overridden(&format!("monitor:{}", m.output())))
        .build();
    {
        let (store, out) = (store.clone(), m.output());
        reset.connect_clicked(move |_| store.reset(&[format!("monitor:{out}")]));
    }
    g.set_header_suffix(Some(&reset));
    let name = m.name.clone();
    let current = {
        let store = store.clone();
        move || store.monitors().into_iter().find(|x| x.name == name)
    };

    let enabled = adw::SwitchRow::builder().title("Use this monitor").active(!m.disabled).build();
    let others_on = store.monitors().iter().filter(|x| !x.disabled && x.name != m.name).count();
    if others_on == 0 && !m.disabled {
        enabled.set_sensitive(false);
        enabled.set_subtitle("The only screen that is on");
    }
    {
        let (store, current) = (store.clone(), current.clone());
        enabled.connect_active_notify(move |row| {
            let Some(m) = current() else { return };
            if row.is_active() == !m.disabled {
                return;
            }
            let mut rule = rule_for(&store, &m);
            rule.disabled = !row.is_active();
            if row.is_active() {
                if m.mode.width == 0 || rule.mode.is_empty() {
                    rule.mode = "preferred".into();
                }
                // Next to the others on the right.
                let right = store
                    .monitors()
                    .iter()
                    .filter(|x| !x.disabled)
                    .map(|x| x.x + x.logical_size().0)
                    .max()
                    .unwrap_or(0);
                rule.position = (right, 0);
            }
            apply_and_confirm(row.upcast_ref(), &store, vec![rule]);
        });
    }
    g.add(&enabled);
    if m.disabled {
        return g;
    }

    // Resolution and refresh rate.
    let mut sizes: Vec<(i32, i32)> = m.modes.iter().map(|x| (x.width, x.height)).collect();
    sizes.sort_by_key(|(w, h)| std::cmp::Reverse(w * h));
    sizes.dedup();
    if sizes.is_empty() {
        sizes.push((m.mode.width, m.mode.height));
    }
    let res = adw::ComboRow::builder()
        .title("Resolution")
        .model(&gtk::StringList::new(
            &sizes
                .iter()
                .enumerate()
                .map(|(i, (w, h))| if i == 0 { format!("{w} × {h} (best)") } else { format!("{w} × {h}") })
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ))
        .selected(sizes.iter().position(|s| *s == (m.mode.width, m.mode.height)).unwrap_or(0) as u32)
        .build();
    let rates_for = {
        let modes = m.modes.clone();
        move |(w, h): (i32, i32)| {
            let mut r: Vec<f64> = modes.iter().filter(|x| (x.width, x.height) == (w, h)).map(|x| x.refresh).collect();
            r.sort_by(|a, b| b.total_cmp(a));
            r.dedup_by(|a, b| (*a - *b).abs() < 0.005);
            r
        }
    };
    let rates = rates_for((m.mode.width, m.mode.height));
    let rate = adw::ComboRow::builder()
        .title("Refresh rate")
        .model(&gtk::StringList::new(
            &rates
                .iter()
                .map(|r| format!("{r:.2} Hz"))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ))
        .selected(rates.iter().position(|r| (r - m.mode.refresh).abs() < 0.05).unwrap_or(0) as u32)
        .build();
    if rates.len() < 2 {
        rate.set_sensitive(false);
    }
    let apply_mode = {
        let (store, current, sizes, res, rate) = (store.clone(), current.clone(), sizes.clone(), res.downgrade(), rate.downgrade());
        let rates_for = rates_for.clone();
        move |widget: &gtk::Widget| {
            let (Some(m), Some(res), Some(rate)) = (current(), res.upgrade(), rate.upgrade()) else {
                return;
            };
            let size = sizes.get(res.selected() as usize).copied().unwrap_or((m.mode.width, m.mode.height));
            let rates = rates_for(size);
            let refresh = rates.get(rate.selected() as usize).or(rates.first()).copied().unwrap_or(m.mode.refresh);
            let mode = hyprmon::Mode {
                width: size.0,
                height: size.1,
                refresh,
            };
            if mode.spec() == m.mode.spec() {
                return;
            }
            let mut rule = rule_for(&store, &m);
            rule.mode = mode.spec();
            let mut rules = Vec::new();
            reflow(
                &store.monitors(),
                &m,
                hyprmon::logical_size(size.0, size.1, rule.scale, rule.transform),
                &mut rules,
                &store,
            );
            rules.insert(0, rule);
            apply_and_confirm(widget, &store, rules);
        }
    };
    {
        let (apply, rate, rates_for, sizes) = (apply_mode.clone(), rate.downgrade(), rates_for.clone(), sizes.clone());
        res.connect_selected_notify(move |row| {
            // New resolution: offer its rates, fastest first.
            if let (Some(rate), Some(size)) = (rate.upgrade(), sizes.get(row.selected() as usize)) {
                let rates = rates_for(*size);
                let model = gtk::StringList::new(
                    &rates
                        .iter()
                        .map(|r| format!("{r:.2} Hz"))
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                );
                rate.set_model(Some(&model));
                rate.set_selected(0);
                rate.set_sensitive(rates.len() > 1);
            }
            apply(row.upcast_ref());
        });
    }
    rate.connect_selected_notify(move |row| apply_mode(row.upcast_ref()));
    g.add(&res);
    g.add(&rate);

    let scales = hyprmon::valid_scales(m.mode.width, m.mode.height, m.scale);
    let scale = adw::ComboRow::builder()
        .title("Scale")
        .subtitle("Size of everything on this screen")
        .model(&gtk::StringList::new(
            &scales
                .iter()
                .map(|s| format!("{:.0} %", s * 100.0))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ))
        .selected(scales.iter().position(|s| (s - m.scale).abs() < 0.001).unwrap_or(0) as u32)
        .build();
    {
        let (store, current) = (store.clone(), current.clone());
        scale.connect_selected_notify(move |row| {
            let (Some(m), Some(s)) = (current(), scales.get(row.selected() as usize).copied()) else {
                return;
            };
            if (s - m.scale).abs() < 0.001 {
                return;
            }
            let mut rule = rule_for(&store, &m);
            rule.scale = s;
            let mut rules = Vec::new();
            reflow(
                &store.monitors(),
                &m,
                hyprmon::logical_size(m.mode.width, m.mode.height, s, rule.transform),
                &mut rules,
                &store,
            );
            rules.insert(0, rule);
            apply_and_confirm(row.upcast_ref(), &store, rules);
        });
    }
    g.add(&scale);

    let rot = adw::ComboRow::builder()
        .title("Rotation")
        .model(&gtk::StringList::new(&ROTATIONS))
        .selected(m.transform.clamp(0, 7) as u32)
        .build();
    {
        let (store, current) = (store.clone(), current.clone());
        rot.connect_selected_notify(move |row| {
            let Some(m) = current() else { return };
            let t = i64::from(row.selected());
            if t == m.transform {
                return;
            }
            let mut rule = rule_for(&store, &m);
            rule.transform = t;
            let mut rules = Vec::new();
            reflow(
                &store.monitors(),
                &m,
                hyprmon::logical_size(m.mode.width, m.mode.height, rule.scale, t),
                &mut rules,
                &store,
            );
            rules.insert(0, rule);
            apply_and_confirm(row.upcast_ref(), &store, rules);
        });
    }
    g.add(&rot);

    let current_vrr = store.monitor_overrides().get(&m.output()).and_then(|r| r.vrr);
    let vrr = adw::ComboRow::builder()
        .title("Variable refresh rate")
        .model(&gtk::StringList::new(&VRR.iter().map(|(_, l)| *l).collect::<Vec<_>>()))
        .selected(VRR.iter().position(|(v, _)| *v == current_vrr).unwrap_or(0) as u32)
        .build();
    vrr.connect_selected_notify(move |row| {
        let Some(m) = current() else { return };
        let Some((v, _)) = VRR.get(row.selected() as usize) else { return };
        let mut rule = rule_for(&store, &m);
        if rule.vrr == *v {
            return;
        }
        rule.vrr = *v;
        // Harmless: no need to ask.
        store.set_monitors(vec![rule]);
    });
    g.add(&vrr);
    g
}

pub fn monitors(r: &Rows) -> adw::PreferencesPage {
    let p = adw::PreferencesPage::new();
    if !r.store.available() {
        p.set_description("Hyprland is not reachable — these settings need a running Hyprland session.");
        p.set_sensitive(false);
        return p;
    }
    p.set_description("Changes apply at once; you get a few seconds to keep or undo them.");
    let layout = group("Arrangement", "");
    layout.add(&arrangement(&r.store));
    p.add(&layout);

    // Per-monitor groups, rebuilt when monitors change.
    let groups: Rc<RefCell<Vec<adw::PreferencesGroup>>> = Rc::default();
    let rebuild = {
        let (r, p, groups) = (r.clone(), p.downgrade(), groups.clone());
        Rc::new(move || {
            let Some(p) = p.upgrade() else { return };
            for g in groups.borrow_mut().drain(..) {
                p.remove(&g);
            }
            let mut monitors = r.store.monitors();
            monitors.sort_by_key(|m| (m.disabled, m.x, m.y));
            for m in &monitors {
                let g = monitor_group(&r, m);
                p.add(&g);
                groups.borrow_mut().push(g);
            }
            let saved: Vec<String> = r
                .store
                .monitor_overrides()
                .keys()
                .filter(|out| !monitors.iter().any(|m| &m.output() == *out))
                .cloned()
                .collect();
            if !saved.is_empty() {
                let g = group("Not connected", "Settings vela keeps for monitors that aren't plugged in.");
                for out in saved {
                    let row = adw::ActionRow::builder().title(out.trim_start_matches("desc:")).build();
                    let forget = gtk::Button::builder().label("Forget").valign(gtk::Align::Center).build();
                    let store = r.store.clone();
                    forget.connect_clicked(move |_| store.reset(&[format!("monitor:{out}")]));
                    row.add_suffix(&forget);
                    g.add(&row);
                }
                p.add(&g);
                groups.borrow_mut().push(g);
            }
        })
    };
    rebuild();
    {
        // Rebuilding under the pointer of an open combo popover would close
        // it mid-use; wait for the change to settle.
        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        r.store.subscribe(move |n| {
            if !n.is_none_or(|n| n.starts_with("monitor:")) {
                return;
            }
            if let Some(id) = pending.borrow_mut().take() {
                id.remove();
            }
            let (rebuild, p2) = (rebuild.clone(), pending.clone());
            *pending.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(150), move || {
                p2.borrow_mut().take();
                rebuild();
            }));
        });
    }
    let store = r.store.clone();
    p.connect_map(move |_| store.refresh_monitors());
    p
}
