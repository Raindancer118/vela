//! The vela background process (GTK). Usually started via `vela daemon`,
//! the systemd user unit, or automatically by `vela toggle`.

use std::process::ExitCode;

fn main() -> ExitCode {
    vela::paths::complete_nixos_env();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("vela=info"))
        .format_timestamp_millis()
        .init();
    let show = std::env::args().skip(1).any(|a| a == "--show");
    let initial = show.then_some(vela::ipc::Command::Show);
    if vela::ui::daemon::run(vela::ui::daemon::Options { initial }) == gtk::glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
