//! Claude plan usage (5-hour session and 7-day limit) for every Claude Code
//! profile on this machine, shown at the bottom of the control center.
//!
//! Unofficial: uses the endpoint behind Claude Code's `/usage`, which may
//! change at any time. Read-only. Each profile's OAuth token is read from its
//! `.credentials.json`, handed to curl on stdin (never argv or environment)
//! and never printed. Expired tokens are left to Claude Code to refresh.

use crate::config::Panel;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// 0..1 (can exceed 1 when over the limit).
    pub utilization: f64,
    /// ms since epoch, 0 if unknown.
    pub resets_at: i64,
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-10-01T12:30:00[.frac](Z|±hh:mm)` → ms since epoch (fraction dropped).
pub fn iso_to_ms(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    if s.len() < 20 || s.as_bytes()[10] != b'T' {
        return None;
    }
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &s[19..];
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match rest {
        "Z" => 0,
        _ => {
            let sign = match rest.chars().next()? {
                '+' => 1,
                '-' => -1,
                _ => return None,
            };
            let oh: i64 = rest.get(1..3)?.parse().ok()?;
            let om: i64 = rest.get(4..6)?.parse().ok()?;
            sign * (oh * 60 + om) * 60
        }
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se - offset;
    Some(secs * 1000)
}

fn window(v: &Value) -> Option<Window> {
    let utilization = v.get("utilization")?.as_f64()? / 100.0;
    let resets_at = v.get("resets_at").and_then(Value::as_str).and_then(iso_to_ms).unwrap_or(0);
    Some(Window {
        utilization: utilization.max(0.0),
        resets_at,
    })
}

/// (5-hour, 7-day) windows of a usage response; None if it has neither.
pub fn parse_usage(body: &str) -> Option<(Option<Window>, Option<Window>)> {
    let v: Value = serde_json::from_str(body).ok()?;
    let (s, w) = (v.get("five_hour").and_then(window), v.get("seven_day").and_then(window));
    (s.is_some() || w.is_some()).then_some((s, w))
}

/// Only characters that can't break out of the quoted curl config value.
pub fn token_is_safe(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || "._~+/=-".contains(c))
}

/// Access token from a `.credentials.json`, if present and not expired.
pub fn read_token(credentials: &str, now_ms: i64) -> Result<String, &'static str> {
    let v: Value = serde_json::from_str(credentials).map_err(|_| "bad-credentials")?;
    let oauth = v.get("claudeAiOauth").ok_or("no-credentials")?;
    let expires = oauth.get("expiresAt").and_then(Value::as_f64).unwrap_or(0.0) as i64;
    let token = oauth.get("accessToken").and_then(Value::as_str).unwrap_or("");
    if expires <= now_ms {
        return Err("expired");
    }
    if !token_is_safe(token) {
        return Err("bad-credentials");
    }
    Ok(token.to_owned())
}

/// Display name of the account's plan from `.credentials.json`.
pub fn plan_of(credentials: &str) -> Option<String> {
    let v: Value = serde_json::from_str(credentials).ok()?;
    let oauth = v.get("claudeAiOauth")?;
    let kind = oauth.get("subscriptionType").and_then(Value::as_str).filter(|s| !s.is_empty())?;
    let tier = oauth.get("rateLimitTier").and_then(Value::as_str).unwrap_or("");
    // "…_max_20x" → "Max 20×"; any Max-like tier without a number → "Max".
    let max = || match tier.rsplit('_').next().and_then(|t| t.strip_suffix('x')) {
        Some(n) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => format!("Max {n}×"),
        _ => "Max".into(),
    };
    Some(match kind {
        "pro" => "Pro".into(),
        // Team seats show as what they amount to: standard ≈ Pro, premium ≈ Max.
        "team" if tier.contains("premium") || tier.contains("max") => max(),
        "team" => "Pro".into(),
        "enterprise" => "Enterprise".into(),
        "free" => "Free".into(),
        "max" => max(),
        other => {
            let words = other.replace('_', " ");
            let mut c = words.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    })
}

/// Claude Code profiles: `~/.claude` ("default") and every ccacct profile in
/// `~/.claude-accounts/<name>` that has credentials.
pub fn profiles(home: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let default = home.join(".claude");
    if default.join(".credentials.json").is_file() {
        out.push(("default".to_owned(), default));
    }
    let mut extra: Vec<(String, PathBuf)> = std::fs::read_dir(home.join(".claude-accounts"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let dir = e.path();
            (!name.ends_with(".lock") && dir.join(".credentials.json").is_file()).then_some((name, dir))
        })
        .collect();
    extra.sort();
    out.extend(extra);
    out
}

/// The profiles to fetch: all but the hidden ones. "Only the default
/// account" is applied by the panel itself, so switching it shows the
/// others again at once instead of after a new request.
pub fn select(profiles: Vec<(String, PathBuf)>, panel: &Panel) -> Vec<(String, PathBuf)> {
    profiles.into_iter().filter(|(name, _)| !panel.claude_usage_hidden.contains(name)).collect()
}

fn fetch(token: &str) -> Result<String, String> {
    let mut child = Command::new("curl")
        .args(["-q", "--silent", "--config", "-", "--max-time", "10", "--write-out", "\n%{http_code}"])
        .args(["--header", "anthropic-beta: oauth-2025-04-20", "--header", "Accept: application/json"])
        .args(["--user-agent", "vela-claude-usage", ENDPOINT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "no-curl".to_owned())?;
    // The token only travels through this pipe.
    child
        .stdin
        .take()
        .ok_or("no-curl")?
        .write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes())
        .map_err(|_| "offline".to_owned())?;
    let out = child.wait_with_output().map_err(|_| "offline".to_owned())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, status) = text.rsplit_once('\n').ok_or("offline")?;
    match status.trim() {
        "200" => Ok(body.to_owned()),
        "000" | "" => Err("offline".into()),
        code => Err(format!("http-{code}")),
    }
}

fn window_json(w: Option<Window>) -> Value {
    w.map_or(Value::Null, |w| json!({ "utilization": w.utilization, "resetsAt": w.resets_at }))
}

/// Shown profiles that currently have a usable token, without any request
/// (lets the panel notice new or freshly logged-in accounts cheaply).
pub fn usable_names(home: &Path, panel: &Panel, now_ms: i64) -> Vec<String> {
    select(profiles(home), panel)
        .into_iter()
        .filter(|(_, dir)| std::fs::read_to_string(dir.join(".credentials.json")).is_ok_and(|c| read_token(&c, now_ms).is_ok()))
        .map(|(name, _)| name)
        .collect()
}

/// One JSON line with every profile's usage (or why it is unavailable).
pub fn report(home: &Path, panel: &Panel, now_ms: i64) -> String {
    let accounts: Vec<Value> = select(profiles(home), panel)
        .into_iter()
        .map(|(name, dir)| {
            let creds = std::fs::read_to_string(dir.join(".credentials.json")).ok();
            let plan = creds.as_deref().and_then(plan_of);
            let result = creds
                .ok_or_else(|| "no-credentials".to_owned())
                .and_then(|c| read_token(&c, now_ms).map_err(str::to_owned))
                .and_then(|t| fetch(&t))
                .and_then(|body| parse_usage(&body).ok_or_else(|| "no-data".to_owned()));
            match result {
                Ok((s, w)) => json!({ "name": name, "plan": plan, "status": "ok", "session": window_json(s), "week": window_json(w) }),
                Err(reason) => json!({ "name": name, "plan": plan, "status": "unavailable", "reason": reason }),
            }
        })
        .collect();
    json!({ "fetchedAt": now_ms, "accounts": accounts }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_timestamps_to_millis() {
        assert_eq!(iso_to_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_ms("2026-10-01T12:30:00+00:00"), Some(1_790_857_800_000));
        assert_eq!(iso_to_ms("2026-10-01T12:30:00.123456+00:00"), Some(1_790_857_800_000));
        assert_eq!(iso_to_ms("2026-10-01T14:30:00+02:00"), Some(1_790_857_800_000));
        assert_eq!(iso_to_ms("garbage"), None);
    }

    #[test]
    fn usage_response() {
        let body = r#"{"five_hour":{"utilization":42.0,"resets_at":"2026-10-01T12:30:00+00:00"},
                       "seven_day":{"utilization":7.5,"resets_at":null},"seven_day_opus":null}"#;
        let (session, week) = parse_usage(body).unwrap();
        assert_eq!(
            session,
            Some(Window {
                utilization: 0.42,
                resets_at: 1_790_857_800_000
            })
        );
        assert_eq!(
            week,
            Some(Window {
                utilization: 0.075,
                resets_at: 0
            })
        );
        assert!(parse_usage(r#"{"five_hour":null,"seven_day":null}"#).is_none(), "no data");
        assert!(parse_usage("not json").is_none());
    }

    #[test]
    fn tokens_that_could_break_the_curl_config_are_refused() {
        assert!(token_is_safe("sk-ant-oat01-AbC_12.3~+/="));
        assert!(!token_is_safe(""));
        assert!(!token_is_safe("abc\"\nurl = http://evil"));
        assert!(!token_is_safe("a b"));
    }

    #[test]
    fn profiles_are_found_and_named() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        std::fs::create_dir_all(h.join(".claude")).unwrap();
        std::fs::write(h.join(".claude/.credentials.json"), "{}").unwrap();
        for p in ["work", "work.lock", "empty"] {
            std::fs::create_dir_all(h.join(".claude-accounts").join(p)).unwrap();
        }
        std::fs::write(h.join(".claude-accounts/work/.credentials.json"), "{}").unwrap();
        std::fs::write(h.join(".claude-accounts/work.lock/.credentials.json"), "{}").unwrap();
        let names: Vec<String> = profiles(h).into_iter().map(|p| p.0).collect();
        assert_eq!(names, vec!["default", "work"]);
    }

    #[test]
    fn hidden_profiles_and_only_default() {
        let all = ["default", "fachschaft", "work"].map(|n| (n.to_owned(), PathBuf::from(n))).to_vec();
        let names = |p: &Panel| select(all.clone(), p).into_iter().map(|p| p.0).collect::<Vec<_>>();
        let mut panel = Panel::default();
        assert_eq!(names(&panel), vec!["default", "fachschaft", "work"]);
        panel.claude_usage_hidden = vec!["fachschaft".into()];
        assert_eq!(names(&panel), vec!["default", "work"]);
        panel.claude_usage_only_default = true;
        assert_eq!(names(&panel), vec!["default", "work"], "only-default is a display filter in the panel");
    }

    #[test]
    fn plan_names() {
        let plan = |c: &str| plan_of(c);
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"pro","rateLimitTier":"default_claude_ai"}}"#),
            Some("Pro".into())
        );
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"max","rateLimitTier":"default_claude_max_20x"}}"#),
            Some("Max 20×".into())
        );
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"max","rateLimitTier":"default_claude_max_5x"}}"#),
            Some("Max 5×".into())
        );
        // Team seats show as what they amount to: standard = Pro, premium = Max.
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"team","rateLimitTier":"default_raven"}}"#),
            Some("Pro".into())
        );
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"team","rateLimitTier":"default_raven_premium"}}"#),
            Some("Max".into())
        );
        assert_eq!(
            plan(r#"{"claudeAiOauth":{"subscriptionType":"team","rateLimitTier":"team_max_5x"}}"#),
            Some("Max 5×".into())
        );
        assert_eq!(plan(r#"{"claudeAiOauth":{"subscriptionType":"enterprise"}}"#), Some("Enterprise".into()));
        assert_eq!(plan(r#"{"claudeAiOauth":{"subscriptionType":"some_new_plan"}}"#), Some("Some new plan".into()));
        assert_eq!(plan(r#"{"claudeAiOauth":{}}"#), None);
        assert_eq!(plan("broken"), None);
    }

    #[test]
    fn usable_names_skip_expired_and_hidden() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        let creds = |exp: i64| format!(r#"{{"claudeAiOauth":{{"accessToken":"t","expiresAt":{exp}}}}}"#);
        std::fs::create_dir_all(h.join(".claude")).unwrap();
        std::fs::write(h.join(".claude/.credentials.json"), creds(5000)).unwrap();
        for (name, exp) in [("new", 5000), ("old", 10), ("hidden", 5000)] {
            std::fs::create_dir_all(h.join(".claude-accounts").join(name)).unwrap();
            std::fs::write(h.join(".claude-accounts").join(name).join(".credentials.json"), creds(exp)).unwrap();
        }
        let panel = Panel {
            claude_usage_hidden: vec!["hidden".into()],
            ..Panel::default()
        };
        assert_eq!(usable_names(h, &panel, 1000), vec!["default", "new"]);
    }

    #[test]
    fn credentials_expiry_and_token() {
        let now = 1_000;
        let ok = r#"{"claudeAiOauth":{"accessToken":"tok.en","expiresAt":2000}}"#;
        assert_eq!(read_token(ok, now), Ok("tok.en".into()));
        assert_eq!(read_token(r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":500}}"#, now), Err("expired"));
        assert_eq!(
            read_token(r#"{"claudeAiOauth":{"accessToken":"bad token","expiresAt":2000}}"#, now),
            Err("bad-credentials")
        );
        assert_eq!(read_token("{}", now), Err("no-credentials"));
    }
}
