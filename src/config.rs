//! Configuration model, validation and (atomic) persistence as TOML.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    pub apps: Apps,
    pub search: Search,
    pub claude: Claude,
    pub terminal: Terminal,
    /// The Quickshell control center (`vela shell`). Style and motion come
    /// from `appearance`/`general` like the launcher's.
    pub panel: Panel,
    /// Dimming, locking, screen off and suspend while away (`vela idle`).
    pub idle: Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// Launcher width in logical pixels.
    pub width: u32,
    /// Maximum launcher height; the grid/result list scrolls beyond it.
    pub max_height: u32,
    /// Distance of the launcher from the top of the monitor, in percent.
    pub vertical_position: u32,
    /// Background opacity of the launcher (0.0–1.0).
    pub opacity: f64,
    pub close_on_focus_loss: bool,
    /// Monitor the launcher always opens on while connected: `desc:<make model
    /// serial>` or a connector name. Empty = the focused monitor.
    pub main_monitor: String,
    /// Total number of rows in the result list.
    pub max_results: u32,
    /// Start applications in their own transient systemd scope.
    pub systemd_scope: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Midnight,
    Graphite,
    Nord,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 5] = [Theme::Dark, Theme::Midnight, Theme::Graphite, Theme::Nord, Theme::Light];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Midnight => "Midnight",
            Theme::Graphite => "Graphite",
            Theme::Nord => "Nord",
            Theme::Light => "Light",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    /// Accent colour as CSS hex (`#rrggbb`).
    pub accent: String,
    pub tile_size: u32,
    pub icon_size: u32,
    pub spacing: u32,
    /// Fixed number of grid columns; 0 = derive from width and tile size.
    pub columns: u32,
    pub border_radius: u32,
    /// Opacity of tiles, rows and the search field surface (0.0–1.0).
    pub surface_opacity: f64,
    pub show_labels: bool,
    /// Accent ring around the selected grid tile.
    pub tile_outline: bool,
    /// Surface behind each grid tile; off = icons float on the panel.
    pub tile_background: bool,
    pub font_scale: f64,
    /// Open/close, grid and result animations.
    pub animations: bool,
    /// 1.0 = normal, 2.0 = twice as fast.
    pub animation_speed: f64,
    /// Blur (and dim) everything else on the launcher's monitor while it is open.
    pub backdrop: bool,
    /// Darkening of the backdrop, 0.0–0.8.
    pub backdrop_dim: f64,
    /// Blur strength of the launcher's backdrop, 1–4 (1 = Hyprland's blur).
    pub backdrop_strength: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    /// Width of the control center in logical pixels.
    pub width: u32,
    pub close_on_focus_loss: bool,
    /// Blur (and dim, `appearance.backdrop_dim`) the rest of its monitor while open.
    pub backdrop: bool,
    /// Blur strength of that backdrop, 1–4 (1 = Hyprland's blur).
    pub backdrop_strength: u32,
    /// How long a notification popup stays (apps may ask for less).
    pub popup_timeout_secs: u32,
    pub popup_max_visible: u32,
    pub critical_popups_stay: bool,
    /// Notifications shown per app before "Show more".
    pub group_collapsed_count: u32,
    /// Workspace dots at the top when switching workspaces.
    pub workspace_osd: bool,
    /// Night light colour temperature in Kelvin.
    pub night_light_temperature: u32,
    /// Clock and date centred at the top of the panel instead of on the left.
    pub clock_centered: bool,
    /// Claude plan usage at the bottom of the panel.
    pub claude_usage: bool,
    /// Only the profile in ~/.claude, not the ccacct ones.
    pub claude_usage_only_default: bool,
    /// Profile names (`default`, ccacct names) that are not shown or fetched.
    pub claude_usage_hidden: Vec<String>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            width: 420,
            close_on_focus_loss: true,
            backdrop: false,
            backdrop_strength: 1,
            popup_timeout_secs: 5,
            popup_max_visible: 4,
            critical_popups_stay: true,
            group_collapsed_count: 2,
            workspace_osd: true,
            night_light_temperature: 4000,
            clock_centered: false,
            claude_usage: true,
            claude_usage_only_default: false,
            claude_usage_hidden: Vec::new(),
        }
    }
}

/// Each step has a switch and a delay in minutes of inactivity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Idle {
    pub dim: bool,
    pub dim_after_min: f64,
    pub lock: bool,
    pub lock_after_min: f64,
    pub screen_off: bool,
    pub screen_off_after_min: f64,
    /// Off = the computer never goes to sleep on its own.
    pub suspend: bool,
    pub suspend_after_min: f64,
    /// Lock the session before the system goes to sleep.
    pub lock_before_sleep: bool,
}

impl Default for Idle {
    fn default() -> Self {
        Idle {
            dim: true,
            dim_after_min: 2.5,
            lock: true,
            lock_after_min: 5.0,
            screen_off: true,
            screen_off_after_min: 5.5,
            suspend: true,
            suspend_after_min: 30.0,
            lock_before_sleep: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GridSource {
    /// Only pinned applications (all apps if nothing is pinned).
    #[default]
    Pinned,
    /// Pinned applications first, followed by every other installed app.
    PinnedThenAll,
    /// All installed applications, alphabetically.
    All,
}

impl GridSource {
    pub const ALL: [GridSource; 3] = [GridSource::Pinned, GridSource::PinnedThenAll, GridSource::All];

    pub fn label(self) -> &'static str {
        match self {
            GridSource::Pinned => "Pinned applications",
            GridSource::PinnedThenAll => "Pinned, then all applications",
            GridSource::All => "All applications",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomAction {
    pub id: String,
    pub name: String,
    /// argv; executed directly, never through a shell.
    pub command: Vec<String>,
    pub icon: String,
    pub terminal: bool,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Apps {
    pub grid: GridSource,
    /// Desktop file IDs (`firefox.desktop`) or `custom:<id>`, in grid order.
    pub pinned: Vec<String>,
    /// Desktop file IDs that never show up in the launcher.
    pub hidden: Vec<String>,
    /// Offer desktop actions (e.g. "New Private Window") in search results.
    pub desktop_actions: bool,
    pub custom: Vec<CustomAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileBackend {
    /// plocate when installed, otherwise the built-in index.
    #[default]
    Auto,
    Plocate,
    Builtin,
}

impl FileBackend {
    pub const ALL: [FileBackend; 3] = [FileBackend::Auto, FileBackend::Plocate, FileBackend::Builtin];

    pub fn label(self) -> &'static str {
        match self {
            FileBackend::Auto => "Automatic",
            FileBackend::Plocate => "plocate",
            FileBackend::Builtin => "Built-in index",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Search {
    pub apps: bool,
    pub files: bool,
    pub claude: bool,
    pub max_app_results: u32,
    pub max_file_results: u32,
    pub file_backend: FileBackend,
    /// Only files below these directories are shown (`~` allowed).
    pub file_roots: Vec<String>,
    /// Path components that exclude a file (e.g. `node_modules`).
    pub exclude: Vec<String>,
    pub include_hidden: bool,
    pub include_directories: bool,
    pub min_file_query_len: u32,
    /// Delay before the file search runs after the last keystroke.
    pub debounce_ms: u32,
    /// Rebuild interval of the built-in index.
    pub index_interval_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Claude {
    /// Show the "Ask Claude" row even when other results exist.
    pub always_visible: bool,
    /// Shift+Enter sends the whole input to Claude.
    pub shift_enter: bool,
    /// Put "Ask Claude" first when the input reads like a question.
    pub prefer_for_questions: bool,
    pub executable: String,
    pub args: Vec<String>,
    pub working_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terminal {
    pub executable: String,
    /// Arguments between the terminal and the command (`-e` for most
    /// terminals). Unset = known default for the chosen terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_args: Option<Vec<String>>,
}

impl Default for General {
    fn default() -> Self {
        General {
            width: 760,
            max_height: 580,
            vertical_position: 18,
            opacity: 0.86,
            close_on_focus_loss: true,
            main_monitor: String::new(),
            max_results: 20,
            systemd_scope: true,
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme: Theme::Dark,
            accent: "#7aa2f7".into(),
            tile_size: 108,
            icon_size: 52,
            spacing: 8,
            columns: 0,
            border_radius: 22,
            surface_opacity: 0.06,
            show_labels: true,
            tile_outline: false,
            tile_background: true,
            font_scale: 1.0,
            animations: true,
            animation_speed: 1.0,
            backdrop: false,
            backdrop_dim: 0.18,
            backdrop_strength: 1,
        }
    }
}

impl Default for Apps {
    fn default() -> Self {
        Apps {
            grid: GridSource::Pinned,
            pinned: [
                "firefox.desktop",
                "brave-browser.desktop",
                "google-chrome.desktop",
                "chromium.desktop",
                "kitty.desktop",
                "com.mitchellh.ghostty.desktop",
                "org.kde.dolphin.desktop",
                "org.gnome.Nautilus.desktop",
                "thunar.desktop",
                "code.desktop",
                "code-oss.desktop",
                "jetbrains-idea.desktop",
                "obsidian.desktop",
                "spotify.desktop",
                "discord.desktop",
                "org.kde.kate.desktop",
                "org.gnome.TextEditor.desktop",
                "org.kde.systemsettings.desktop",
                "vela:settings",
            ]
            .map(String::from)
            .to_vec(),
            hidden: Vec::new(),
            desktop_actions: true,
            custom: Vec::new(),
        }
    }
}

impl Default for Search {
    fn default() -> Self {
        Search {
            apps: true,
            files: true,
            claude: true,
            max_app_results: 8,
            max_file_results: 10,
            file_backend: FileBackend::Auto,
            file_roots: vec!["~".into()],
            exclude: [
                ".git",
                "node_modules",
                "target",
                ".cache",
                "__pycache__",
                ".venv",
                "venv",
                ".gradle",
                ".m2",
                ".cargo",
                ".rustup",
                ".npm",
            ]
            .map(String::from)
            .to_vec(),
            include_hidden: false,
            include_directories: true,
            min_file_query_len: 2,
            debounce_ms: 80,
            index_interval_minutes: 30,
        }
    }
}

impl Default for Claude {
    fn default() -> Self {
        Claude {
            always_visible: true,
            shift_enter: true,
            prefer_for_questions: true,
            executable: "claude".into(),
            args: vec!["--dangerously-skip-permissions".into()],
            working_dir: "~".into(),
        }
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Terminal {
            executable: "kitty".into(),
            exec_args: None,
        }
    }
}

/// Known terminals and the arguments that make them run a command.
pub const TERMINAL_PRESETS: &[(&str, &[&str])] = &[
    ("kitty", &[]),
    ("ghostty", &["-e"]),
    ("foot", &[]),
    ("alacritty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("konsole", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("kgx", &["--"]),
    ("xfce4-terminal", &["-x"]),
    ("xterm", &["-e"]),
];

impl Terminal {
    pub fn effective_exec_args(&self) -> Vec<String> {
        if let Some(args) = &self.exec_args {
            return args.clone();
        }
        let name = Path::new(self.executable.trim()).file_name().and_then(|n| n.to_str()).unwrap_or("");
        TERMINAL_PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, a)| a.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec!["-e".into()])
    }
}

fn valid_hex_color(s: &str) -> bool {
    let hex = s.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

impl Config {
    /// Clamps values into sane ranges and repairs invalid ones, so the UI
    /// never has to deal with a broken configuration.
    pub fn sanitize(&mut self) {
        let g = &mut self.general;
        g.width = g.width.clamp(360, 2400);
        g.max_height = g.max_height.clamp(200, 2000);
        g.vertical_position = g.vertical_position.min(60);
        g.opacity = finite_or(g.opacity, 0.86).clamp(0.0, 1.0);
        g.max_results = g.max_results.clamp(1, 100);

        let a = &mut self.appearance;
        a.tile_size = a.tile_size.clamp(56, 240);
        a.icon_size = a.icon_size.clamp(16, 192).min(a.tile_size);
        a.spacing = a.spacing.min(64);
        a.columns = a.columns.min(16);
        a.border_radius = a.border_radius.min(64);
        a.surface_opacity = finite_or(a.surface_opacity, 0.06).clamp(0.0, 1.0);
        a.font_scale = finite_or(a.font_scale, 1.0).clamp(0.6, 2.0);
        a.animation_speed = finite_or(a.animation_speed, 1.0).clamp(0.25, 4.0);
        a.backdrop_dim = finite_or(a.backdrop_dim, 0.18).clamp(0.0, 0.8);
        a.backdrop_strength = a.backdrop_strength.clamp(1, 4);
        a.accent = a.accent.trim().to_owned();
        if !valid_hex_color(&a.accent) {
            a.accent = Appearance::default().accent;
        }

        let apps = &mut self.apps;
        dedup_keep_order(&mut apps.pinned);
        dedup_keep_order(&mut apps.hidden);
        apps.custom.retain(|c| !c.name.trim().is_empty() && !c.command.is_empty());
        for (i, c) in apps.custom.iter_mut().enumerate() {
            if c.id.trim().is_empty() {
                c.id = format!("action-{}", i + 1);
            }
        }

        let s = &mut self.search;
        s.max_app_results = s.max_app_results.clamp(1, 50);
        s.max_file_results = s.max_file_results.clamp(1, 100);
        s.min_file_query_len = s.min_file_query_len.clamp(1, 10);
        s.debounce_ms = s.debounce_ms.min(2000);
        s.index_interval_minutes = s.index_interval_minutes.clamp(1, 24 * 60);
        s.file_roots.retain(|r| !r.trim().is_empty());
        if s.file_roots.is_empty() {
            s.file_roots.push("~".into());
        }

        let p = &mut self.panel;
        p.width = p.width.clamp(320, 800);
        p.backdrop_strength = p.backdrop_strength.clamp(1, 4);
        p.popup_timeout_secs = p.popup_timeout_secs.clamp(1, 60);
        p.popup_max_visible = p.popup_max_visible.clamp(1, 10);
        p.group_collapsed_count = p.group_collapsed_count.clamp(1, 10);
        p.night_light_temperature = p.night_light_temperature.clamp(1000, 6500);

        let i = &mut self.idle;
        for m in [
            &mut i.dim_after_min,
            &mut i.lock_after_min,
            &mut i.screen_off_after_min,
            &mut i.suspend_after_min,
        ] {
            *m = finite_or(*m, 5.0).clamp(0.5, 720.0);
        }

        if self.claude.executable.trim().is_empty() {
            self.claude.executable = Claude::default().executable;
        }
        if self.claude.working_dir.trim().is_empty() {
            self.claude.working_dir = "~".into();
        }
        if self.terminal.executable.trim().is_empty() {
            self.terminal.executable = Terminal::default().executable;
        }
    }

    pub fn from_toml(text: &str) -> Result<Config> {
        let mut cfg: Config = toml::from_str(text)?;
        cfg.sanitize();
        Ok(cfg)
    }

    pub fn to_toml(&self) -> String {
        let body = toml::to_string_pretty(self).unwrap_or_default();
        format!("{HEADER}\n{body}")
    }
}

const HEADER: &str = "# vela configuration. Edited by the settings window (`vela settings`);
# manual edits are picked up live. See README.md for every option.";

fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() { v } else { fallback }
}

fn dedup_keep_order(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| !s.trim().is_empty() && seen.insert(s.clone()));
}

pub enum LoadOutcome {
    Loaded(Config),
    /// No config yet; defaults were written.
    Created(Config),
}

/// Loads the config file; creates it with defaults if it doesn't exist.
/// A syntax error is returned as `Err` so callers can keep the old config.
pub fn load_or_create(path: &Path) -> Result<LoadOutcome> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(LoadOutcome::Loaded(
            Config::from_toml(&text).with_context(|| format!("invalid config {}", path.display()))?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let cfg = Config::default();
            save(path, &cfg)?;
            Ok(LoadOutcome::Created(cfg))
        }
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Atomic write (temp file + rename) so readers never see half a file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().context("config path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let tmp: PathBuf = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("vela"),
        std::process::id()
    ));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn save(path: &Path, cfg: &Config) -> Result<()> {
    write_atomic(path, &cfg.to_toml())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let cfg = Config::default();
        let text = cfg.to_toml();
        assert!(text.starts_with("# vela configuration"));
        assert_eq!(Config::from_toml(&text).unwrap(), cfg);
    }

    #[test]
    fn partial_file_uses_defaults_for_the_rest() {
        let cfg = Config::from_toml("[general]\nwidth = 900\n[claude]\nshift_enter = false\n").unwrap();
        assert_eq!(cfg.general.width, 900);
        assert_eq!(cfg.general.max_height, General::default().max_height);
        assert!(!cfg.claude.shift_enter);
        assert_eq!(cfg.search, Search::default());
        assert_eq!(cfg.general.main_monitor, "", "no main monitor = follow focus");
        let cfg = Config::from_toml("[general]\nmain_monitor = \"desc:HP Inc. HP E243i 6CM8191WP7\"\n").unwrap();
        assert_eq!(cfg.general.main_monitor, "desc:HP Inc. HP E243i 6CM8191WP7");
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert!(Config::from_toml("future_option = 1\n[general]\nfoo = \"bar\"\n").is_ok());
    }

    #[test]
    fn syntax_and_type_errors_are_reported() {
        assert!(Config::from_toml("[general\nwidth = 1").is_err());
        assert!(Config::from_toml("[general]\nwidth = \"wide\"").is_err());
    }

    #[test]
    fn sanitize_clamps_and_repairs() {
        let cfg = Config::from_toml(
            "[general]\nwidth = 5\nopacity = 7.5\n[appearance]\naccent = \"red\"\ntile_size = 60\nicon_size = 100\n\
             [apps]\npinned = [\"a.desktop\", \"a.desktop\", \"\", \"b.desktop\"]\n\
             [[apps.custom]]\nname = \"\"\ncommand = [\"x\"]\n[[apps.custom]]\nname = \"Ok\"\ncommand = [\"y\"]\n\
             [search]\nfile_roots = []\n[claude]\nexecutable = \"  \"\n",
        )
        .unwrap();
        assert_eq!(cfg.general.width, 360);
        assert_eq!(cfg.general.opacity, 1.0);
        assert_eq!(cfg.appearance.accent, Appearance::default().accent);
        assert_eq!(cfg.appearance.icon_size, 60);
        assert_eq!(cfg.apps.pinned, vec!["a.desktop", "b.desktop"]);
        assert_eq!(cfg.apps.custom.len(), 1);
        assert_eq!(cfg.apps.custom[0].id, "action-1");
        assert_eq!(cfg.search.file_roots, vec!["~"]);
        assert_eq!(cfg.claude.executable, "claude");
    }

    #[test]
    fn example_config_is_valid_and_matches_the_defaults() {
        let cfg = Config::from_toml(include_str!("../data/config.example.toml")).unwrap();
        assert_eq!(cfg.appearance.backdrop, Appearance::default().backdrop);
        assert_eq!(cfg.general.main_monitor, General::default().main_monitor);
        assert_eq!(cfg.panel, Panel::default());
        assert_eq!(cfg.idle, Idle::default());
    }

    #[test]
    fn backdrop_is_off_by_default_and_its_dim_is_clamped() {
        let a = Appearance::default();
        assert!(!a.backdrop, "blurring the desktop is opt-in");
        assert!((0.0..=0.8).contains(&a.backdrop_dim));
        let cfg = Config::from_toml("[appearance]\nbackdrop = true\nbackdrop_dim = 3.0\n").unwrap();
        assert!(cfg.appearance.backdrop);
        assert_eq!(cfg.appearance.backdrop_dim, 0.8, "never black out the screen");
        let cfg = Config::from_toml("[appearance]\nbackdrop_dim = -1.0\n").unwrap();
        assert_eq!(cfg.appearance.backdrop_dim, 0.0);
        assert_eq!((a.backdrop_strength, Panel::default().backdrop_strength), (1, 1), "1 = Hyprland's blur as is");
        let cfg = Config::from_toml("[appearance]\nbackdrop_strength = 9\n[panel]\nbackdrop_strength = 0\n").unwrap();
        assert_eq!((cfg.appearance.backdrop_strength, cfg.panel.backdrop_strength), (4, 1));
    }

    #[test]
    fn panel_defaults_and_partial_files() {
        let p = Panel::default();
        assert_eq!(p.width, 420);
        assert!(p.close_on_focus_loss);
        assert!(!p.backdrop, "blurring the desktop is opt-in for the panel too");
        assert_eq!((p.popup_timeout_secs, p.popup_max_visible, p.group_collapsed_count), (5, 4, 2));
        assert!(p.critical_popups_stay && p.workspace_osd);
        assert_eq!(p.night_light_temperature, 4000);
        assert!(!p.clock_centered, "clock on the left by default");
        assert!(p.claude_usage && !p.claude_usage_only_default && p.claude_usage_hidden.is_empty());
        let cfg = Config::from_toml("[panel]\nbackdrop = true\n").unwrap();
        assert!(cfg.panel.backdrop);
        assert!(!cfg.appearance.backdrop, "launcher and panel backdrop are independent");
        assert_eq!(cfg.panel.width, 420);
    }

    #[test]
    fn panel_values_are_clamped() {
        let cfg = Config::from_toml(
            "[panel]\nwidth = 10\npopup_timeout_secs = 0\npopup_max_visible = 99\ngroup_collapsed_count = 0\nnight_light_temperature = 100000\n",
        )
        .unwrap();
        assert_eq!(cfg.panel.width, 320);
        assert_eq!(cfg.panel.popup_timeout_secs, 1);
        assert_eq!(cfg.panel.popup_max_visible, 10);
        assert_eq!(cfg.panel.group_collapsed_count, 1);
        assert_eq!(cfg.panel.night_light_temperature, 6500);
    }

    #[test]
    fn idle_defaults_and_clamping() {
        let i = Idle::default();
        assert_eq!(
            (i.dim_after_min, i.lock_after_min, i.screen_off_after_min, i.suspend_after_min),
            (2.5, 5.0, 5.5, 30.0)
        );
        assert!(i.lock_before_sleep);
        assert!(i.dim && i.lock && i.screen_off && i.suspend, "all on, like the hypridle sample");
        let cfg = Config::from_toml("[idle]\nsuspend = false\nlock_after_min = -3\ndim_after_min = 99999\n").unwrap();
        assert!(!cfg.idle.suspend && cfg.idle.lock);
        assert_eq!(cfg.idle.suspend_after_min, 30.0, "the time is kept while switched off");
        assert_eq!(cfg.idle.lock_after_min, 0.5);
        assert_eq!(cfg.idle.dim_after_min, 720.0);
    }

    #[test]
    fn terminal_presets() {
        let mut t = Terminal {
            executable: "kitty".into(),
            exec_args: None,
        };
        assert!(t.effective_exec_args().is_empty());
        t.executable = "/usr/bin/ghostty".into();
        assert_eq!(t.effective_exec_args(), vec!["-e"]);
        t.executable = "my-term".into();
        assert_eq!(t.effective_exec_args(), vec!["-e"]);
        t.exec_args = Some(vec!["--run".into()]);
        assert_eq!(t.effective_exec_args(), vec!["--run"]);
    }

    #[test]
    fn load_creates_saves_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");
        let LoadOutcome::Created(mut cfg) = load_or_create(&path).unwrap() else {
            panic!("expected creation")
        };
        assert!(path.exists());
        cfg.general.width = 1000;
        cfg.apps.pinned = vec!["x.desktop".into()];
        cfg.claude.args = vec!["--dangerously-skip-permissions".into(), "--model".into(), "opus".into()];
        save(&path, &cfg).unwrap();
        let LoadOutcome::Loaded(back) = load_or_create(&path).unwrap() else {
            panic!("expected load")
        };
        assert_eq!(back, cfg);
        // No temp files left behind.
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn broken_file_is_an_error_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[[[").unwrap();
        assert!(load_or_create(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[[");
    }
}
