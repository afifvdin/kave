//! Global keyboard capture with a listen-only Quartz event tap.
//!
//! The tap's run loop source is added to the main run loop, so the callback
//! runs on the main thread alongside AppKit and can update the UI directly.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, CGPreflightListenEventAccess,
    CGRequestListenEventAccess,
};

use super::{keycodes, layout};
use crate::key::{Action, Key, KeyEvent, Modifier, Modifiers};

type Handler = Box<dyn FnMut(KeyEvent)>;

thread_local! {
    static TAP: RefCell<Option<CFRetained<CFMachPort>>> = const { RefCell::new(None) };
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    /// `fn` has no reliable flag: arrow and function keys set it too.
    static FN_HELD: Cell<bool> = const { Cell::new(false) };
}

/// Whether kave has the Input Monitoring permission.
pub fn has_permission() -> bool {
    CGPreflightListenEventAccess()
}

/// Show the system prompt for Input Monitoring (only the first time).
pub fn request_permission() {
    CGRequestListenEventAccess();
}

/// Start delivering key events to `handler`. Fails without permission.
pub fn install(handler: impl FnMut(KeyEvent) + 'static) -> Result<(), &'static str> {
    let mask: CGEventMask = [
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ]
    .iter()
    .fold(0, |m, t| m | 1 << t.0);

    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            mask,
            Some(callback),
            ptr::null_mut(),
        )
    }
    .ok_or("could not create the event tap")?;
    let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0)
        .ok_or("could not create a run loop source for the event tap")?;
    let run_loop = CFRunLoop::main().ok_or("no main run loop")?;
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&tap, true);

    HANDLER.set(Some(Box::new(handler)));
    TAP.set(Some(tap));
    Ok(())
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    ty: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
        TAP.with_borrow(|tap| {
            if let Some(tap) = tap {
                CGEvent::tap_enable(tap, true);
            }
        });
    } else if let Some(ev) = translate(ty, unsafe { event.as_ref() }) {
        HANDLER.with_borrow_mut(|h| {
            if let Some(h) = h {
                h(ev);
            }
        });
    }
    // Listen-only taps can't modify events; the return value is ignored.
    event.as_ptr()
}

fn translate(ty: CGEventType, event: &CGEvent) -> Option<KeyEvent> {
    let code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode) as u16;
    let flags = CGEvent::flags(Some(event));

    if ty == CGEventType::FlagsChanged {
        return modifier_event(code, flags);
    }
    let action = if ty == CGEventType::KeyUp {
        Action::Release
    } else if CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventAutorepeat) != 0
    {
        Action::Repeat
    } else if ty == CGEventType::KeyDown {
        Action::Press
    } else {
        return None;
    };
    let key = keycodes::fixed_key(code)
        .or_else(|| layout::base_char(code).map(Key::Char))
        .unwrap_or(Key::Unknown(code as u32));
    Some(KeyEvent {
        key,
        action,
        modifiers: modifiers(flags),
        text: (action != Action::Release)
            .then(|| typed_text(event))
            .flatten(),
    })
}

/// Modifier keys arrive as `FlagsChanged`; work out which key moved and in
/// which direction from the flags.
fn modifier_event(code: u16, flags: CGEventFlags) -> Option<KeyEvent> {
    let (key, down) = if code == 0x39 {
        // Caps Lock reports once per toggle, not per press and release.
        (Key::CapsLock, true)
    } else {
        let (modifier, device_bit) = keycodes::modifier_key(code)?;
        let down = if modifier == Modifier::Fn {
            let down = flags.contains(CGEventFlags::MaskSecondaryFn);
            FN_HELD.set(down);
            down
        } else {
            flags.0 & device_bit != 0
        };
        (Key::Modifier(modifier), down)
    };
    Some(KeyEvent {
        key,
        action: if down { Action::Press } else { Action::Release },
        modifiers: modifiers(flags),
        text: None,
    })
}

fn modifiers(flags: CGEventFlags) -> Modifiers {
    let mut m = Modifiers::empty();
    for (flag, modifier) in [
        (CGEventFlags::MaskControl, Modifier::Control),
        (CGEventFlags::MaskAlternate, Modifier::Option),
        (CGEventFlags::MaskShift, Modifier::Shift),
        (CGEventFlags::MaskCommand, Modifier::Command),
    ] {
        if flags.contains(flag) {
            m.insert(modifier);
        }
    }
    if FN_HELD.get() {
        m.insert(Modifier::Fn);
    }
    m
}

fn typed_text(event: &CGEvent) -> Option<String> {
    let mut buf = [0u16; 8];
    let mut len = 0;
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(event),
            buf.len() as _,
            &mut len,
            buf.as_mut_ptr(),
        )
    };
    let s = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]);
    (!s.is_empty()).then_some(s)
}
