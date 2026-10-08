//! macOS backend: Quartz event tap → display engine → AppKit overlay.

mod keycodes;
mod layout;
mod menu;
mod overlay;
mod tap;

use std::path::PathBuf;
use std::process::ExitCode;
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2_foundation::NSTimer;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::config::Config;

pub fn run(config: Config, config_path: Option<PathBuf>, demo: bool) -> ExitCode {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("kave: must run on the main thread");
        return ExitCode::FAILURE;
    };
    let ns_app = NSApplication::sharedApplication(mtm);
    // No Dock icon or app menu; kave lives in the menu bar.
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let mode = config.mode;
    app::start(mtm, config, config_path);
    if demo {
        play_demo(Rc::new(crate::demo::script(mode)), 0);
        ns_app.run();
        return ExitCode::SUCCESS;
    }
    if !tap::has_permission() {
        tap::request_permission();
    }
    if let Err(e) = tap::install(app::on_key) {
        eprintln!("kave: {e}");
        app::explain_permission(mtm);
        return ExitCode::FAILURE;
    }
    ns_app.run();
    ExitCode::SUCCESS
}

/// Feed the demo script to the app, forever.
fn play_demo(steps: Rc<Vec<crate::demo::Step>>, index: usize) {
    let step = &steps[index];
    let event = step.event.clone();
    let delay = step.after_ms as f64 / 1000.0;
    let next = (index + 1) % steps.len();
    let fire = RcBlock::new(move |_: NonNull<NSTimer>| {
        app::on_key(event.clone());
        play_demo(Rc::clone(&steps), next);
    });
    // SAFETY: scheduled on and fired from the main run loop.
    unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(delay, false, &fire) };
}

/// App state shared by the event tap and the menu. Everything runs on the
/// main thread.
mod app {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::process::Command;
    use std::rc::Rc;
    use std::time::Instant;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSApplication, NSWorkspace};
    use objc2_foundation::{NSString, NSURL};

    use super::menu::StatusMenu;
    use super::overlay::Overlay;
    use crate::config::{Config, Mode};
    use crate::display::Engine;
    use crate::key::KeyEvent;

    struct App {
        mtm: MainThreadMarker,
        config_path: Option<PathBuf>,
        engine: Engine,
        overlay: Rc<Overlay>,
        menu: StatusMenu,
        paused: bool,
        debug: bool,
    }

    thread_local! {
        static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    }

    fn with_app(f: impl FnOnce(&mut App)) {
        APP.with_borrow_mut(|app| {
            if let Some(app) = app {
                f(app);
            }
        });
    }

    pub fn start(mtm: MainThreadMarker, config: Config, config_path: Option<PathBuf>) {
        let menu = StatusMenu::new(mtm);
        menu.set_typing(config.mode == Mode::Typing);
        let app = App {
            mtm,
            config_path,
            engine: Engine::new(&config),
            overlay: Overlay::new(mtm, &config),
            menu,
            paused: false,
            debug: std::env::var_os("KAVE_DEBUG").is_some(),
        };
        APP.set(Some(app));
    }

    pub fn on_key(ev: KeyEvent) {
        with_app(|app| {
            if app.paused {
                return;
            }
            let frame = app.engine.handle(&ev, Instant::now());
            if app.debug {
                eprintln!("{ev:?} -> {:?}", frame.as_ref().map(|f| f.to_text()));
            }
            if let Some(frame) = frame {
                app.overlay.show(&frame);
            }
        });
    }

    pub fn toggle_typing() {
        with_app(|app| {
            let mode = match app.engine.mode() {
                Mode::Shortcuts => Mode::Typing,
                Mode::Typing => Mode::Shortcuts,
            };
            app.engine.set_mode(mode);
            app.menu.set_typing(mode == Mode::Typing);
            app.overlay.hide();
        });
    }

    pub fn toggle_pause() {
        with_app(|app| {
            app.paused = !app.paused;
            app.menu.set_paused(app.paused);
            app.overlay.hide();
        });
    }

    pub fn open_config() {
        with_app(|app| {
            let Some(path) = &app.config_path else {
                return alert(
                    app.mtm,
                    "No config location",
                    "Set $KAVE_CONFIG to a file path.",
                );
            };
            if let Err(e) = Config::write_template(path) {
                return alert(app.mtm, "Couldn't create the config file", &e.to_string());
            }
            // -t opens in the default text editor, whatever owns .toml.
            if let Err(e) = Command::new("open").arg("-t").arg(path).spawn() {
                alert(app.mtm, "Couldn't open the config file", &e.to_string());
            }
        });
    }

    pub fn reload_config() {
        with_app(|app| match Config::load(app.config_path.as_ref()) {
            Ok(config) => {
                app.engine = Engine::new(&config);
                app.overlay.apply_config(&config);
                app.overlay.hide();
                app.menu.set_typing(config.mode == Mode::Typing);
            }
            Err(e) => alert(app.mtm, "Couldn't load the config", &e),
        });
    }

    /// Tell the user how to grant Input Monitoring, offering to open the
    /// right System Settings pane.
    pub fn explain_permission(mtm: MainThreadMarker) {
        let open = show_alert(
            mtm,
            "kave needs Input Monitoring",
            "Allow kave (or the terminal you run it from) in System Settings → \
             Privacy & Security → Input Monitoring, then start kave again.",
            &["Open System Settings", "Quit"],
        );
        if open {
            let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent";
            if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
    }

    fn alert(mtm: MainThreadMarker, title: &str, message: &str) {
        show_alert(mtm, title, message, &["OK"]);
    }

    /// Returns whether the first button was chosen.
    fn show_alert(mtm: MainThreadMarker, title: &str, message: &str, buttons: &[&str]) -> bool {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(message));
        for b in buttons {
            alert.addButtonWithTitle(&NSString::from_str(b));
        }
        NSApplication::sharedApplication(mtm).activate();
        alert.runModal() == NSAlertFirstButtonReturn
    }
}
