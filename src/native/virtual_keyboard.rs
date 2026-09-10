//! Fixed physical keys for actions that must remain keyboard events.
use std::{io::Write, os::fd::AsFd, time::Instant};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

const KEYMAP: &str = "xkb_keymap {\n\
xkb_keycodes \"novakeys\" { minimum=8; maximum=36; <ESC>=9; <BKSP>=22; <TAB>=23; <RTRN>=36; };\n\
xkb_types \"novakeys\" { include \"complete\" };\n\
xkb_compatibility \"novakeys\" { include \"complete\" };\n\
xkb_symbols \"novakeys\" { key <ESC> { [ Escape ] }; key <BKSP> { [ BackSpace ] }; key <TAB> { [ Tab ] }; key <RTRN> { [ Return ] }; };\n};\n\0";

pub(crate) fn control_code(ch: char) -> Option<u32> {
    match ch {
        '\n' => Some(28),
        '\t' => Some(15),
        '\u{001b}' => Some(1),
        '\u{0008}' => Some(14),
        _ => None,
    }
}
pub struct VirtualKeyboardV1 {
    proxy: ZwpVirtualKeyboardV1,
    started: Instant,
}
impl VirtualKeyboardV1 {
    pub(crate) fn new(proxy: ZwpVirtualKeyboardV1) -> anyhow::Result<Self> {
        let mut file = tempfile::tempfile()?;
        file.write_all(KEYMAP.as_bytes())?;
        proxy.keymap(1, file.as_fd(), KEYMAP.len() as u32);
        Ok(Self {
            proxy,
            started: Instant::now(),
        })
    }
    pub(crate) fn key(&self, code: u32) {
        let time = self.started.elapsed().as_millis() as u32;
        self.proxy.modifiers(0, 0, 0, 0);
        self.proxy.key(time, code, 1);
        self.proxy.key(time, code, 0);
    }
    pub(crate) fn destroy(&self) {
        self.proxy.destroy();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_control_map_resolves_normal_positions() {
        let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
        let map = xkbcommon::xkb::Keymap::new_from_string(
            &context,
            KEYMAP.trim_end_matches('\0').into(),
            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap();
        let state = xkbcommon::xkb::State::new(&map);
        for (ch, name) in [
            ('\n', "Return"),
            ('\t', "Tab"),
            ('\u{001b}', "Escape"),
            ('\u{0008}', "BackSpace"),
        ] {
            let symbol = state.key_get_one_sym((control_code(ch).unwrap() + 8).into());
            assert_eq!(xkbcommon::xkb::keysym_get_name(symbol), name);
        }
        assert_eq!(control_code(' '), None);
        assert_eq!(control_code('ΰ'), None);
    }
}
