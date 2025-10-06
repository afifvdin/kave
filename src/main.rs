use std::collections::HashSet;

use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;

use xkbcommon::xkb::{self, Keysym};

fn get_xkb_code(key: evdev::KeyCode) -> xkbcommon::xkb::Keycode {
    return xkbcommon::xkb::Keycode::new(key.code() as u32 + 8);
}

fn is_modifier_key(key: xkb::Keysym) -> bool {
    match key {
        xkb::Keysym::BackSpace
        | xkb::Keysym::Return
        | xkb::Keysym::Delete
        | xkb::Keysym::Tab
        | xkb::Keysym::Escape => true,
        _ => xkb::Keysym::is_modifier_key(key),
    }
}

fn get_xkb_utf8(key: xkb::Keysym) -> String {
    match key {
        Keysym::space => "␣".to_string(),
        Keysym::Caps_Lock => "⇪".to_string(),
        Keysym::BackSpace => "⌫".to_string(),
        Keysym::Return => "⏎".to_string(),
        Keysym::Escape => "⎋".to_string(),
        Keysym::Tab => "⇥".to_string(),
        Keysym::Delete => "⌦".to_string(),
        Keysym::Insert => "󰏔".to_string(),
        Keysym::Home => "󰋜".to_string(),
        Keysym::End => "󰘵".to_string(),
        Keysym::Page_Up => "󰙪".to_string(),
        Keysym::Page_Down => "󰙩".to_string(),
        Keysym::Shift_L => "⇧".to_string(),
        Keysym::Shift_R => "⇧".to_string(),
        Keysym::Control_L => "⌃".to_string(),
        Keysym::Control_R => "⌃".to_string(),
        Keysym::Alt_L => "⎇".to_string(),
        Keysym::Alt_R => "⎇".to_string(),
        Keysym::Super_L => "⊞".to_string(),
        Keysym::Super_R => "⊞".to_string(),
        Keysym::Print => "⎙".to_string(),
        Keysym::function => "󰘧".to_string(),

        Keysym::Up => "↑".to_string(),
        Keysym::Down => "↓".to_string(),
        Keysym::Left => "←".to_string(),
        Keysym::Right => "→".to_string(),

        Keysym::F1 => "F1".to_string(),
        Keysym::F2 => "F2".to_string(),
        Keysym::F3 => "F3".to_string(),
        Keysym::F4 => "F4".to_string(),
        Keysym::F5 => "F5".to_string(),
        Keysym::F6 => "F6".to_string(),
        Keysym::F7 => "F7".to_string(),
        Keysym::F8 => "F8".to_string(),
        Keysym::F9 => "F9".to_string(),
        Keysym::F10 => "F10".to_string(),
        Keysym::F11 => "F11".to_string(),
        Keysym::F12 => "F12".to_string(),

        _ => xkb::keysym_to_utf8(key),
    }
}

fn show_then_fade(revealer: &gtk::Revealer, label: &gtk::Label) {
    thread_local! {
        static FADE_TIMER: std::cell::RefCell<Option<gtk::glib::SourceId>> = std::cell::RefCell::new(None);
    }

    revealer.set_reveal_child(true);
    FADE_TIMER.with_borrow_mut(|cell| {
        if let Some(id) = cell.take() {
            let _ = id.remove();
        }
    });

    let revealer_weak = revealer.downgrade();
    let label_weak = label.downgrade();
    let id = gtk::glib::timeout_add_local(std::time::Duration::from_millis(700), move || {
        if let Some(rev) = revealer_weak.upgrade() {
            rev.set_reveal_child(false);
        }
        if let Some(span) = label_weak.upgrade() {
            span.set_label("");
        }
        FADE_TIMER.with_borrow_mut(|cell| {
            *cell = None;
        });
        gtk::glib::ControlFlow::Break
    });

    FADE_TIMER.with_borrow_mut(|cell| *cell = Some(id));
}

fn activate(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

    window.add_css_class("window");
    window.init_layer_shell();
    window.set_resizable(false);
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    window.set_decorated(false);
    window.set_overflow(gtk::Overflow::Hidden);
    window.set_halign(gtk::Align::Center);
    window.set_valign(gtk::Align::Center);

    let (sender, receiver): (
        async_channel::Sender<String>,
        async_channel::Receiver<String>,
    ) = async_channel::bounded(1);

    let revealer = gtk::Revealer::new();
    revealer.add_css_class("revealer");
    revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    revealer.set_transition_duration(300);
    revealer.set_reveal_child(false);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.add_css_class("row");
    row.set_halign(gtk::Align::Center);
    row.set_hexpand(false);
    row.set_homogeneous(false);

    let span = gtk::Label::new(Some(""));
    span.add_css_class("span");
    span.set_halign(gtk::Align::Center);
    span.set_justify(gtk::Justification::Center);

    let span_copy = span.clone();
    let revealer_copy = revealer.clone();

    gio::glib::MainContext::default().spawn_local(async move {
        while let Ok(label) = receiver.recv().await {
            span_copy.set_label(&format!("{}", &label.to_uppercase()));
            show_then_fade(&revealer_copy, &span_copy);
        }
    });

    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        &"
        .window{
        font-family: SF Pro Display;
        background-color: transparent;
        }
        .span {
        font-size: 3rem;
        min-width: 3.5rem;
        padding: .5rem 1rem;
        border-radius: 1.5rem;
        margin: 1.5rem;
        background-color: #171717;
        }
        ",
    );

    let display = gtk::gdk::Display::default().unwrap();
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    std::thread::spawn(move || {
        let mut device = evdev::Device::open("/dev/input/event2").unwrap();
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &ctx,
            "evdev",
            "pc105",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("Failed to compile keymap");

        let mut state = xkb::State::new(&keymap);
        let mut modifiers: HashSet<xkbcommon::xkb::Keysym> = HashSet::new();

        loop {
            for event in device.fetch_events().unwrap() {
                match event.destructure() {
                    evdev::EventSummary::Key(_, code, 2) => {
                        let keycode = get_xkb_code(code);
                        state.update_key(keycode, xkb::KeyDirection::Down);

                        let key = state.key_get_one_sym(keycode);
                        let symbol_utf8 = get_xkb_utf8(key);
                        let is_modifier = is_modifier_key(key);

                        if is_modifier {
                            if !modifiers.contains(&key) {
                                modifiers.insert(key);
                            }
                        }

                        let keys = modifiers
                            .iter()
                            .cloned()
                            .map(|k| get_xkb_utf8(k))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if is_modifier {
                            let _ = sender.try_send(keys);
                        } else if modifiers.len() > 0 && !modifiers.contains(&key) {
                            let _ = sender.try_send(format!("{} {}", keys, symbol_utf8));
                        }
                    }
                    evdev::EventSummary::Key(_, code, 1) => {
                        let keycode = get_xkb_code(code);
                        state.update_key(keycode, xkb::KeyDirection::Down);

                        let key = state.key_get_one_sym(keycode);
                        let symbol_utf8 = get_xkb_utf8(key);
                        let is_modifier = is_modifier_key(key);

                        if is_modifier {
                            if !modifiers.contains(&key) {
                                modifiers.insert(key);
                            }
                        }

                        let keys = modifiers
                            .iter()
                            .cloned()
                            .map(|k| get_xkb_utf8(k))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if is_modifier {
                            let _ = sender.try_send(keys);
                        } else if modifiers.len() > 0 && !modifiers.contains(&key) {
                            let _ = sender.try_send(format!("{} {}", keys, symbol_utf8));
                        }
                    }
                    evdev::EventSummary::Key(_, code, 0) => {
                        let keycode = get_xkb_code(code);
                        state.update_key(keycode, xkb::KeyDirection::Up);

                        let key = state.key_get_one_sym(keycode);
                        let is_modifier = is_modifier_key(key);

                        if is_modifier {
                            modifiers.remove(&key);
                        }
                    }
                    _ => {}
                }
            }
        }
    });
    row.append(&span);
    revealer.set_child(Some(&row));
    window.set_child(Some(&revealer));
    window.show();
}

fn main() -> gtk::glib::ExitCode {
    let application = gtk::Application::new(Some("com.afifvdin.kave"), Default::default());
    application.connect_activate(activate);

    return application.run();
}
