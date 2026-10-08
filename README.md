# kave

Open source keyboard visualizer. kave shows the keys you press as a row of
little keycaps, for screencasts, live coding and presentations.

- **Real keycaps.** Every key is drawn as a 3D keycap, labelled the way Apple
  keyboards are (`⌘` in the corner, "command" underneath). Caps sink while
  you hold a key and spring back up when you let go.
- **Accent caps.** Esc and Return get a coral accent keycap, like an artisan cap
  on a mechanical keyboard. The color is yours to pick.
- **Shortcuts mode** (default) shows combos and special keys; **typing mode**
  shows everything you type, with special keys as caps inline.
- **Repeats are counted.** Pressing `⌘ Z` three times shows one `⌘ Z` and a `×3`
  badge instead of flooding the screen.
- **Follows your keyboard layout**, so the same key reads `⌘ Z` on QWERTY and
  `⌘ Y` on QWERTZ.
- **Follows light and dark mode**, sits on top of every Space and full-screen
  app, and never steals a click.
- A menu bar item to switch modes, pause, and edit or reload the config.

Try the look without granting any permission:

```bash
cargo run -- --demo
```

Platforms: **macOS** (14 Sonoma or later) for now. Windows and Linux are planned;
the key model, config and display logic are platform-independent, so a new
platform only needs a backend in `src/platform/`.

## Install

You need a [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/afifvdin/kave && cd kave
./scripts/bundle-macos.sh
cp -r target/release/Kave.app /Applications/
open /Applications/Kave.app
```

### Permission

kave reads keystrokes, so macOS asks for **Input Monitoring** the first time it
starts. Allow it in System Settings → Privacy & Security → Input Monitoring,
then start kave again.

If you run the bare binary (`cargo run`) instead of `Kave.app`, the permission
belongs to your terminal app rather than to kave.

kave never sees what you type into password fields: macOS blocks key events
while Secure Input is on.

## Usage

kave runs in the menu bar (keyboard icon). From there you can toggle typing mode,
pause, open or reload the config, and quit.

```
kave [options]

  -t, --typing        Show everything you type, not just shortcuts
  -s, --shortcuts     Show shortcuts and special keys only (default)
  -c, --config PATH   Read config from PATH (default: ~/.config/kave/config.toml)
      --demo          Play example keystrokes to preview the look
      --init-config   Write a documented config file and exit
  -h, --help          Show this help
  -V, --version       Show the version
```

Set `KAVE_DEBUG=1` to log every key event and what kave shows for it.

## Configuration

Config lives in `~/.config/kave/config.toml` (or `$KAVE_CONFIG`). Create a
documented one with `kave --init-config` or the menu's **Open Config…**, and apply
changes with **Reload Config**. Every setting is optional:

```toml
mode = "shortcuts"          # or "typing"
position = "bottom"         # "top", "center" or "bottom"
margin = 80.0               # gap from that screen edge, in points
hide_after_ms = 1200
fade_ms = 250
max_chars = 32              # typing mode: roughly how much text fits

[style]
theme = "auto"              # "auto" follows the system, or "dark" / "light"
size = 56.0                 # keycap height in points
accent_color = "#ff6b3d"    # Esc/Return caps and the repeat badge
font = ""                   # empty = SF Pro Rounded
tray = true                 # translucent tray behind the keys

# Relabel keys, by key name or by default legend.
[replacements]
command = "cmd"
"↩" = "⏎"
```

Key names: `command`, `control`, `option`, `shift`, `fn`, `return`, `enter`,
`tab`, `space`, `backspace`, `delete`, `escape`, `capslock`, `up`, `down`,
`left`, `right`, `home`, `end`, `pageup`, `pagedown`, `f1`–`f20`, and the
character itself for everything else (`a`, `1`, `/`).

## Development

```bash
cargo test
cargo run -- --typing
```

```
src/
  key.rs          platform-independent key model
  config.rs       config file
  display.rs      turns key events into keycaps on screen (unit tested)
  theme.rs        light and dark palettes
  demo.rs         the scripted keystrokes behind --demo
  platform/
    macos/        event tap, keyboard layout lookup, Core Animation overlay, menu bar
```

## Roadmap

- [x] macOS support
- [x] Automatically detect keyboard layout
- [x] Custom styling
- [x] Character replacement
- [x] Typing mode
- [ ] Windows
- [ ] Linux (X11 and Wayland)
- [ ] Mouse clicks
- [ ] Keyboard sound
- [ ] Multi-monitor placement options
