//! Discovery of installed applications from the XDG `applications`
//! directories, following the desktop-file ID and precedence rules.

use super::desktop_entry::{self, DesktopEntry};
use crate::paths;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct App {
    /// Desktop file ID, e.g. `org.gnome.Nautilus.desktop`.
    pub id: String,
    pub path: PathBuf,
    pub entry: DesktopEntry,
}

#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    pub apps: Vec<App>,
    pub malformed: Vec<(PathBuf, String)>,
}

pub struct ScanOptions {
    pub locales: Vec<String>,
    pub desktops: Vec<String>,
}

impl ScanOptions {
    pub fn from_env() -> Self {
        let desktops = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        ScanOptions {
            locales: desktop_entry::locale_candidates(&desktop_entry::current_locale()),
            desktops,
        }
    }
}

fn collect_desktop_files(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        // Follow symlinks: many packages symlink their desktop files.
        let is_dir = ft.is_dir() || (ft.is_symlink() && path.is_dir());
        if is_dir {
            collect_desktop_files(base, &path, out, depth + 1);
        } else if path.extension().is_some_and(|e| e == "desktop")
            && let Ok(rel) = path.strip_prefix(base)
        {
            let id = rel.to_string_lossy().replace('/', "-");
            out.push((id, path));
        }
    }
}

/// Scans `<dir>/applications` for every data dir (highest priority first).
pub fn scan(data_dirs: &[PathBuf], opts: &ScanOptions) -> ScanReport {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut report = ScanReport::default();

    for data_dir in data_dirs {
        let base = data_dir.join("applications");
        let mut files = Vec::new();
        collect_desktop_files(&base, &base, &mut files, 0);
        for (id, path) in files {
            // The first file with a given ID shadows all later ones, even if
            // it turns out to be hidden or broken.
            if seen.insert(id.clone(), ()).is_some() {
                continue;
            }
            let content = match std::fs::read(&path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(err) => {
                    report.malformed.push((path, err.to_string()));
                    continue;
                }
            };
            let entry = match desktop_entry::parse(&content, &opts.locales) {
                Ok(e) => e,
                Err(desktop_entry::ParseError::NotAnApplication(_)) => continue,
                Err(err) => {
                    report.malformed.push((path, err.to_string()));
                    continue;
                }
            };
            if entry.hidden || entry.no_display || !entry.shown_in(&opts.desktops) {
                continue;
            }
            if entry.exec.is_none() && !entry.dbus_activatable {
                report.malformed.push((path, "no Exec key".into()));
                continue;
            }
            if let Some(try_exec) = &entry.try_exec
                && paths::find_executable(try_exec).is_none()
            {
                continue;
            }
            report.apps.push(App { id, path, entry });
        }
    }

    report.apps.sort_by_cached_key(|a| a.entry.name.to_lowercase());
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join("applications").join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn app(name: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\n", name.to_lowercase())
    }

    fn opts() -> ScanOptions {
        ScanOptions {
            locales: vec![],
            desktops: vec!["Hyprland".into()],
        }
    }

    #[test]
    fn scans_with_precedence_and_ids() {
        let user = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        write(system.path(), "firefox.desktop", &app("Firefox"));
        write(user.path(), "firefox.desktop", &app("My Firefox"));
        write(system.path(), "kde/dolphin.desktop", &app("Dolphin"));
        write(system.path(), "notes.txt", "irrelevant");

        let report = scan(&[user.path().into(), system.path().into()], &opts());
        let names: Vec<_> = report.apps.iter().map(|a| (a.id.as_str(), a.entry.name.as_str())).collect();
        assert_eq!(names, vec![("kde-dolphin.desktop", "Dolphin"), ("firefox.desktop", "My Firefox")]);
    }

    #[test]
    fn hidden_user_entry_removes_system_entry() {
        let user = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        write(system.path(), "ads.desktop", &app("Ads"));
        write(user.path(), "ads.desktop", "[Desktop Entry]\nType=Application\nName=Ads\nHidden=true\n");
        let report = scan(&[user.path().into(), system.path().into()], &opts());
        assert!(report.apps.is_empty());
    }

    #[test]
    fn filters_nodisplay_showin_tryexec_and_reports_malformed() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.desktop", "[Desktop Entry]\nType=Application\nName=A\nExec=a\nNoDisplay=true\n");
        write(d.path(), "b.desktop", "[Desktop Entry]\nType=Application\nName=B\nExec=b\nOnlyShowIn=KDE;\n");
        write(
            d.path(),
            "c.desktop",
            "[Desktop Entry]\nType=Application\nName=C\nExec=c\nTryExec=no-such-binary-vela\n",
        );
        write(d.path(), "d.desktop", "[Desktop Entry]\nType=Application\nName=D\nExec=sh\nTryExec=sh\n");
        write(d.path(), "broken.desktop", "this is not a desktop file");
        write(d.path(), "noexec.desktop", "[Desktop Entry]\nType=Application\nName=E\n");
        write(d.path(), "binary.desktop", "\u{feff}[Desktop Entry]\nType=Application\nName=F\nExec=f\n");

        let report = scan(&[d.path().into()], &opts());
        let names: Vec<_> = report.apps.iter().map(|a| a.entry.name.as_str()).collect();
        assert_eq!(names, vec!["D", "F"]);
        assert_eq!(report.malformed.len(), 2);
    }

    #[test]
    fn missing_dirs_are_fine() {
        let report = scan(&[PathBuf::from("/nonexistent/vela")], &opts());
        assert!(report.apps.is_empty() && report.malformed.is_empty());
    }
}
