mod config;
mod demo;
mod display;
mod key;
mod platform;
mod theme;

use std::path::PathBuf;
use std::process::ExitCode;

use config::{Config, Mode};

const HELP: &str = "\
kave — show your keystrokes on screen

Usage: kave [options]

Options:
  -t, --typing        Show everything you type, not just shortcuts
  -s, --shortcuts     Show shortcuts and special keys only (default)
  -c, --config PATH   Read config from PATH (default: ~/.config/kave/config.toml)
      --demo          Play example keystrokes to preview the look
      --init-config   Write a documented config file and exit
  -h, --help          Show this help
  -V, --version       Show the version
";

fn main() -> ExitCode {
    let mut mode = None;
    let mut path = Config::path();
    let mut init = false;
    let mut demo = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-t" | "--typing" => mode = Some(Mode::Typing),
            "-s" | "--shortcuts" => mode = Some(Mode::Shortcuts),
            "-c" | "--config" => match args.next() {
                Some(p) => path = Some(PathBuf::from(p)),
                None => return usage_error("--config needs a path"),
            },
            "--init-config" => init = true,
            "--demo" => demo = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("kave {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => return usage_error(&format!("unknown option {other:?}")),
        }
    }

    if init {
        let Some(path) = path else {
            return usage_error("no config location; pass --config PATH");
        };
        if path.exists() {
            eprintln!("kave: {} already exists", path.display());
            return ExitCode::FAILURE;
        }
        return match Config::write_template(&path) {
            Ok(()) => {
                println!("Wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kave: {}: {e}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let mut config = match Config::load(path.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("kave: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(mode) = mode {
        config.mode = mode;
    }
    platform::run(config, path, demo)
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("kave: {msg}\n\n{HELP}");
    ExitCode::from(2)
}
