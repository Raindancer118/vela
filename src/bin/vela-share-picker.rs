//! `vela share-picker` under its own name, for xdph's `custom_picker_binary`
//! (which takes a path, not a command line).

use std::os::unix::process::CommandExt;
use std::process::ExitCode;

fn main() -> ExitCode {
    let vela = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("vela")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "vela".into());
    let err = std::process::Command::new(&vela).arg("share-picker").args(std::env::args_os().skip(1)).exec();
    eprintln!("vela-share-picker: cannot run {}: {err}", vela.display());
    ExitCode::FAILURE
}
