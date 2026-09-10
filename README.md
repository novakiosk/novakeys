# NOVA Keys
<img width="805" height="255" alt="NOVA Keys showing its dark and light themes" src="https://github.com/user-attachments/assets/dc8180c4-656c-41e4-8ae2-70518710da3f" />

NOVA Keys is a GTK4 on-screen keyboard for compatible Wayland desktops. It offers
21 language layouts, long-press alternatives, custom layouts, and local Chinese,
Japanese and Korean composition and conversion.

It was created as an extension for [NOVA Kiosk](https://github.com/novakiosk/novakiosk) and is derived from
[wkeys](https://github.com/ptazithos/wkeys) by Tai Zeming.

## Compatibility

NOVA Keys has been tested with Sway. Other Wayland compositors need all three
protocols used by the keyboard:

- `input-method-v2` for text-field focus and Unicode text delivery;
- `virtual-keyboard-v1` for controls such as Enter and Backspace;
- `wlr-layer-shell` for the on-screen keyboard window.

X11 sessions are not supported. Applications must also provide Wayland
text-input integration for the keyboard to appear when a field is focused.
Another input method already owning the seat can prevent NOVA Keys from starting.

## Release downloads

[Release archives](https://github.com/novakiosk/novakeys/releases) are labelled with their Fedora version and x86_64 architecture.
They include the executable, configuration example and license notices; install
the matching shared libraries and dictionaries listed below. These archives are
not self-contained binaries for other Linux distributions.

## Build and install

You need the Rust toolchain selected by `rust-toolchain.toml`, a C compiler,
`pkg-config`, GTK 4.10 or newer, gtk4-layer-shell, libxkbcommon, Wayland,
Anthy Unicode (including its input library), libhangul, librime 1.13 or newer,
and Luna Pinyin dictionaries. Fonts must cover the scripts you use.

### Fedora

```sh
sudo dnf install gcc pkgconf-pkg-config gtk4-devel gtk4-layer-shell-devel \
  libxkbcommon-devel wayland-devel anthy-unicode-devel libhangul-devel \
  librime-devel brise google-noto-sans-cjk-fonts
```

### Debian 13 (trixie)

Install the packaged prerequisites:

```sh
sudo apt update
sudo apt install --no-install-recommends build-essential pkg-config meson ninja-build \
  ca-certificates curl xz-utils libgtk-4-dev libgtk4-layer-shell-dev libxkbcommon-dev libwayland-dev \
  libhangul-dev librime-dev rime-data-luna-pinyin rime-prelude rime-essay fonts-noto-cjk
```

Debian's `libanthy-dev` does not provide Anthy Unicode. Build and install
[Anthy Unicode 1.0.0.20260213](https://github.com/fujiwarat/anthy-unicode/releases/tag/1.0.0.20260213),
including `libanthyinput-unicode`, into `/usr/local`:

```sh
(
  set -e
  cd "$(mktemp -d)"
  curl -fLO https://github.com/fujiwarat/anthy-unicode/releases/download/1.0.0.20260213/anthy-unicode-1.0.0.20260213.tar.xz
  echo '1d79da684ba4b8bee82e55987361cceb970678045a26ad0b5435de44510b3252  anthy-unicode-1.0.0.20260213.tar.xz' | sha256sum --check
  tar -xf anthy-unicode-1.0.0.20260213.tar.xz
  cd anthy-unicode-1.0.0.20260213
  meson setup build --prefix=/usr/local --libdir=lib --sysconfdir=/usr/local/etc -Demacs=disabled
  meson compile -C build
  sudo meson install -C build
  sudo ldconfig
)
```

### Compile NOVA Keys

The runtime needs the corresponding shared libraries and complete Luna Pinyin
data, including the default presets and essay data. Other distributions need
equivalent packages; Anthy Unicode with `libanthyinput-unicode` is required.

From a source checkout:

```sh
cargo build --release --locked
install -Dm755 target/release/novakeys ~/.local/bin/novakeys
```

Add `~/.local/bin` to your `PATH`, then run `novakeys` as your desktop user
inside the Wayland session. To try it without installing, run
`./target/release/novakeys`. Only one instance runs per user.

For Sway autostart, add this to `~/.config/sway/config`:

```sway
exec ~/.local/bin/novakeys
```

Use your compositor's equivalent session-start setting elsewhere. The process
needs the session's `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` environment.

## Using the keyboard

Focus a supported application's text field to show the keyboard. Tap a key
to type, hold a key with alternatives to open its selection popup, and use the
language button to change layouts. Shift uppercases the next single-character key or alternative.

Chinese, Japanese and Korean composition starts automatically with their layouts.
Uncommitted text and candidates appear above the keys. `ABC` types Latin text.
Switching language or script commits the current selection first; changing text
fields discards unfinished composition.

- **Chinese:** type Pinyin words or sentences, then select a candidate or press
  Space. `nihao` offers `你好`; `xi'an` distinguishes syllables. Use `v` for ü;
  `lue` and `nue` are also accepted. `简` and `繁` select Simplified or Traditional
  output. Arrow and page buttons edit or browse candidates.
- **Japanese:** type romaji, for example `nihongo` → `にほんご`. Press Space or
  `変換` for kanji such as `日本語`, choose candidates for each segment, then Enter
  to commit. Segment controls move or resize the selected segment. Hiragana,
  Katakana and Latin modes are available. Cursor movement is disabled while a
  romaji syllable is unfinished; complete it or use Backspace first.
- **Korean:** use the Hangul-labelled Dubeolsik keys. Shift supplies doubled
  consonants and shifted vowels. Syllables combine as you type. `漢字` offers
  Hanja for the current uncommitted word; Space commits the word and a space.

With no composition, Enter sends a normal Return key. Input stays on the local
machine. Password and sensitive fields use direct Latin input without
composition or dictionary lookup. The engines do not learn a personal typing
history; compiled system dictionaries are cached privately.

## Configuration

Create a TOML file named `config` in `$XDG_CONFIG_HOME/novakeys/`, normally
`~/.config/novakeys/`. All settings are optional; see
[config.example](config.example).

```toml
# ~/.config/novakeys/config
default_language = "en"
supported_languages = ["en", "no", "de", "fr"]
show_language_switcher = true
dark_mode = false
keyboard_width = 800
backdrop_width = "content" # "content" or "stretch"
```

Omit `supported_languages` to make every available layout selectable.
Without custom layouts, the available layouts are the built-ins:
`ar`, `cs`, `de`, `el`, `en`, `es`, `fr`, `he`, `hi`, `hu`, `ja`, `ka`, `ko`,
`no`, `pl`, `pt`, `sv`, `tr`, `uk`, `ur`, and `zh`.

`default_language` must name an available layout. Without it, a custom layout
marked `default = true` takes precedence, followed by English, then the first
available code. User configuration takes precedence over XDG system
configuration directories.

Set `keyboard_width` between 240 and 7680 pixels. For custom styling, place
GTK CSS in `style.css` beside `config`. `backdrop_width = "stretch"` extends
the backdrop across the screen; for example:

```css
window > box.novakeys-backdrop.stretch {
    background: rgba(0, 0, 0, 0.4);
}
```

Apply configuration, layout, and CSS edits with `novakeys -m reload-config`.
Invalid changes are rejected while the previous configuration stays active.

For comfortable key and popup sizes, use a keyboard width of at least 480 pixels;
800 pixels is the tested default. Very narrow windows can exceed the requested width.

## Custom layouts

Use the [bundled layouts](assets) as examples. Save your layout as
`layout-<code>.toml` beside `config`, using lowercase letters, digits, or
hyphens for the code. Optional `language_code` metadata must match the filename.

```toml
# layout-custom.toml
language_name = "Custom"
language_flag = "🌐"
layout = [
  [{ text = "a", alternatives = ["á", "å"] }, { text = "b" }],
  [{ action = "shift" }, { text = " ", display_text = "Space", width = 3 },
   { action = "backspace" }, { action = "enter" },
   { action = "language_selector" }],
]
```

Custom layouts override built-ins with the same code. By default, adding custom
layouts makes only those layouts available. Set `include_builtins_with_custom = true`
in `config` to keep the bundled layouts too.

Keys can specify `text`, `action`, `display_text`, `alternatives`, `flag`, and
`width` (1–32 relative units), with up to eight alternatives per key. Actions are
`backspace`, `enter`, `space`, `shift`,
`language_selector`. The `zh`, `ja`, and `ko` layout codes enable their respective
composition engines.

## Commands

These commands control an already-running instance:

```sh
novakeys -m reload-config
novakeys -m set-language no
novakeys -m get-status
novakeys -m hide-language-switcher
novakeys -m show-language-switcher
novakeys -m close
```

`get-status` prints JSON. Successful commands exit zero; failures exit nonzero.
`close` also succeeds when no instance is running. Commands can be combined
and run in order, such as `novakeys -m reload-config -m set-language en`.
If a command reports an unknown outcome, query status before retrying.

## License

This repository is licensed under the [MIT License](LICENSE)
