//! Opens an access point for a Quick Share transfer; runs as root through
//! polkit. See `vela::wifihotspot::helper`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    vela::wifihotspot::helper::main(&args)
}
