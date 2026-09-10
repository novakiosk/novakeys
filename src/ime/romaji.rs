//! One incremental Anthy romanization context, restricted to unconverted NONE/EDIT states.
use super::{MAX_INPUT, Mode, copy_text, cstring, ffi};
use anyhow::{Result, ensure};
use std::ffi::c_void;
pub(super) struct Reading {
    pub text: String,
    pub cursor: usize,
    pub pending: String,
}
pub(super) struct Romaji {
    config: *mut c_void,
    context: *mut c_void,
    mode: Mode,
}
impl Romaji {
    pub fn new() -> Result<Self> {
        let config = unsafe { ffi::anthy_input_create_config() };
        ensure!(
            !config.is_null(),
            "Japanese typing configuration unavailable"
        );
        let result = Self {
            config,
            context: std::ptr::null_mut(),
            mode: Mode::Native,
        };
        for (map, to) in [(2, c"ん"), (3, c"ン")] {
            ensure!(
                unsafe {
                    ffi::anthy_input_edit_rk_config(
                        config,
                        map,
                        c"n'".as_ptr(),
                        to.as_ptr(),
                        c"".as_ptr(),
                    )
                } == 0,
                "Japanese apostrophe mapping unavailable"
            );
        }
        unsafe {
            ffi::anthy_input_change_config(config);
        }
        Ok(result)
    }
    pub fn reset(&mut self) {
        if !self.context.is_null() {
            unsafe {
                ffi::anthy_input_free_context(self.context);
            }
        }
        self.context = std::ptr::null_mut();
    }
    fn ensure_context(&mut self) -> Result<()> {
        if self.context.is_null() {
            self.context = unsafe { ffi::anthy_input_create_context(self.config) };
            ensure!(
                !self.context.is_null(),
                "Japanese typing context unavailable"
            );
            ensure!(
                unsafe {
                    ffi::anthy_input_map_select(
                        self.context,
                        if self.mode == Mode::Katakana { 3 } else { 2 },
                    )
                } == 0,
                "Japanese typing mode unavailable"
            );
        }
        Ok(())
    }
    pub fn mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.reset();
    }
    pub fn insert(&mut self, text: &str) -> Result<()> {
        self.ensure_context()?;
        let text = cstring(text)?;
        unsafe {
            ffi::anthy_input_str(self.context, text.as_ptr());
        }
        self.editable()
    }
    fn editable(&self) -> Result<()> {
        ensure!(
            [1, 2].contains(&unsafe { ffi::anthy_input_get_state(self.context) }),
            "Japanese parser left unconverted editing state"
        );
        Ok(())
    }
    pub fn backspace(&mut self) -> Result<()> {
        if self.context.is_null() {
            return Ok(());
        }
        self.editable()?;
        unsafe {
            ffi::anthy_input_erase_prev(self.context);
        }
        Ok(())
    }
    pub fn move_cursor(&mut self, delta: i32) -> Result<()> {
        if !self.context.is_null() && self.snapshot()?.pending.is_empty() {
            self.editable()?;
            unsafe {
                ffi::anthy_input_move(self.context, delta.signum());
            }
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<String> {
        let reading = self.snapshot()?;
        if reading.text.is_empty() {
            return Ok(String::new());
        }
        let text = if !reading.pending.is_empty() && reading.pending != "n" {
            reading.text
        } else {
            // EDIT finalizes unconverted text; reject CONV to avoid its learning path.
            ensure!(
                unsafe { ffi::anthy_input_get_state(self.context) } == 2,
                "Japanese finalization requires unconverted text"
            );
            unsafe {
                ffi::anthy_input_commit(self.context);
            }
            let edit = unsafe { ffi::anthy_input_get_preedit(self.context) };
            ensure!(!edit.is_null(), "Japanese finalized reading unavailable");
            let result = unsafe { copy_text((*edit).commit) };
            unsafe {
                ffi::anthy_input_free_preedit(edit);
            }
            result?
        };
        self.reset();
        Ok(text)
    }
    pub fn snapshot(&self) -> Result<Reading> {
        if self.context.is_null() {
            return Ok(Reading {
                text: String::new(),
                cursor: 0,
                pending: String::new(),
            });
        }
        self.editable()?;
        let edit = unsafe { ffi::anthy_input_get_preedit(self.context) };
        ensure!(!edit.is_null(), "Japanese typing preview unavailable");
        let result = (|| {
            let mut part = unsafe { (*edit).segment };
            let mut text = String::new();
            let mut pending = String::new();
            let mut cursor = None;
            let mut count = 0;
            while !part.is_null() {
                ensure!(count < MAX_INPUT, "Japanese preview exceeds segment limit");
                if unsafe { (*part).flags } & 1 != 0 {
                    cursor = Some(text.len());
                }
                let value = unsafe { copy_text((*part).text) }?;
                ensure!(
                    text.len() + value.len() <= 4096,
                    "Japanese reading exceeds text limit"
                );
                text.push_str(&value);
                if unsafe { (*part).flags } & 16 != 0 {
                    pending.push_str(&value);
                }
                part = unsafe { (*part).next };
                count += 1;
            }
            Ok(Reading {
                cursor: cursor.unwrap_or(text.len()),
                text,
                pending,
            })
        })();
        unsafe {
            ffi::anthy_input_free_preedit(edit);
        }
        result
    }
}
impl Drop for Romaji {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                ffi::anthy_input_free_context(self.context);
            }
            ffi::anthy_input_free_config(self.config);
        }
    }
}
