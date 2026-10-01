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
const PROFILE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/profile";
/// The plan rarely changes: asked this often per account (and again sooner
/// after a failed request).
pub const PROFILE_INTERVAL_MS: i64 = 6 * 3_600_000;
pub const PROFILE_RETRY_MS: i64 = 15 * 60_000;

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

/// Display name of the account's plan from `.credentials.json` (written at
/// login, so it can lag behind: see `plan_from_profile`).
pub fn plan_of(credentials: &str) -> Option<String> {
    let v: Value = serde_json::from_str(credentials).ok()?;
    let oauth = v.get("claudeAiOauth")?;
    let kind = oauth.get("subscriptionType").and_then(Value::as_str).filter(|s| !s.is_empty())?;
    let tier = oauth.get("rateLimitTier").and_then(Value::as_str).unwrap_or("");
    Some(plan_name(kind, tier))
}

/// The plan as Anthropic has it now, from `/api/oauth/profile`: the
/// organisation's type and rate-limit tier (a team seat upgraded to Max
/// shows as Max there long before the local credentials notice).
pub fn plan_from_profile(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let org = v.get("organization");
    let tier = org.and_then(|o| o.get("rate_limit_tier")).and_then(Value::as_str).unwrap_or("");
    let kind = match org.and_then(|o| o.get("organization_type")).and_then(Value::as_str) {
        Some(t) if !t.is_empty() => t.strip_prefix("claude_").unwrap_or(t).to_owned(),
        _ => {
            let acc = v.get("account")?;
            let flag = |k: &str| acc.get(k).and_then(Value::as_bool).unwrap_or(false);
            if flag("has_claude_max") {
                "max".into()
            } else if flag("has_claude_pro") {
                "pro".into()
            } else {
                return None;
            }
        }
    };
    Some(plan_name(&kind, tier))
}

fn plan_name(kind: &str, tier: &str) -> String {
    // "…_max_20x" → "Max 20×"; any Max-like tier without a number → "Max".
    let max = || match tier.rsplit('_').next().and_then(|t| t.strip_suffix('x')) {
        Some(n) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => format!("Max {n}×"),
        _ => "Max".into(),
    };
    match kind {
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
    }
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

/// Why a request failed; `retry_after_ms` from the Retry-After header.
#[derive(Debug, Clone, PartialEq)]
pub struct FetchError {
    pub reason: String,
    pub retry_after_ms: Option<i64>,
}

impl FetchError {
    fn new(reason: &str) -> FetchError {
        FetchError {
            reason: reason.into(),
            retry_after_ms: None,
        }
    }
}

/// The usage request with curl; the token only travels through its stdin.
pub fn fetch(token: &str) -> Result<String, FetchError> {
    get(ENDPOINT, token)
}

/// The profile request (the current plan).
pub fn fetch_profile(token: &str) -> Result<String, FetchError> {
    get(PROFILE_ENDPOINT, token)
}

fn get(url: &str, token: &str) -> Result<String, FetchError> {
    let mut child = Command::new("curl")
        .args(["-q", "--silent", "--config", "-", "--max-time", "10"])
        .args(["--write-out", "\n%{http_code} %header{retry-after}"])
        .args(["--header", "anthropic-beta: oauth-2025-04-20", "--header", "Accept: application/json"])
        .args(["--user-agent", "vela-claude-usage", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| FetchError::new("no-curl"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| FetchError::new("no-curl"))?
        .write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes())
        .map_err(|_| FetchError::new("offline"))?;
    let out = child.wait_with_output().map_err(|_| FetchError::new("offline"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, tail) = text.rsplit_once('\n').ok_or_else(|| FetchError::new("offline"))?;
    let mut parts = tail.split_whitespace();
    let status = parts.next().unwrap_or("");
    let retry_after_ms = parts.next().and_then(|s| s.parse::<i64>().ok()).map(|s| s * 1000);
    match status {
        "200" => Ok(body.to_owned()),
        "000" | "" => Err(FetchError::new("offline")),
        code => Err(FetchError {
            reason: format!("http-{code}"),
            retry_after_ms,
        }),
    }
}

/// Shortest pause between two requests for one account, and the longest
/// back-off after repeated rate limiting.
pub const MIN_INTERVAL_MS: i64 = 60_000;
pub const MAX_INTERVAL_MS: i64 = 15 * 60_000;
/// Last good numbers are shown at most this long after they were fetched;
/// the panel applies the same limit (services/ClaudeUsage.qml).
pub const MAX_AGE_MS: i64 = 5 * 60_000;

/// Asks the profile endpoint with a token.
pub type ProfileFetch = Box<dyn FnMut(&str) -> Result<String, FetchError> + Send>;

/// (5-hour, 7-day) windows of one answer.
type Windows = (Option<Window>, Option<Window>);

#[derive(Default)]
struct AccountState {
    plan: Option<String>,
    /// From the profile endpoint; wins over the credentials' plan.
    live_plan: Option<String>,
    profile_next_at: i64,
    /// Last successful windows and when they were fetched.
    last_ok: Option<(Windows, i64)>,
    reason: Option<String>,
    interval_ms: i64,
    next_at: i64,
}

/// Background polling (`vela-daemon`): each account on its own schedule, as
/// often as the endpoint allows. Starts at MIN_INTERVAL_MS, doubles on
/// HTTP 429 (or waits Retry-After) up to MAX_INTERVAL_MS, and speeds up again
/// after successes. Expired or hidden accounts cost no request.
#[derive(Default)]
pub struct Poller {
    accounts: std::collections::BTreeMap<String, AccountState>,
    order: Vec<String>,
    /// Asks for the current plan; None (tests) keeps the credentials' plan.
    pub profile: Option<ProfileFetch>,
}

impl Poller {
    pub fn next_at(&self, name: &str) -> Option<i64> {
        self.accounts.get(name).map(|a| a.next_at)
    }

    /// One round; true if the snapshot changed.
    pub fn tick(&mut self, home: &Path, panel: &Panel, now_ms: i64, fetch: &mut dyn FnMut(&str) -> Result<String, FetchError>) -> bool {
        let selected = select(profiles(home), panel);
        let names: Vec<String> = selected.iter().map(|(n, _)| n.clone()).collect();
        let mut changed = names != self.order;
        self.accounts.retain(|n, _| names.contains(n));
        self.order = names;
        for (name, dir) in selected {
            let acc = self.accounts.entry(name).or_default();
            if now_ms < acc.next_at {
                continue;
            }
            let creds = std::fs::read_to_string(dir.join(".credentials.json")).ok();
            let token = creds.clone().ok_or("no-credentials").and_then(|c| read_token(&c, now_ms));
            if let (Some(ask), Ok(t)) = (self.profile.as_mut(), &token)
                && now_ms >= acc.profile_next_at
            {
                match ask(t).ok().as_deref().and_then(plan_from_profile) {
                    Some(p) => {
                        acc.live_plan = Some(p);
                        acc.profile_next_at = now_ms + PROFILE_INTERVAL_MS;
                    }
                    None => acc.profile_next_at = now_ms + PROFILE_RETRY_MS,
                }
            }
            let plan = acc.live_plan.clone().or_else(|| creds.as_deref().and_then(plan_of));
            changed |= plan != acc.plan;
            acc.plan = plan;
            let interval = acc.interval_ms.max(MIN_INTERVAL_MS);
            match token {
                // No request: look again soon (Claude Code may log in or refresh).
                Err(reason) => {
                    changed |= acc.reason.as_deref() != Some(reason);
                    acc.reason = Some(reason.into());
                    acc.next_at = now_ms + MIN_INTERVAL_MS;
                }
                Ok(token) => match fetch(&token).and_then(|b| parse_usage(&b).ok_or_else(|| FetchError::new("no-data"))) {
                    Ok(windows) => {
                        changed = true;
                        acc.last_ok = Some((windows, now_ms));
                        acc.reason = None;
                        acc.interval_ms = (interval * 3 / 4).max(MIN_INTERVAL_MS);
                        acc.next_at = now_ms + acc.interval_ms;
                    }
                    Err(e) => {
                        changed |= acc.reason.as_deref() != Some(e.reason.as_str());
                        let limited = e.reason == "http-429";
                        acc.interval_ms = if limited { (interval * 2).min(MAX_INTERVAL_MS) } else { interval };
                        acc.next_at = now_ms + acc.interval_ms.max(e.retry_after_ms.unwrap_or(0));
                        acc.reason = Some(e.reason);
                    }
                },
            }
        }
        changed
    }

    /// The cache file content the panel reads.
    pub fn snapshot(&self, now_ms: i64) -> String {
        let accounts: Vec<Value> = self
            .order
            .iter()
            .filter_map(|name| {
                let a = self.accounts.get(name)?;
                Some(match a.last_ok {
                    Some(((s, w), at)) if now_ms - at <= MAX_AGE_MS => json!({
                        "name": name, "plan": a.plan, "status": "ok", "fetchedAt": at,
                        "session": window_json(s), "week": window_json(w), "problem": a.reason,
                    }),
                    _ => json!({ "name": name, "plan": a.plan, "status": "unavailable", "reason": a.reason }),
                })
            })
            .collect();
        json!({ "fetchedAt": now_ms, "accounts": accounts }).to_string()
    }
}

fn window_json(w: Option<Window>) -> Value {
    w.map_or(Value::Null, |w| json!({ "utilization": w.utilization, "resetsAt": w.resets_at }))
}

/// Where the background poller keeps its results (read by the panel).
pub fn cache_file() -> PathBuf {
    crate::paths::cache_dir().join("claude-usage.json")
}

/// Starts the background poller thread (in `vela-daemon`). Set
/// VELA_NO_CLAUDE_USAGE=1 for test daemons that share the real home.
pub fn spawn_poller() {
    use crate::components::{Component, has};
    // Only the panel shows it.
    if !(has(Component::Claude) && has(Component::Panel)) {
        return;
    }
    if std::env::var_os("VELA_NO_CLAUDE_USAGE").is_some_and(|v| !v.is_empty()) {
        return;
    }
    std::thread::Builder::new()
        .name("claude-usage".into())
        .spawn(|| {
            let mut poller = Poller {
                profile: Some(Box::new(fetch_profile)),
                ..Poller::default()
            };
            loop {
                let cfg = std::fs::read_to_string(crate::paths::config_file())
                    .ok()
                    .and_then(|t| crate::config::Config::from_toml(&t).ok())
                    .unwrap_or_default();
                if cfg.panel.claude_usage {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as i64);
                    if poller.tick(&crate::paths::home_dir(), &cfg.panel, now, &mut |t| fetch(t)) {
                        if let Err(e) = crate::config::write_atomic(&cache_file(), &poller.snapshot(now)) {
                            log::warn!("claude usage: cannot write cache: {e:#}");
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        })
        .expect("spawn claude-usage thread");
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
            let token = creds
                .clone()
                .ok_or_else(|| "no-credentials".to_owned())
                .and_then(|c| read_token(&c, now_ms).map_err(str::to_owned));
            let live = token.as_ref().ok().and_then(|t| fetch_profile(t).ok()).as_deref().and_then(plan_from_profile);
            let plan = live.or_else(|| creds.as_deref().and_then(plan_of));
            let result = token
                .and_then(|t| fetch(&t).map_err(|e| e.reason))
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
    fn plan_from_the_profile_endpoint() {
        let team_max = r#"{"account":{"has_claude_max":false,"has_claude_pro":true},
            "organization":{"organization_type":"claude_team","rate_limit_tier":"default_claude_max_5x","seat_tier":"team_tier_1"}}"#;
        assert_eq!(plan_from_profile(team_max).as_deref(), Some("Max 5×"), "upgraded team seat");
        let pro = r#"{"organization":{"organization_type":"claude_pro","rate_limit_tier":"default_claude_ai"}}"#;
        assert_eq!(plan_from_profile(pro).as_deref(), Some("Pro"));
        let max = r#"{"organization":{"organization_type":"claude_max","rate_limit_tier":"default_claude_max_20x"}}"#;
        assert_eq!(plan_from_profile(max).as_deref(), Some("Max 20×"));
        let flags_only = r#"{"account":{"has_claude_max":true}}"#;
        assert_eq!(plan_from_profile(flags_only).as_deref(), Some("Max"));
        assert_eq!(plan_from_profile(r#"{"account":{}}"#), None);
        assert_eq!(plan_from_profile("<html>"), None);
    }

    #[test]
    fn poller_prefers_the_live_plan_and_asks_rarely() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        creds(&h.join(".claude"), i64::MAX / 2);
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let a = asked.clone();
        let mut p = Poller {
            profile: Some(Box::new(move |_| {
                a.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(r#"{"organization":{"organization_type":"claude_team","rate_limit_tier":"default_claude_max_5x"}}"#.into())
            })),
            ..Poller::default()
        };
        let panel = Panel::default();
        let mut ok = |_: &str| -> Result<String, FetchError> { Ok(BODY.into()) };
        p.tick(h, &panel, 0, &mut ok);
        let snap: Value = serde_json::from_str(&p.snapshot(1)).unwrap();
        assert_eq!(snap["accounts"][0]["plan"], "Max 5×", "live plan over the credentials' Pro");
        p.tick(h, &panel, MIN_INTERVAL_MS, &mut ok);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1, "not again before PROFILE_INTERVAL_MS");
        p.tick(h, &panel, PROFILE_INTERVAL_MS, &mut ok);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2);
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

    fn creds(dir: &Path, exp: i64) {
        std::fs::create_dir_all(dir).unwrap();
        let c = format!(r#"{{"claudeAiOauth":{{"accessToken":"tok","expiresAt":{exp},"subscriptionType":"pro"}}}}"#);
        std::fs::write(dir.join(".credentials.json"), c).unwrap();
    }

    const BODY: &str = r#"{"five_hour":{"utilization":10.0,"resets_at":null},"seven_day":{"utilization":20.0,"resets_at":null}}"#;

    #[test]
    fn poller_fetches_each_account_on_its_own_schedule() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        creds(&h.join(".claude"), 10_000_000);
        creds(&h.join(".claude-accounts/old"), 5);
        let mut p = Poller::default();
        let calls = std::cell::Cell::new(0);
        let mut ok = |_: &str| -> Result<String, FetchError> {
            calls.set(calls.get() + 1);
            Ok(BODY.into())
        };
        let panel = Panel::default();
        assert!(p.tick(h, &panel, 1_000, &mut ok), "first tick fetches and changes the snapshot");
        assert_eq!(calls.get(), 1, "expired accounts are never fetched");
        assert!(!p.tick(h, &panel, 1_000 + MIN_INTERVAL_MS - 1, &mut ok));
        assert_eq!(calls.get(), 1, "not before the interval");
        p.tick(h, &panel, 1_000 + MIN_INTERVAL_MS, &mut ok);
        assert_eq!(calls.get(), 2);
        let snap: Value = serde_json::from_str(&p.snapshot(2_000)).unwrap();
        let names: Vec<&str> = snap["accounts"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["default", "old"]);
        assert_eq!(snap["accounts"][0]["status"], "ok");
        assert_eq!(snap["accounts"][0]["plan"], "Pro");
        assert_eq!(snap["accounts"][0]["session"]["utilization"], 0.1);
        assert_eq!(snap["accounts"][1]["status"], "unavailable");
    }

    #[test]
    fn poller_backs_off_on_429_and_honours_retry_after() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        creds(&h.join(".claude"), i64::MAX / 2);
        let mut p = Poller::default();
        let panel = Panel::default();
        let mut limited = |_: &str| -> Result<String, FetchError> {
            Err(FetchError {
                reason: "http-429".into(),
                retry_after_ms: None,
            })
        };
        p.tick(h, &panel, 0, &mut limited);
        assert_eq!(p.next_at("default"), Some(2 * MIN_INTERVAL_MS), "doubled after a 429");
        p.tick(h, &panel, 2 * MIN_INTERVAL_MS, &mut limited);
        assert_eq!(p.next_at("default"), Some(2 * MIN_INTERVAL_MS + 4 * MIN_INTERVAL_MS));
        let mut told = |_: &str| -> Result<String, FetchError> {
            Err(FetchError {
                reason: "http-429".into(),
                retry_after_ms: Some(3_600_000),
            })
        };
        let t = 6 * MIN_INTERVAL_MS;
        p.tick(h, &panel, t, &mut told);
        assert_eq!(p.next_at("default"), Some(t + 3_600_000), "Retry-After wins when longer");
        // Success afterwards keeps the last numbers and speeds up again.
        let mut ok = |_: &str| -> Result<String, FetchError> { Ok(BODY.into()) };
        let t = t + 3_600_000;
        p.tick(h, &panel, t, &mut ok);
        assert!(p.next_at("default").unwrap() - t < MAX_INTERVAL_MS);
    }

    #[test]
    fn poller_keeps_last_numbers_through_failures_and_skips_hidden() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        creds(&h.join(".claude"), i64::MAX / 2);
        creds(&h.join(".claude-accounts/work"), i64::MAX / 2);
        let mut p = Poller::default();
        let mut panel = Panel::default();
        let mut ok = |_: &str| -> Result<String, FetchError> { Ok(BODY.into()) };
        p.tick(h, &panel, 0, &mut ok);
        let mut down = |_: &str| -> Result<String, FetchError> {
            Err(FetchError {
                reason: "offline".into(),
                retry_after_ms: None,
            })
        };
        p.tick(h, &panel, MIN_INTERVAL_MS, &mut down);
        let snap: Value = serde_json::from_str(&p.snapshot(MIN_INTERVAL_MS)).unwrap();
        assert_eq!(snap["accounts"][0]["status"], "ok", "last good numbers stay");
        assert_eq!(snap["accounts"][0]["fetchedAt"], 0);
        panel.claude_usage_hidden = vec!["work".into()];
        p.tick(h, &panel, 2 * MIN_INTERVAL_MS, &mut down);
        let snap: Value = serde_json::from_str(&p.snapshot(2 * MIN_INTERVAL_MS)).unwrap();
        assert_eq!(snap["accounts"].as_array().unwrap().len(), 1, "hidden accounts drop out");
        // Numbers older than MAX_AGE are not shown as current anymore.
        let snap: Value = serde_json::from_str(&p.snapshot(MAX_AGE_MS + 1)).unwrap();
        assert_eq!(snap["accounts"][0]["status"], "unavailable");
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
