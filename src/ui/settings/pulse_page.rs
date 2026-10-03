//! Settings → Pulse: the task manager's window and background recording.

use super::binder::{Binder, group};
use crate::config::PULSE_PAGES;
use adw::prelude::*;

pub fn build(b: &Binder) -> adw::PreferencesPage {
    let p = adw::PreferencesPage::new();

    let open = group(
        "Pulse",
        "Task manager: apps, processes, performance, diagnosis and services. Ctrl+Shift+Esc opens it.",
    );
    let row = adw::ActionRow::builder()
        .title("Open Pulse")
        .subtitle("Also in the app menu, or run vela-pulse")
        .activatable(true)
        .build();
    let go = gtk::Button::builder()
        .icon_name("go-next-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&go);
    let launch = || {
        let exe = std::env::current_exe().ok().map(|e| e.with_file_name("vela")).filter(|p| p.exists());
        let spec = crate::launch::SpawnSpec {
            argv: vec![exe.map_or("vela".into(), |p| p.to_string_lossy().into_owned()), "pulse".into()],
            name: "vela-pulse".into(),
            ..Default::default()
        };
        let _ = crate::launch::spawn_detached(&spec, false);
    };
    row.connect_activated(move |_| launch());
    go.connect_clicked(move |_| launch());
    open.add(&row);
    p.add(&open);

    let window = group("Window", "");
    window.add(&b.combo(
        "Opens on",
        "",
        &["Overview", "Apps & processes", "Performance", "Diagnosis", "Services", "Activity"],
        |c| PULSE_PAGES.iter().position(|p| *p == c.pulse.start_page).unwrap_or(0),
        |c, i| c.pulse.start_page = PULSE_PAGES.get(i).copied().unwrap_or("overview").into(),
    ));
    window.add(&b.switch(
        "Smooth graphs",
        "Graphs glide between measurements. Off: they move one step per measurement, exactly as measured",
        |c| c.pulse.smooth_graphs,
        |c, v| c.pulse.smooth_graphs = v,
    ));
    window.add(&b.spin(
        "Update interval",
        "Seconds between measurements while the window is open",
        0.25,
        10.0,
        0.25,
        2,
        |c| c.pulse.interval_ms as f64 / 1000.0,
        |c, v| c.pulse.interval_ms = (v * 1000.0).round() as u32,
    ));
    window.add(&b.combo(
        "Graph range",
        "How far back the performance graphs reach",
        &["1 minute", "5 minutes"],
        |c| usize::from(c.pulse.range_secs >= 300),
        |c, i| c.pulse.range_secs = if i == 1 { 300 } else { 60 },
    ));
    window.add(&b.switch(
        "Heat map",
        "Tint the CPU, memory, disk and GPU cells of the process table by load",
        |c| c.pulse.heat_map,
        |c, v| c.pulse.heat_map = v,
    ));
    window.add(&b.switch(
        "Ask before ending a task",
        "Force quitting and stopping system services always ask",
        |c| c.pulse.confirm_end,
        |c, v| c.pulse.confirm_end = v,
    ));
    window.add(&b.switch(
        "Show kernel threads",
        "List the kernel's own threads in Apps & processes",
        |c| c.pulse.show_kernel,
        |c, v| c.pulse.show_kernel = v,
    ));
    p.add(&window);

    let keys = group(
        "Keys",
        "For the selected app or process in Apps & processes, or the app whose details are open. One key (k) or Delete, Insert, Home, End, Space, F1–F12; empty = off. The keys act right away, without asking.",
    );
    keys.add(&b.entry("Kill (force quit)", |c| c.pulse.key_force.clone(), |c, v| c.pulse.key_force = v));
    keys.add(&b.entry("End (ask to close)", |c| c.pulse.key_end.clone(), |c, v| c.pulse.key_end = v));
    keys.add(&b.entry("Restart", |c| c.pulse.key_restart.clone(), |c, v| c.pulse.key_restart = v));
    keys.add(&b.entry("Pause or resume", |c| c.pulse.key_pause.clone(), |c, v| c.pulse.key_pause = v));
    keys.add(&b.entry("Efficiency mode", |c| c.pulse.key_efficiency.clone(), |c, v| c.pulse.key_efficiency = v));
    p.add(&keys);

    let rec = group(
        "In the background",
        "The vela daemon keeps a light record, so Pulse opens with history and knows what crashed, hung or ran out of memory while it was closed.",
    );
    let exp = b.expander(
        "Record in the background",
        "About 1 % of one core; nothing leaves your computer",
        |c| c.pulse.record,
        |c, v| c.pulse.record = v,
    );
    exp.add_row(&b.spin(
        "Every",
        "Seconds between background measurements",
        1.0,
        60.0,
        1.0,
        0,
        |c| c.pulse.record_interval_secs as f64,
        |c, v| c.pulse.record_interval_secs = v.round() as u32,
    ));
    exp.add_row(&b.spin(
        "Keep",
        "Minutes of history",
        5.0,
        1440.0,
        5.0,
        0,
        |c| c.pulse.history_minutes as f64,
        |c, v| c.pulse.history_minutes = v.round() as u32,
    ));
    rec.add(&exp);
    p.add(&rec);
    p
}
