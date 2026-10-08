//! Platform-independent keyboard model.
//!
//! Every platform backend translates its native events into [`KeyEvent`]s, so
//! the rest of kave never has to know about scan codes or OS APIs.

/// A modifier key. Left and right variants are not distinguished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Modifier {
    Fn,
    Control,
    Option,
    Shift,
    Command,
}

impl Modifier {
    /// All modifiers in the order they are displayed (Apple's ordering).
    pub const ALL: [Modifier; 5] = [
        Modifier::Fn,
        Modifier::Control,
        Modifier::Option,
        Modifier::Shift,
        Modifier::Command,
    ];

    fn bit(self) -> u8 {
        match self {
            Modifier::Fn => 1 << 0,
            Modifier::Control => 1 << 1,
            Modifier::Option => 1 << 2,
            Modifier::Shift => 1 << 3,
            Modifier::Command => 1 << 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Modifier::Fn => "fn",
            Modifier::Control => "control",
            Modifier::Option => "option",
            Modifier::Shift => "shift",
            Modifier::Command => "command",
        }
    }
}

/// A set of held modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const fn empty() -> Self {
        Modifiers(0)
    }

    pub fn contains(self, m: Modifier) -> bool {
        self.0 & m.bit() != 0
    }

    pub fn insert(&mut self, m: Modifier) {
        self.0 |= m.bit();
    }

    pub fn remove(&mut self, m: Modifier) {
        self.0 &= !m.bit();
    }

    #[cfg(test)]
    pub fn with(mut self, m: Modifier) -> Self {
        self.insert(m);
        self
    }

    /// Whether any modifier other than Shift is held. Shift on its own is
    /// part of ordinary typing, so it doesn't make a keystroke a "shortcut".
    pub fn is_shortcut(self) -> bool {
        Modifier::ALL
            .iter()
            .any(|&m| m != Modifier::Shift && self.contains(m))
    }

    /// Held modifiers in display order.
    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |&m| self.contains(m))
    }
}

/// A key, resolved through the active keyboard layout where that matters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key that produces a character. Holds the character the active
    /// layout produces without any modifiers, e.g. `"a"` or `"/"`.
    Char(String),
    Space,
    Return,
    Enter,
    Tab,
    Backspace,
    Delete,
    Escape,
    CapsLock,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Help,
    Clear,
    F(u8),
    Modifier(Modifier),
    /// A key we have no name for, with its native key code.
    Unknown(u32),
}

impl Key {
    /// Stable lowercase name, used as the lookup key for replacements in the
    /// config file.
    pub fn name(&self) -> String {
        match self {
            Key::Char(c) => c.to_lowercase(),
            Key::Space => "space".into(),
            Key::Return => "return".into(),
            Key::Enter => "enter".into(),
            Key::Tab => "tab".into(),
            Key::Backspace => "backspace".into(),
            Key::Delete => "delete".into(),
            Key::Escape => "escape".into(),
            Key::CapsLock => "capslock".into(),
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Left => "left".into(),
            Key::Right => "right".into(),
            Key::Home => "home".into(),
            Key::End => "end".into(),
            Key::PageUp => "pageup".into(),
            Key::PageDown => "pagedown".into(),
            Key::Help => "help".into(),
            Key::Clear => "clear".into(),
            Key::F(n) => format!("f{n}"),
            Key::Modifier(m) => m.name().into(),
            Key::Unknown(code) => format!("key{code}"),
        }
    }

    /// Keys worth showing even without a modifier: editing, navigation and
    /// function keys, as opposed to keys that type text.
    pub fn is_special(&self) -> bool {
        !matches!(
            self,
            Key::Char(_) | Key::Space | Key::Modifier(_) | Key::Unknown(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press,
    /// Auto-repeat while the key is held down.
    Repeat,
    Release,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub action: Action,
    /// Modifiers held at the time of the event. For a modifier key press this
    /// includes the modifier itself.
    pub modifiers: Modifiers,
    /// Text the keystroke produced, if any (layout, Shift, Option and Caps
    /// Lock applied).
    pub text: Option<String>,
}
