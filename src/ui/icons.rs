//! Icon lookup with fallbacks; a missing or broken icon never breaks a row.

use crate::paths;
use gtk::gio;
use gtk::prelude::*;
use std::path::Path;

const APP_FALLBACK: &str = "application-x-executable";

const BUNDLED: &[(&str, &str)] = &[
    ("vela.svg", include_str!("../../data/icons/vela.svg")),
    ("vela-claude-symbolic.svg", include_str!("../../data/icons/vela-claude-symbolic.svg")),
    ("vela-drag-handle-symbolic.svg", include_str!("../../data/icons/vela-drag-handle-symbolic.svg")),
    ("vela-windows-symbolic.svg", include_str!("../../data/icons/vela-windows-symbolic.svg")),
    ("vela-blur-symbolic.svg", include_str!("../../data/icons/vela-blur-symbolic.svg")),
    ("vela-animations-symbolic.svg", include_str!("../../data/icons/vela-animations-symbolic.svg")),
    ("vela-layouts-symbolic.svg", include_str!("../../data/icons/vela-layouts-symbolic.svg")),
    ("vela-behaviour-symbolic.svg", include_str!("../../data/icons/vela-behaviour-symbolic.svg")),
    ("vela-monitors-symbolic.svg", include_str!("../../data/icons/vela-monitors-symbolic.svg")),
    ("vela-shortcuts-symbolic.svg", include_str!("../../data/icons/vela-shortcuts-symbolic.svg")),
    ("vela-rules-symbolic.svg", include_str!("../../data/icons/vela-rules-symbolic.svg")),
    ("vela-autostart-symbolic.svg", include_str!("../../data/icons/vela-autostart-symbolic.svg")),
    ("vela-all-options-symbolic.svg", include_str!("../../data/icons/vela-all-options-symbolic.svg")),
    ("vela-updates-symbolic.svg", include_str!("../../data/icons/vela-updates-symbolic.svg")),
];

/// Makes the bundled icons available by name even when vela runs from the
/// build directory without being installed.
pub fn install_bundled() {
    let base = paths::cache_dir().join("icons");
    let dir = base.join("hicolor/scalable/apps");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, svg) in BUNDLED {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = std::fs::write(&path, svg);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&base);
    }
}

fn theme() -> Option<gtk::IconTheme> {
    gtk::gdk::Display::default().map(|d| gtk::IconTheme::for_display(&d))
}

fn has_icon(name: &str) -> bool {
    theme().is_some_and(|t| t.has_icon(name))
}

/// Resolves a desktop-entry `Icon` value (name or absolute path).
pub fn app_icon(icon: Option<&str>) -> gio::Icon {
    let fallback = || gio::ThemedIcon::new(APP_FALLBACK).upcast::<gio::Icon>();
    let Some(icon) = icon.map(str::trim).filter(|i| !i.is_empty()) else {
        return fallback();
    };
    if icon.starts_with('/') || icon.starts_with('~') {
        let p = paths::expand_tilde(icon);
        return if p.is_file() {
            gio::FileIcon::new(&gio::File::for_path(p)).upcast()
        } else {
            fallback()
        };
    }
    if has_icon(icon) {
        return gio::ThemedIcon::new(icon).upcast();
    }
    // Some entries wrongly include an extension: "foo.png".
    if let Some(stem) = Path::new(icon).file_stem().and_then(|s| s.to_str())
        && stem != icon
        && has_icon(stem)
    {
        return gio::ThemedIcon::new(stem).upcast();
    }
    // Last resort: /usr/share/pixmaps.
    for ext in ["png", "svg", "xpm"] {
        let p = Path::new("/usr/share/pixmaps").join(format!("{icon}.{ext}"));
        if p.is_file() {
            return gio::FileIcon::new(&gio::File::for_path(p)).upcast();
        }
    }
    fallback()
}

pub fn app_image(icon: Option<&str>, size: i32) -> gtk::Image {
    let img = gtk::Image::from_gicon(&app_icon(icon));
    img.set_pixel_size(size);
    img
}

pub fn file_image(path: &Path, is_dir: bool, size: i32) -> gtk::Image {
    let icon: gio::Icon = if is_dir {
        let home = paths::home_dir();
        let special = [
            ("Desktop", "user-desktop"),
            ("Documents", "folder-documents"),
            ("Downloads", "folder-download"),
            ("Music", "folder-music"),
            ("Pictures", "folder-pictures"),
            ("Videos", "folder-videos"),
        ]
        .iter()
        .find(|(n, _)| path == home.join(n))
        .map(|(_, i)| *i)
        .filter(|i| has_icon(i));
        gio::ThemedIcon::with_default_fallbacks(special.unwrap_or("folder")).upcast()
    } else {
        let (ctype, _) = gio::content_type_guess(Some(path), None);
        gio::content_type_get_icon(&ctype)
    };
    let img = gtk::Image::from_gicon(&icon);
    img.set_pixel_size(size);
    img
}

pub fn named_image(name: &str, size: i32) -> gtk::Image {
    let img = gtk::Image::from_icon_name(if has_icon(name) { name } else { APP_FALLBACK });
    img.set_pixel_size(size);
    img
}
