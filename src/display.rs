//! Turns key events into what's shown on screen: a row of keycaps, and in
//! typing mode the text in between.
//!
//! The engine is pure: platforms feed it [`KeyEvent`]s with a timestamp and
//! draw whatever [`Frame`] it returns.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::config::{Config, Mode};
use crate::key::{Action, Key, KeyEvent, Modifier, Modifiers};

/// Everything currently on screen, left to right.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Stable while the item is on screen, so renderers can animate changes
    /// to it instead of redrawing it.
    pub id: u64,
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// Typed text (typing mode only).
    Text { text: String, clipped: bool },
    /// Keys pressed together, and how many times in a row.
    Combo { caps: Vec<Cap>, count: u32 },
}

/// One keycap, labelled the way Apple keyboards do: a main legend and, for
/// modifiers and named keys, a small word.
#[derive(Clone, Debug, PartialEq)]
pub struct Cap {
    /// Unique within its combo; stays the same across frames.
    pub id: String,
    /// Main legend: `K`, `⌘`, `↩`. Empty for the space bar.
    pub legend: String,
    /// Small word under the legend: `command`, `return`.
    pub name: Option<String>,
    /// Width in key units (1 = a letter key).
    pub width: f64,
    /// Drawn in the accent color, like an artisan keycap.
    pub accent: bool,
    /// The key is held down right now.
    pub held: bool,
}

impl Frame {
    /// Plain-text rendering, for logs and tests.
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        for item in &self.items {
            parts.push(match &item.kind {
                ItemKind::Text { text, clipped } => {
                    format!("{}{text}", if *clipped { "…" } else { "" })
                }
                ItemKind::Combo { caps, count } => {
                    let keys: Vec<&str> = caps
                        .iter()
                        .map(|c| match (c.legend.as_str(), &c.name) {
                            ("", Some(name)) => name.as_str(),
                            (legend, _) => legend,
                        })
                        .collect();
                    let keys = keys.join(" ");
                    if *count > 1 {
                        format!("{keys} ×{count}")
                    } else {
                        keys
                    }
                }
            });
        }
        parts.join(" ")
    }
}

#[derive(Clone, Debug)]
enum Chunk {
    Text {
        id: u64,
        text: String,
        clipped: bool,
    },
    Combo {
        id: u64,
        keys: Vec<Key>,
        count: u32,
    },
}

impl Chunk {
    fn combo_modifiers(&self) -> Option<Modifiers> {
        match self {
            Chunk::Combo { keys, .. } => Some(keys.iter().fold(Modifiers::empty(), |mut m, k| {
                if let Key::Modifier(x) = k {
                    m.insert(*x);
                }
                m
            })),
            Chunk::Text { .. } => None,
        }
    }

    /// A combo of modifiers only, shown while a shortcut is being formed.
    fn is_preview(&self) -> bool {
        matches!(self, Chunk::Combo { keys, .. } if keys.iter().all(|k| matches!(k, Key::Modifier(_))))
    }
}

pub struct Engine {
    mode: Mode,
    max_chars: usize,
    hide_after: Duration,
    replacements: HashMap<String, String>,
    chunks: Vec<Chunk>,
    next_id: u64,
    last_shown: Option<Instant>,
    held: HashSet<Key>,
    modifiers: Modifiers,
}

impl Engine {
    pub fn new(config: &Config) -> Self {
        Engine {
            mode: config.mode,
            max_chars: config.max_chars.max(1),
            hide_after: Duration::from_millis(config.hide_after_ms),
            replacements: config.replacements.clone(),
            chunks: Vec::new(),
            next_id: 0,
            last_shown: None,
            held: HashSet::new(),
            modifiers: Modifiers::empty(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.chunks.clear();
    }

    /// Handle an event. Returns the new frame to show, or `None` to leave
    /// the display as it is.
    pub fn handle(&mut self, ev: &KeyEvent, now: Instant) -> Option<Frame> {
        self.modifiers = ev.modifiers;
        if !matches!(ev.key, Key::Modifier(_) | Key::CapsLock) {
            match ev.action {
                Action::Press | Action::Repeat => self.held.insert(ev.key.clone()),
                Action::Release => self.held.remove(&ev.key),
            };
        }
        // Once the previous keys have faded out, start from scratch.
        if self
            .last_shown
            .is_none_or(|t| now.duration_since(t) > self.hide_after)
        {
            self.chunks.clear();
        }

        let changed = match (ev.action, self.mode) {
            (Action::Release, _) => false,
            (_, Mode::Shortcuts) => self.shortcut(ev),
            (_, Mode::Typing) => self.typing(ev),
        };
        // Also redraw when a visible key goes up or down.
        if !changed && !self.shows(&ev.key) {
            return None;
        }
        self.last_shown = Some(now);
        Some(self.frame())
    }

    fn shows(&self, key: &Key) -> bool {
        self.chunks
            .iter()
            .any(|c| matches!(c, Chunk::Combo { keys, .. } if keys.contains(key)))
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn shortcut(&mut self, ev: &KeyEvent) -> bool {
        let mods = ev.modifiers;
        let last = self.chunks.last();
        let last_mods = last.and_then(Chunk::combo_modifiers);
        let preview = last.is_some_and(Chunk::is_preview);
        if let Key::Modifier(_) = ev.key {
            if ev.action != Action::Press || !mods.is_shortcut() {
                return false;
            }
            // Pressing the modifiers of the shortcut on screen again just
            // pushes those keycaps down.
            if last_mods == Some(mods) {
                return false;
            }
            let keys = modifier_keys(mods);
            match self.chunks.last_mut() {
                // Grow the preview, keeping its keycaps on screen.
                Some(Chunk::Combo { keys: k, .. }) if preview => *k = keys,
                _ => {
                    let id = self.new_id();
                    self.chunks = vec![Chunk::Combo { id, keys, count: 1 }];
                }
            }
            return true;
        }

        if !mods.is_shortcut() && !ev.key.is_special() {
            return false;
        }
        let mut keys = modifier_keys(mods);
        keys.push(ev.key.clone());
        match self.chunks.last_mut() {
            Some(Chunk::Combo { keys: k, count, .. }) if *k == keys => *count += 1,
            // The shortcut the preview was building up to.
            Some(Chunk::Combo { keys: k, count, .. }) if preview => {
                *k = keys;
                *count = 1;
            }
            _ => {
                let id = self.new_id();
                self.chunks = vec![Chunk::Combo { id, keys, count: 1 }];
            }
        }
        true
    }

    fn typing(&mut self, ev: &KeyEvent) -> bool {
        if let Key::Modifier(_) = ev.key {
            return false;
        }
        let text = self.typed_text(ev);
        let mods = ev.modifiers;
        let is_combo = ev.key.is_special()
            || ev.key == Key::Space && mods.is_shortcut()
            || mods.contains(Modifier::Command)
            || mods.contains(Modifier::Control)
            || mods.contains(Modifier::Fn)
            || text.is_none();

        if is_combo {
            let mut keys = modifier_keys(mods);
            keys.push(ev.key.clone());
            match self.chunks.last_mut() {
                Some(Chunk::Combo { keys: k, count, .. }) if *k == keys => *count += 1,
                _ => {
                    let id = self.new_id();
                    self.chunks.push(Chunk::Combo { id, keys, count: 1 });
                }
            }
        } else {
            let text = text.unwrap_or_default();
            match self.chunks.last_mut() {
                Some(Chunk::Text { text: t, .. }) => t.push_str(&text),
                _ => {
                    let id = self.new_id();
                    self.chunks.push(Chunk::Text {
                        id,
                        text,
                        clipped: false,
                    });
                }
            }
        }
        self.prune();
        true
    }

    /// The printable text a keystroke typed, with replacements applied.
    fn typed_text(&self, ev: &KeyEvent) -> Option<String> {
        if ev.key == Key::Space {
            return Some(" ".into());
        }
        let text = ev
            .text
            .as_deref()
            .filter(|t| !t.is_empty() && !t.chars().any(char::is_control))?;
        Some(
            self.replacements
                .get(text)
                .cloned()
                .unwrap_or_else(|| text.to_string()),
        )
    }

    /// Rough on-screen width of a chunk, in characters.
    fn weight(&self, chunk: &Chunk) -> usize {
        match chunk {
            Chunk::Text { text, .. } => text.chars().count(),
            Chunk::Combo { keys, .. } => keys
                .iter()
                .map(|k| (self.cap(k).width * 2.0).ceil() as usize)
                .sum::<usize>(),
        }
    }

    /// Drop or trim what has scrolled out of view.
    fn prune(&mut self) {
        loop {
            let total: usize = self.chunks.iter().map(|c| self.weight(c)).sum();
            if total <= self.max_chars {
                return;
            }
            let len = self.chunks.len();
            match self.chunks.first_mut() {
                Some(Chunk::Text { text, clipped, .. }) if len == 1 => {
                    let len = text.chars().count();
                    *text = text.chars().skip(len - self.max_chars).collect();
                    *clipped = true;
                    return;
                }
                Some(_) if len > 1 => {
                    self.chunks.remove(0);
                }
                _ => return,
            }
        }
    }

    fn frame(&self) -> Frame {
        let items = self
            .chunks
            .iter()
            .map(|chunk| match chunk {
                Chunk::Text { id, text, clipped } => Item {
                    id: *id,
                    kind: ItemKind::Text {
                        text: text.clone(),
                        clipped: *clipped,
                    },
                },
                Chunk::Combo { id, keys, count } => Item {
                    id: *id,
                    kind: ItemKind::Combo {
                        caps: keys.iter().map(|k| self.cap(k)).collect(),
                        count: *count,
                    },
                },
            })
            .collect();
        Frame { items }
    }

    fn cap(&self, key: &Key) -> Cap {
        let mut cap = default_cap(key);
        cap.held = match key {
            Key::Modifier(m) => self.modifiers.contains(*m),
            _ => self.held.contains(key),
        };
        if let Some(r) = self.replacements.get(&key.name()) {
            cap.legend = r.clone();
            cap.name = None;
        } else if let Some(r) = self.replacements.get(&cap.legend) {
            cap.legend = r.clone();
        }
        cap
    }
}

fn modifier_keys(mods: Modifiers) -> Vec<Key> {
    mods.iter().map(Key::Modifier).collect()
}

/// The built-in keycap for a key, before replacements.
pub fn default_cap(key: &Key) -> Cap {
    let (legend, name, width, accent): (String, Option<&str>, f64, bool) = match key {
        Key::Char(c) => (c.to_uppercase(), None, 1.0, false),
        Key::F(n) => (format!("F{n}"), None, 1.0, false),
        Key::Unknown(code) => (format!("<{code}>"), None, 1.0, false),
        Key::Modifier(m) => {
            let (legend, name) = modifier_legend(*m);
            let width = match m {
                Modifier::Fn => 1.0,
                Modifier::Shift => 1.75,
                _ => 1.5,
            };
            (legend.into(), name, width, false)
        }
        Key::Space => (String::new(), Some("space"), 3.5, false),
        Key::Return => ("↩".into(), Some("return"), 1.75, true),
        Key::Enter => ("⌤".into(), Some("enter"), 1.5, true),
        Key::Escape => (String::new(), Some("esc"), 1.25, true),
        Key::Tab => ("⇥".into(), Some("tab"), 1.5, false),
        Key::Backspace => ("⌫".into(), Some("delete"), 1.5, false),
        Key::Delete => ("⌦".into(), Some("del"), 1.25, false),
        Key::CapsLock => ("⇪".into(), Some("caps lock"), 1.75, false),
        Key::Up => ("↑".into(), None, 1.0, false),
        Key::Down => ("↓".into(), None, 1.0, false),
        Key::Left => ("←".into(), None, 1.0, false),
        Key::Right => ("→".into(), None, 1.0, false),
        Key::Home => ("↖".into(), Some("home"), 1.25, false),
        Key::End => ("↘".into(), Some("end"), 1.25, false),
        Key::PageUp => ("⇞".into(), Some("page up"), 1.5, false),
        Key::PageDown => ("⇟".into(), Some("page down"), 1.5, false),
        Key::Help => (String::new(), Some("help"), 1.25, false),
        Key::Clear => ("⌧".into(), Some("clear"), 1.25, false),
    };
    Cap {
        id: key.name(),
        legend,
        name: name.map(String::from),
        width,
        accent,
        held: false,
    }
}

#[cfg(target_os = "macos")]
fn modifier_legend(m: Modifier) -> (&'static str, Option<&'static str>) {
    match m {
        Modifier::Fn => ("fn", None),
        Modifier::Control => ("⌃", Some("control")),
        Modifier::Option => ("⌥", Some("option")),
        Modifier::Shift => ("⇧", Some("shift")),
        Modifier::Command => ("⌘", Some("command")),
    }
}

#[cfg(not(target_os = "macos"))]
fn modifier_legend(m: Modifier) -> (&'static str, Option<&'static str>) {
    match m {
        Modifier::Fn => ("Fn", None),
        Modifier::Control => ("Ctrl", None),
        Modifier::Option => ("Alt", None),
        Modifier::Shift => ("⇧", Some("shift")),
        Modifier::Command if cfg!(windows) => ("⊞", Some("win")),
        Modifier::Command => ("◆", Some("super")),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn ev(key: Key, action: Action, mods: &[Modifier], text: Option<&str>) -> KeyEvent {
        KeyEvent {
            key,
            action,
            modifiers: mods.iter().fold(Modifiers::empty(), |m, &x| m.with(x)),
            text: text.map(String::from),
        }
    }

    fn press(key: Key, mods: &[Modifier], text: Option<&str>) -> KeyEvent {
        ev(key, Action::Press, mods, text)
    }

    fn ch(c: &str, mods: &[Modifier]) -> KeyEvent {
        let shifted = mods.contains(&Modifier::Shift);
        let text = if shifted {
            c.to_uppercase()
        } else {
            c.to_string()
        };
        press(Key::Char(c.into()), mods, Some(&text))
    }

    fn engine(mode: Mode) -> Engine {
        Engine::new(&Config {
            mode,
            max_chars: 12,
            ..Config::default()
        })
    }

    fn show(e: &mut Engine, ev: &KeyEvent, t: Instant) -> Option<String> {
        e.handle(ev, t).map(|f| f.to_text())
    }

    use Modifier::{Command, Shift};

    #[test]
    fn shortcuts_show_combos_only() {
        let mut e = engine(Mode::Shortcuts);
        let t = Instant::now();
        assert_eq!(show(&mut e, &ch("a", &[]), t), None);
        assert_eq!(show(&mut e, &ch("a", &[Shift]), t), None);
        assert_eq!(show(&mut e, &press(Key::Space, &[], Some(" ")), t), None);
        assert_eq!(
            show(&mut e, &ch("k", &[Shift, Command]), t).as_deref(),
            Some("⇧ ⌘ K")
        );
        assert_eq!(
            show(&mut e, &press(Key::Escape, &[], None), t).as_deref(),
            Some("esc")
        );
        assert_eq!(
            show(&mut e, &press(Key::Tab, &[Shift], None), t).as_deref(),
            Some("⇧ ⇥")
        );
    }

    #[test]
    fn modifier_preview_grows_into_the_shortcut() {
        let mut e = engine(Mode::Shortcuts);
        let t = Instant::now();
        assert_eq!(
            show(&mut e, &press(Key::Modifier(Shift), &[Shift], None), t),
            None
        );
        let cmd = e
            .handle(&press(Key::Modifier(Command), &[Shift, Command], None), t)
            .unwrap();
        assert_eq!(cmd.to_text(), "⇧ ⌘");
        let full = e.handle(&ch("k", &[Shift, Command]), t).unwrap();
        assert_eq!(full.to_text(), "⇧ ⌘ K");
        // Same item, so the modifier caps stay on screen.
        assert_eq!(cmd.items[0].id, full.items[0].id);
    }

    #[test]
    fn caps_follow_held_keys() {
        let mut e = engine(Mode::Shortcuts);
        let t = Instant::now();
        let held = |f: &Frame| match &f.items[0].kind {
            ItemKind::Combo { caps, .. } => caps.iter().map(|c| c.held).collect::<Vec<_>>(),
            _ => panic!(),
        };
        let f = e.handle(&ch("z", &[Command]), t).unwrap();
        assert_eq!(held(&f), [true, true]);
        let f = e
            .handle(
                &ev(Key::Char("z".into()), Action::Release, &[Command], None),
                t,
            )
            .unwrap();
        assert_eq!(held(&f), [true, false]);
        let f = e
            .handle(&ev(Key::Modifier(Command), Action::Release, &[], None), t)
            .unwrap();
        assert_eq!(held(&f), [false, false]);
        // Releasing a key that isn't shown changes nothing.
        assert!(
            e.handle(&ev(Key::Char("q".into()), Action::Release, &[], None), t)
                .is_none()
        );
    }

    #[test]
    fn repeated_combos_are_counted_until_they_expire() {
        let mut e = engine(Mode::Shortcuts);
        let t = Instant::now();
        let undo = ch("z", &[Command]);
        assert_eq!(show(&mut e, &undo, t).as_deref(), Some("⌘ Z"));
        assert_eq!(show(&mut e, &undo, t).as_deref(), Some("⌘ Z ×2"));
        // Re-pressing ⌘ for the next undo doesn't reset the count.
        let cmd = press(Key::Modifier(Command), &[Command], None);
        assert_eq!(show(&mut e, &cmd, t).as_deref(), Some("⌘ Z ×2"));
        assert_eq!(show(&mut e, &undo, t).as_deref(), Some("⌘ Z ×3"));
        assert_eq!(
            show(&mut e, &ch("c", &[Command]), t).as_deref(),
            Some("⌘ C")
        );
        let later = t + Duration::from_secs(5);
        assert_eq!(show(&mut e, &undo, later).as_deref(), Some("⌘ Z"));
    }

    #[test]
    fn typing_accumulates_text_and_combos() {
        let mut e = engine(Mode::Typing);
        let t = Instant::now();
        show(&mut e, &ch("h", &[Shift]), t);
        show(&mut e, &ch("i", &[]), t);
        assert_eq!(
            show(&mut e, &press(Key::Space, &[], Some(" ")), t).as_deref(),
            Some("Hi ")
        );
        show(&mut e, &press(Key::Backspace, &[], None), t);
        assert_eq!(
            show(&mut e, &press(Key::Backspace, &[], None), t).as_deref(),
            Some("Hi  ⌫ ×2")
        );
    }

    #[test]
    fn typing_uses_option_text() {
        let mut e = engine(Mode::Typing);
        let t = Instant::now();
        let ev = press(Key::Char("e".into()), &[Modifier::Option], Some("é"));
        assert_eq!(show(&mut e, &ev, t).as_deref(), Some("é"));
        let ev = press(Key::Left, &[Modifier::Option], None);
        assert_eq!(show(&mut e, &ev, t).as_deref(), Some("é ⌥ ←"));
    }

    #[test]
    fn typing_scrolls_long_lines() {
        let mut e = engine(Mode::Typing);
        let t = Instant::now();
        let mut last = None;
        for c in "abcdefghijklmnopqrstuvwxyz".chars() {
            last = show(&mut e, &ch(&c.to_string(), &[]), t);
        }
        assert_eq!(last.as_deref(), Some("…opqrstuvwxyz"));
        // A wide key pushes the old text out entirely.
        show(&mut e, &press(Key::Return, &[], None), t);
        assert_eq!(show(&mut e, &ch("a", &[]), t).as_deref(), Some("↩ a"));
    }

    #[test]
    fn caps_have_names_and_accents() {
        let ret = default_cap(&Key::Return);
        assert_eq!(
            (ret.legend.as_str(), ret.name.as_deref(), ret.accent),
            ("↩", Some("return"), true)
        );
        let k = default_cap(&Key::Char("k".into()));
        assert_eq!((k.legend.as_str(), k.name, k.width), ("K", None, 1.0));
    }

    #[test]
    fn replacements_by_name_and_symbol() {
        let mut config = Config::default();
        config.replacements.insert("command".into(), "Cmd".into());
        config.replacements.insert("↩".into(), "Enter".into());
        let mut e = Engine::new(&config);
        let t = Instant::now();
        let f = e.handle(&ch("k", &[Command]), t).unwrap();
        let ItemKind::Combo { caps, .. } = &f.items[0].kind else {
            panic!()
        };
        assert_eq!(
            (caps[0].legend.as_str(), caps[0].name.as_deref()),
            ("Cmd", None)
        );
        let f = e.handle(&press(Key::Return, &[], None), t).unwrap();
        let ItemKind::Combo { caps, .. } = &f.items[0].kind else {
            panic!()
        };
        assert_eq!(
            (caps[0].legend.as_str(), caps[0].name.as_deref()),
            ("Enter", Some("return"))
        );
    }
}
