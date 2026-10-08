//! Resolve key codes to characters through the active keyboard layout, so
//! shortcuts show `⌘ Z` on QWERTY and `⌘ Y` on QWERTZ for the same key.

use std::ffi::c_void;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, key: *const c_void) -> *const c_void;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_len: usize,
        actual_len: *mut usize,
        chars: *mut u16,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
}

const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1;

/// The character `code` produces with no modifiers held. Must be called on
/// the main thread (Text Input Sources requires it).
pub fn base_char(code: u16) -> Option<String> {
    // Input methods (e.g. Japanese) may have no Unicode layout data; fall
    // back to the ASCII-capable layout they type through.
    unsafe {
        translate(TISCopyCurrentKeyboardLayoutInputSource(), code)
            .or_else(|| translate(TISCopyCurrentASCIICapableKeyboardLayoutInputSource(), code))
    }
}

/// Consumes (releases) `source`.
unsafe fn translate(source: *mut c_void, code: u16) -> Option<String> {
    if source.is_null() {
        return None;
    }
    let result = unsafe {
        let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            None
        } else {
            let mut dead_keys = 0u32;
            let mut buf = [0u16; 8];
            let mut len = 0usize;
            let status = UCKeyTranslate(
                CFDataGetBytePtr(data).cast(),
                code,
                K_UC_KEY_ACTION_DISPLAY,
                0,
                LMGetKbdType() as u32,
                K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
                &mut dead_keys,
                buf.len(),
                &mut len,
                buf.as_mut_ptr(),
            );
            (status == 0).then(|| String::from_utf16_lossy(&buf[..len]))
        }
    };
    unsafe { CFRelease(source) };
    result.filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
}
