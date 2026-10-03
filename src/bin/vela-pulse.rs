//! `vela pulse` under its own name: the task manager as a program of its
//! own (vela-pulse.desktop, menus, `vela-pulse serve` etc. pass through).

use std::os::unix::process::CommandExt;
use std::process::ExitCode;

fn main() -> ExitCode {
    let vela = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("vela")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "vela".into());
    let err = std::process::Command::new(&vela).arg("pulse").args(std::env::args_os().skip(1)).exec();
    eprintln!("vela-pulse: cannot run {}: {err}", vela.display());
    ExitCode::FAILURE
}
