//! Pulse: vela's task manager. Samples processes and hardware, groups
//! processes into apps, finds what's wrong, and acts on it. Logic and
//! readers live here; the window is Quickshell (`shell/pulse.qml`), fed by
//! `vela pulse serve` (JSON lines).

pub mod actions;
pub mod apps;
pub mod doctor;
pub mod engine;
pub mod health;
pub mod hw;
pub mod procfs;
pub mod recorder;
pub mod sample;
pub mod serve;
pub mod services;
