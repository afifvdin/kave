//! The menu bar item.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSString;

use super::app;

define_class!(
    /// Receives menu actions and forwards them to the app.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KaveMenuTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(toggleTyping:))]
        fn toggle_typing(&self, _sender: Option<&AnyObject>) {
            app::toggle_typing();
        }

        #[unsafe(method(togglePause:))]
        fn toggle_pause(&self, _sender: Option<&AnyObject>) {
            app::toggle_pause();
        }

        #[unsafe(method(openConfig:))]
        fn open_config(&self, _sender: Option<&AnyObject>) {
            app::open_config();
        }

        #[unsafe(method(reloadConfig:))]
        fn reload_config(&self, _sender: Option<&AnyObject>) {
            app::reload_config();
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

pub struct StatusMenu {
    _item: Retained<NSStatusItem>,
    _target: Retained<Target>,
    typing: Retained<NSMenuItem>,
    pause: Retained<NSMenuItem>,
}

impl StatusMenu {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("keyboard"),
                Some(&NSString::from_str("kave")),
            );
            match image {
                Some(image) => {
                    image.setTemplate(true);
                    button.setImage(Some(&image));
                }
                None => button.setTitle(&NSString::from_str("kave")),
            }
        }

        let target = Target::new(mtm);
        let menu = NSMenu::new(mtm);
        let add = |title: &str, action: Option<Sel>, key: &str| {
            let entry = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    action,
                    &NSString::from_str(key),
                )
            };
            // `terminate:` has no target, so it goes up the responder chain
            // to NSApplication.
            if action != Some(sel!(terminate:)) {
                unsafe { entry.setTarget(Some(&target)) };
            }
            menu.addItem(&entry);
            entry
        };

        let typing = add("Typing Mode", Some(sel!(toggleTyping:)), "t");
        let pause = add("Pause", Some(sel!(togglePause:)), "p");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add("Open Config…", Some(sel!(openConfig:)), ",");
        add("Reload Config", Some(sel!(reloadConfig:)), "r");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add("Quit kave", Some(sel!(terminate:)), "q");
        item.setMenu(Some(&menu));

        StatusMenu {
            _item: item,
            _target: target,
            typing,
            pause,
        }
    }

    pub fn set_typing(&self, on: bool) {
        self.typing.setState(state(on));
    }

    pub fn set_paused(&self, on: bool) {
        self.pause.setState(state(on));
    }
}

fn state(on: bool) -> isize {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}
