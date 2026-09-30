//! A selection background that glides between rows (sidebar of the settings,
//! launcher results) instead of each row switching its own background.

use adw::prelude::*;
use std::cell::{Cell, RefCell};

pub struct Marker {
    pill: gtk::Widget,
    /// The pill's parent; row bounds are taken in its coordinates, as padding
    /// or margins of the list would otherwise shift the marker.
    layer: gtk::Overlay,
    /// Also follow the row's x and width (rows inset within the list).
    horizontal: bool,
    anim: RefCell<Option<adw::TimedAnimation>>,
    from: Cell<[f64; 4]>,
}

impl Marker {
    /// Wraps `list` so a marker with CSS class `class` is drawn *below* its
    /// rows: the marker's layer is the overlay's main child and the list an
    /// overlay that decides the size. Returns the widget to put in place of
    /// `list`.
    pub fn below(list: &impl IsA<gtk::Widget>, class: &str, horizontal: bool) -> (gtk::Overlay, Marker) {
        let pill = gtk::Box::builder()
            .css_classes([class])
            .visible(false)
            .can_target(false)
            .valign(gtk::Align::Start)
            .build();
        if horizontal {
            pill.set_halign(gtk::Align::Start);
        }
        let under = gtk::Box::new(gtk::Orientation::Vertical, 0);
        under.append(&pill);
        let layer = gtk::Overlay::builder().child(&under).build();
        layer.add_overlay(list);
        layer.set_measure_overlay(list, true);
        let marker = Marker {
            pill: pill.upcast(),
            layer: layer.clone(),
            horizontal,
            anim: RefCell::default(),
            from: Cell::default(),
        };
        (layer, marker)
    }

    fn place(pill: &gtk::Widget, horizontal: bool, [x, y, w, h]: [f64; 4]) {
        pill.set_margin_top(y.round().max(0.0) as i32);
        pill.set_height_request(h.round().max(1.0) as i32);
        if horizontal {
            pill.set_margin_start(x.round().max(0.0) as i32);
            pill.set_width_request(w.round().max(1.0) as i32);
        }
    }

    pub fn hide(&self) {
        self.pill.set_visible(false);
        self.from.set([0.0; 4]);
    }

    /// Moves to `row`; returns false if the row has no size yet.
    pub fn move_to(&self, row: &impl IsA<gtk::Widget>, animate: bool) -> bool {
        let Some(b) = row.compute_bounds(&self.layer) else { return false };
        if b.height() <= 0.0 {
            return false;
        }
        let to = [f64::from(b.x()), f64::from(b.y()), f64::from(b.width()), f64::from(b.height())];
        if let Some(a) = self.anim.borrow_mut().take() {
            a.skip();
        }
        let from = self.from.replace(to);
        self.pill.set_visible(true);
        if !animate || from[3] == 0.0 {
            Marker::place(&self.pill, self.horizontal, to);
            return true;
        }
        let (pill, horizontal) = (self.pill.clone(), self.horizontal);
        let target = adw::CallbackAnimationTarget::new(move |t| {
            let mix = |i: usize| from[i] + (to[i] - from[i]) * t;
            Marker::place(&pill, horizontal, [mix(0), mix(1), mix(2), mix(3)]);
        });
        let a = adw::TimedAnimation::builder()
            .widget(&self.pill)
            .value_from(0.0)
            .value_to(1.0)
            .duration(260)
            .easing(adw::Easing::EaseOutBack)
            .target(&target)
            .build();
        a.play();
        *self.anim.borrow_mut() = Some(a);
        true
    }
}
