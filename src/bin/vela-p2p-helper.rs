//! Opens a Wi-Fi Direct group for a Quick Share transfer; runs as root
//! through polkit. See `vela::wifidirect::helper`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    vela::wifidirect::helper::main(&args)
}
