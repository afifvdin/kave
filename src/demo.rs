//! A scripted loop of keystrokes for `kave --demo`: preview the look without
//! granting Input Monitoring.

use crate::config::Mode;
use crate::key::{Action, Key, KeyEvent, Modifier, Modifiers};

pub struct Step {
    /// Delay before this event, in milliseconds.
    pub after_ms: u64,
    pub event: KeyEvent,
}

struct Script {
    steps: Vec<Step>,
    mods: Modifiers,
    wait: u64,
}

impl Script {
    fn push(&mut self, key: Key, action: Action, text: Option<String>) {
        if let Key::Modifier(m) = key {
            if action == Action::Release {
                self.mods.remove(m);
            } else {
                self.mods.insert(m);
            }
        }
        self.steps.push(Step {
            after_ms: std::mem::replace(&mut self.wait, 0),
            event: KeyEvent {
                key,
                action,
                modifiers: self.mods,
                text,
            },
        });
    }

    fn wait(&mut self, ms: u64) -> &mut Self {
        self.wait += ms;
        self
    }

    fn hold(&mut self, m: Modifier) -> &mut Self {
        self.push(Key::Modifier(m), Action::Press, None);
        self.wait(90)
    }

    fn release(&mut self, m: Modifier) -> &mut Self {
        self.push(Key::Modifier(m), Action::Release, None);
        self.wait(40)
    }

    /// Press and release a key.
    fn tap(&mut self, key: Key, text: Option<&str>, hold_ms: u64) -> &mut Self {
        self.push(key.clone(), Action::Press, text.map(String::from));
        self.wait(hold_ms);
        self.push(key, Action::Release, None);
        self.wait(60)
    }

    fn char(&mut self, c: char) -> &mut Self {
        let key = if c == ' ' {
            Key::Space
        } else {
            Key::Char(c.to_lowercase().to_string())
        };
        let text = c.to_string();
        self.tap(key, Some(&text), 70)
    }
}

pub fn script(mode: Mode) -> Vec<Step> {
    use Modifier::*;
    let mut s = Script {
        steps: Vec::new(),
        mods: Modifiers::empty(),
        wait: 300,
    };
    let ch = |c: &str| Key::Char(c.into());
    match mode {
        Mode::Shortcuts => {
            s.hold(Command)
                .hold(Shift)
                .tap(ch("k"), Some("K"), 260)
                .release(Shift)
                .release(Command);
            s.wait(1300);
            s.hold(Command);
            for _ in 0..3 {
                s.tap(ch("z"), Some("z"), 120).wait(260);
            }
            s.release(Command).wait(1300);
            s.tap(Key::Escape, None, 200).wait(1300);
            s.hold(Control).hold(Option).tap(Key::Space, Some(" "), 300);
            s.release(Option).release(Control).wait(1300);
            s.hold(Command)
                .tap(Key::Return, None, 220)
                .release(Command)
                .wait(1600);
        }
        Mode::Typing => {
            for c in "Hello kave".chars() {
                s.char(c).wait(40);
            }
            s.wait(250)
                .tap(Key::Backspace, None, 90)
                .wait(140)
                .tap(Key::Backspace, None, 90);
            s.wait(200);
            for c in "ve!".chars() {
                s.char(c).wait(40);
            }
            s.wait(300)
                .hold(Command)
                .tap(ch("s"), Some("s"), 200)
                .release(Command);
            s.wait(2000);
        }
    }
    // The pause after the last step comes before the first one on the next loop.
    let trailing = s.wait;
    s.steps[0].after_ms += trailing;
    s.steps
}
