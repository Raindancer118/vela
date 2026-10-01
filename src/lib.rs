//! vela — a Spotlight-style launcher for Hyprland.
//!
//! The pure logic (desktop entries, search, config, launching) lives in
//! modules without GTK so it can be tested and used by the lean CLI client;
//! `ui` contains everything GTK.

pub mod apps;
pub mod claude_usage;
pub mod config;
pub mod history;
pub mod hypranim;
pub mod hyprconf;
pub mod hyprland;
pub mod hyprmon;
pub mod idle;
pub mod ipc;
pub mod launch;
pub mod mcp;
pub mod paths;
pub mod search;
pub mod shell;
pub mod theme;
pub mod ui;
