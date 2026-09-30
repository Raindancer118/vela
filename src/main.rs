//! `vela` command-line client. It links no GTK, so `vela toggle` costs about a
//! millisecond; the GUI lives in the `vela-daemon` binary.

use clap::{Parser, Subcommand};
use std::os::unix::process::CommandExt;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use vela::ipc::{self, Command, SendError};
use vela::{config, launch, paths};

/// Spotlight-style launcher for Hyprland with file search and Claude Code.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Clone)]
enum Cmd {
    /// Run the background daemon (normally started by systemd).
    Daemon {
        /// Show the launcher right after start.
        #[arg(long)]
        show: bool,
    },
    /// Show or hide the launcher (default). Starts the daemon if needed.
    Toggle,
    /// Show the launcher.
    Show,
    /// Hide the launcher.
    Hide,
    /// Open the settings window, optionally on a page
    /// (general, appearance, apps, search, claude).
    Settings {
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(ipc::SETTINGS_PAGES))]
        page: Option<String>,
    },
    /// Reload configuration, applications and the file index.
    Reload,
    /// Stop the daemon.
    Quit,
    /// Exit successfully if the daemon is running.
    Status,
    /// Print the default configuration (TOML).
    DefaultConfig,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("vela=info"))
        .format_timestamp_millis()
        .init();
    let cli = Cli::parse();
    let cmd = cli.command.unwrap_or(Cmd::Toggle);
    let ipc_cmd = match cmd.clone() {
        Cmd::Daemon { show } => {
            let mut cmd = std::process::Command::new(daemon_binary());
            if show {
                cmd.arg("--show");
            }
            let err = cmd.exec();
            eprintln!("vela: cannot start vela-daemon: {err}");
            return ExitCode::FAILURE;
        }
        Cmd::Toggle => Command::Toggle,
        Cmd::Show => Command::Show,
        Cmd::Hide => Command::Hide,
        Cmd::Settings { page } => Command::Settings(page.and_then(|p| ipc::SETTINGS_PAGES.iter().copied().find(|x| *x == p))),
        Cmd::Reload => Command::Reload,
        Cmd::Quit => Command::Quit,
        Cmd::Status => Command::Ping,
        Cmd::DefaultConfig => {
            print!("{}", config::Config::default().to_toml());
            return ExitCode::SUCCESS;
        }
    };
    client(ipc_cmd)
}

fn client(cmd: Command) -> ExitCode {
    let socket = paths::socket_path();
    match ipc::send(&socket, cmd) {
        Ok(reply) if reply == "ok" => ExitCode::SUCCESS,
        Ok(reply) => {
            eprintln!("vela: {reply}");
            ExitCode::FAILURE
        }
        Err(SendError::NotRunning) => {
            if matches!(cmd, Command::Hide | Command::Quit | Command::Reload | Command::Ping) {
                if cmd == Command::Ping {
                    eprintln!("vela: daemon is not running");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            match start_daemon() {
                Ok(()) => {
                    let first = if cmd == Command::Toggle { Command::Show } else { cmd };
                    match ipc::send(&socket, first) {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(_) => {
                            eprintln!("vela: daemon started but does not answer");
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(e) => {
                    eprintln!("vela: could not start the daemon: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(SendError::Other(e)) => {
            eprintln!("vela: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Starts the daemon via systemd if the unit is installed, otherwise directly.
fn start_daemon() -> Result<(), String> {
    let unit_installed = ["/usr/lib/systemd/user/vela.service", "/etc/systemd/user/vela.service"]
        .iter()
        .map(std::path::PathBuf::from)
        .chain([
            paths::home_dir().join(".config/systemd/user/vela.service"),
            paths::home_dir().join(".local/share/systemd/user/vela.service"),
        ])
        .any(|p| p.exists());
    let started_by_systemd = unit_installed
        && std::process::Command::new("systemctl")
            .args(["--user", "start", "vela.service"])
            .status()
            .is_ok_and(|s| s.success());
    if !started_by_systemd {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let spec = launch::SpawnSpec {
            argv: vec![exe.to_string_lossy().into_owned(), "daemon".into()],
            name: "vela".into(),
            ..Default::default()
        };
        launch::spawn_detached(&spec, false).map_err(|e| e.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if ipc::send(&paths::socket_path(), Command::Ping).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("timed out waiting for the daemon".into())
}

/// `vela-daemon` next to this binary, or from PATH.
fn daemon_binary() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("vela-daemon")))
        .filter(|p| p.exists())
        .or_else(|| paths::find_executable("vela-daemon"))
        .unwrap_or_else(|| "vela-daemon".into())
}
