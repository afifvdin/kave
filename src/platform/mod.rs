//! Platform backends. Each one captures global key events, feeds them to
//! [`crate::display::Engine`] and draws the result in an overlay.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::run;

#[cfg(not(target_os = "macos"))]
pub fn run(
    _config: crate::config::Config,
    _path: Option<std::path::PathBuf>,
) -> std::process::ExitCode {
    eprintln!("kave: this platform isn't supported yet (macOS only for now)");
    std::process::ExitCode::FAILURE
}
