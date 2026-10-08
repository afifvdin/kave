//! macOS virtual key codes (`kVK_*` from Carbon's `Events.h`).

use crate::key::{Key, Modifier};

/// Keys whose meaning doesn't depend on the keyboard layout.
pub fn fixed_key(code: u16) -> Option<Key> {
    Some(match code {
        0x24 => Key::Return,
        0x30 => Key::Tab,
        0x31 => Key::Space,
        0x33 => Key::Backspace,
        0x35 => Key::Escape,
        0x39 => Key::CapsLock,
        0x47 => Key::Clear,
        0x4C => Key::Enter,
        0x72 => Key::Help,
        0x73 => Key::Home,
        0x74 => Key::PageUp,
        0x75 => Key::Delete,
        0x77 => Key::End,
        0x79 => Key::PageDown,
        0x7B => Key::Left,
        0x7C => Key::Right,
        0x7D => Key::Down,
        0x7E => Key::Up,
        0x7A => Key::F(1),
        0x78 => Key::F(2),
        0x63 => Key::F(3),
        0x76 => Key::F(4),
        0x60 => Key::F(5),
        0x61 => Key::F(6),
        0x62 => Key::F(7),
        0x64 => Key::F(8),
        0x65 => Key::F(9),
        0x6D => Key::F(10),
        0x67 => Key::F(11),
        0x6F => Key::F(12),
        0x69 => Key::F(13),
        0x6B => Key::F(14),
        0x71 => Key::F(15),
        0x6A => Key::F(16),
        0x40 => Key::F(17),
        0x4F => Key::F(18),
        0x50 => Key::F(19),
        0x5A => Key::F(20),
        _ => return None,
    })
}

/// The modifier a key code belongs to, and its device-dependent flag bit
/// (`NX_DEVICE*KEYMASK`), which tells left and right keys apart. `fn` has no
/// such bit.
pub fn modifier_key(code: u16) -> Option<(Modifier, u64)> {
    Some(match code {
        0x3B => (Modifier::Control, 0x0000_0001),
        0x3E => (Modifier::Control, 0x0000_2000),
        0x38 => (Modifier::Shift, 0x0000_0002),
        0x3C => (Modifier::Shift, 0x0000_0004),
        0x37 => (Modifier::Command, 0x0000_0008),
        0x36 => (Modifier::Command, 0x0000_0010),
        0x3A => (Modifier::Option, 0x0000_0020),
        0x3D => (Modifier::Option, 0x0000_0040),
        // kVK_Function, and the globe key on newer keyboards.
        0x3F | 0xB3 => (Modifier::Fn, 0),
        _ => return None,
    })
}
