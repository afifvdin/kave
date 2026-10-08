//! User configuration, read from a TOML file.

use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, io};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Show shortcuts and special keys only, e.g. `⌘ ⇧ K` or `⎋`.
    #[default]
    Shortcuts,
    /// Show everything you type, including plain text.
    Typing,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub mode: Mode,
    pub position: Position,
    /// Distance in points from the screen edge.
    pub margin: f64,
    /// How long a keystroke stays on screen before fading out.
    pub hide_after_ms: u64,
    pub fade_ms: u64,
    /// Longest line shown in typing mode before older text scrolls off.
    pub max_chars: usize,
    pub style: Style,
    /// Replace how a key is shown. Keys are key names (`command`, `return`,
    /// `space`, `f1`, `a`, …) or the default symbol (`⌘`, `↩`, …).
    pub replacements: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the system's light or dark appearance.
    #[default]
    Auto,
    Dark,
    Light,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub theme: Theme,
    /// Height of a keycap in points; everything else scales with it.
    pub size: f64,
    /// Color of the Esc and Return keycaps and the repeat badge.
    /// `#rrggbb` or `#rrggbbaa`.
    pub accent_color: String,
    /// Font family for legends; empty means SF Pro Rounded.
    pub font: String,
    /// Draw the translucent tray behind the keycaps.
    pub tray: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::default(),
            position: Position::default(),
            margin: 80.0,
            hide_after_ms: 1200,
            fade_ms: 250,
            max_chars: 32,
            style: Style::default(),
            replacements: HashMap::new(),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Style {
            theme: Theme::Auto,
            size: 56.0,
            accent_color: "#ff6b3d".into(),
            font: String::new(),
            tray: true,
        }
    }
}

/// The documented config written by `kave --init-config` and the menu's
/// "Open Config…" item.
pub const TEMPLATE: &str = r##"# kave configuration. Delete a line to use its default.

# "shortcuts" shows combos and special keys; "typing" shows everything.
mode = "shortcuts"

# "top", "center" or "bottom" of the screen, and the gap from that edge.
position = "bottom"
margin = 80.0

hide_after_ms = 1200
fade_ms = 250

# Typing mode: roughly how many characters fit before old ones scroll off.
max_chars = 32

[style]
theme = "auto"                    # "auto", "dark" or "light"
size = 56.0                       # keycap height in points
accent_color = "#ff6b3d"          # Esc/Return keycaps and the repeat badge
font = ""                         # empty = SF Pro Rounded
tray = true                       # translucent tray behind the keys

# Relabel keys, by key name or by default legend.
[replacements]
# command = "cmd"
# option = "alt"
# "↩" = "⏎"
"##;

impl Config {
    /// `$KAVE_CONFIG`, else `~/.config/kave/config.toml` on Unix-likes and
    /// the platform config directory elsewhere.
    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("KAVE_CONFIG") {
            return Some(p.into());
        }
        let base = if cfg!(unix) {
            dirs::home_dir().map(|h| h.join(".config"))
        } else {
            dirs::config_dir()
        };
        base.map(|b| b.join("kave").join("config.toml"))
    }

    /// Load the config, falling back to defaults when the file is missing.
    pub fn load(path: Option<&PathBuf>) -> Result<Config, String> {
        let Some(path) = path else {
            return Ok(Config::default());
        };
        match fs::read_to_string(path) {
            Ok(s) => Config::parse(&s).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn parse(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }

    /// Write [`TEMPLATE`] to `path` unless a file already exists there.
    pub fn write_template(path: &PathBuf) -> io::Result<()> {
        if path.exists() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, TEMPLATE)
    }
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` into RGBA components in `0..=1`.
pub fn parse_color(s: &str) -> Option<[f64; 4]> {
    let hex = s.trim().strip_prefix('#')?;
    let expanded: String = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_string()
    };
    if !matches!(expanded.len(), 6 | 8) || !expanded.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&expanded[i..i + 2], 16).ok();
    let a = if expanded.len() == 8 { byte(6)? } else { 255 };
    Some([byte(0)?, byte(2)?, byte(4)?, a].map(|b| b as f64 / 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_matches_defaults() {
        assert_eq!(Config::parse(TEMPLATE).unwrap(), Config::default());
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let c = Config::parse("mode = \"typing\"\n[style]\ntheme = \"light\"").unwrap();
        assert_eq!(c.mode, Mode::Typing);
        assert_eq!(c.style.theme, Theme::Light);
        assert_eq!(c.style.size, Style::default().size);
        assert_eq!(c.hide_after_ms, Config::default().hide_after_ms);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(Config::parse("colour = \"red\"").is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_color("#00000000"), Some([0.0, 0.0, 0.0, 0.0]));
        assert_eq!(parse_color("#ff0000").map(|c| c[0]), Some(1.0));
        assert_eq!(parse_color("ff0000"), None);
        assert_eq!(parse_color("#ff00"), None);
        assert_eq!(parse_color("#gg0000"), None);
    }
}
