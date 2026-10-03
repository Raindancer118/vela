//! systemd services (user and system) for the Services page.

use super::engine::run_text;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub unit: String,
    pub description: String,
    /// active, inactive, failed, activating, deactivating
    pub active: String,
    /// running, exited, dead, failed, …
    pub sub: String,
    /// enabled, disabled, static, masked, … (empty for transient units)
    pub enabled: String,
    pub user: bool,
}

#[derive(Deserialize)]
struct FileRow {
    unit_file: String,
    state: String,
}

pub fn merge(units_json: &str, files_json: &str, user: bool) -> Vec<Service> {
    let units = super::health::parse_units(units_json);
    let files: HashMap<String, String> = serde_json::from_str::<Vec<FileRow>>(files_json.trim())
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.unit_file, f.state))
        .collect();
    let mut out: Vec<Service> = units
        .into_iter()
        .filter(|u| u.unit.ends_with(".service") && u.load != "not-found")
        .map(|u| Service {
            enabled: files.get(&u.unit).cloned().unwrap_or_default(),
            unit: u.unit,
            description: u.description,
            active: u.active,
            sub: u.sub,
            user,
        })
        .collect();
    // Installed but never loaded (disabled) services are worth listing too.
    for (unit, state) in &files {
        if !unit.contains('@') && !out.iter().any(|s| &s.unit == unit) && state != "masked" && state != "alias" {
            out.push(Service {
                unit: unit.clone(),
                active: "inactive".into(),
                sub: "dead".into(),
                enabled: state.clone(),
                user,
                ..Default::default()
            });
        }
    }
    out.sort_by(|a, b| a.unit.cmp(&b.unit));
    out
}

pub fn list() -> Vec<Service> {
    let mut all = Vec::new();
    for user in [true, false] {
        let scope: &[&str] = if user { &["--user"] } else { &[] };
        let args = |rest: &[&'static str]| -> Vec<&str> { scope.iter().copied().chain(rest.iter().copied()).collect() };
        let units = run_text("systemctl", &args(&["list-units", "--type=service", "--all", "--output=json", "--no-pager"])).unwrap_or_default();
        let files = run_text("systemctl", &args(&["list-unit-files", "--type=service", "--output=json", "--no-pager"])).unwrap_or_default();
        all.extend(merge(&units, &files, user));
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_units_and_files() {
        let units = r#"[{"unit":"a.service","load":"loaded","active":"active","sub":"running","description":"A"},
                        {"unit":"gone.service","load":"not-found","active":"inactive","sub":"dead","description":""},
                        {"unit":"x.socket","load":"loaded","active":"active","sub":"listening","description":""}]"#;
        let files = r#"[{"unit_file":"a.service","state":"enabled"},{"unit_file":"b.service","state":"disabled"},
                        {"unit_file":"t@.service","state":"static"},{"unit_file":"m.service","state":"masked"}]"#;
        let s = merge(units, files, true);
        assert_eq!(s.iter().map(|s| s.unit.as_str()).collect::<Vec<_>>(), vec!["a.service", "b.service"]);
        assert_eq!((s[0].enabled.as_str(), s[0].active.as_str(), s[0].user), ("enabled", "active", true));
        assert_eq!((s[1].active.as_str(), s[1].enabled.as_str()), ("inactive", "disabled"));
    }
}
