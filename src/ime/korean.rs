use super::{Command, MAX_INPUT, PAGE_SIZE, Snapshot, copy_text, cstring, ffi};
use anyhow::{Result, ensure};
use std::{ffi::c_void, ptr};
pub(super) struct Korean {
    context: *mut c_void,
    table: *mut c_void,
    raw: String,
    prefix: String,
    candidates: Vec<String>,
    page: usize,
}
unsafe fn ucs4(pointer: *const u32) -> Result<String> {
    if pointer.is_null() {
        return Ok(String::new());
    }
    let mut text = String::new();
    for index in 0..MAX_INPUT {
        let value = unsafe { *pointer.add(index) };
        if value == 0 {
            return Ok(text);
        }
        text.push(
            char::from_u32(value)
                .ok_or_else(|| anyhow::anyhow!("Invalid Korean engine character"))?,
        );
    }
    anyhow::bail!("Korean engine text exceeds limit")
}
impl Korean {
    pub fn new() -> Result<Self> {
        let context = unsafe { ffi::hangul_ic_new(c"2".as_ptr()) };
        ensure!(!context.is_null(), "Korean input context unavailable");
        Ok(Self {
            context,
            table: ptr::null_mut(),
            raw: String::new(),
            prefix: String::new(),
            candidates: Vec::new(),
            page: 0,
        })
    }
    pub fn reset(&mut self) {
        unsafe {
            ffi::hangul_ic_reset(self.context);
        }
        self.raw.clear();
        self.prefix.clear();
        self.candidates.clear();
        self.page = 0;
    }
    fn preedit(&self) -> Result<String> {
        Ok(format!("{}{}", self.prefix, unsafe {
            ucs4(ffi::hangul_ic_get_preedit_string(self.context))
        }?))
    }
    fn finish(&mut self) -> Result<String> {
        let text = format!("{}{}", self.prefix, unsafe {
            ucs4(ffi::hangul_ic_flush(self.context))
        }?);
        self.reset();
        Ok(text)
    }
    fn feed(&mut self, key: char) -> Result<bool> {
        let consumed = unsafe { ffi::hangul_ic_process(self.context, key as i32) };
        let text = unsafe { ucs4(ffi::hangul_ic_get_commit_string(self.context)) }?;
        self.prefix.push_str(&text);
        Ok(consumed)
    }
    fn replay(&mut self) -> Result<()> {
        let raw = self.raw.clone();
        unsafe {
            ffi::hangul_ic_reset(self.context);
        }
        self.prefix.clear();
        for key in raw.chars() {
            self.feed(key)?;
        }
        Ok(())
    }
    fn hanja(&mut self) -> Result<()> {
        if self.table.is_null() {
            self.table = unsafe { ffi::hanja_table_load(ptr::null()) };
            ensure!(!self.table.is_null(), "Korean Hanja dictionary unavailable");
        }
        let text = cstring(&self.preedit()?)?;
        let list = unsafe { ffi::hanja_table_match_exact(self.table, text.as_ptr()) };
        self.candidates.clear();
        self.page = 0;
        if list.is_null() {
            return Ok(());
        }
        let count = unsafe { ffi::hanja_list_get_size(list) };
        let result = (|| -> Result<Vec<String>> {
            ensure!(
                (0..=10000).contains(&count),
                "Hanja candidate count exceeds limit"
            );
            (0..count)
                .map(|index| unsafe {
                    copy_text(ffi::hanja_list_get_nth_value(list, index as u32))
                })
                .collect()
        })();
        unsafe {
            ffi::hanja_list_delete(list);
        }
        self.candidates = result?;
        Ok(())
    }
    pub fn command(&mut self, command: &Command) -> Result<Snapshot> {
        let mut committed = String::new();
        let mut control = None;
        match command {
            Command::Reset => self.reset(),
            Command::Text(text) => {
                super::check_input_limit(self.raw.len() + text.len())?;
                self.candidates.clear();
                for key in text.chars() {
                    if key.is_ascii_alphabetic() {
                        if self.feed(key)? {
                            self.raw.push(key);
                        } else {
                            committed.push_str(&self.finish()?);
                            committed.push(key);
                        }
                    } else {
                        committed.push_str(&self.finish()?);
                        committed.push(key);
                    }
                }
            }
            Command::Backspace => {
                self.candidates.clear();
                if self.raw.pop().is_some() {
                    if !unsafe { ffi::hangul_ic_backspace(self.context) } {
                        self.replay()?;
                    }
                } else {
                    control = Some('\u{0008}');
                }
            }
            Command::Space => {
                committed = self.finish()?;
                committed.push(' ');
            }
            Command::Enter | Command::Finish => {
                if self.raw.is_empty() {
                    if matches!(command, Command::Enter) {
                        control = Some('\n');
                    }
                } else {
                    committed = self.finish()?;
                }
            }
            Command::Escape => {
                if self.candidates.is_empty() {
                    self.reset();
                } else {
                    self.candidates.clear();
                }
            }
            Command::Hanja => self.hanja()?,
            Command::Select(index) => {
                let choice = self.page * PAGE_SIZE + index;
                ensure!(
                    *index < PAGE_SIZE && choice < self.candidates.len(),
                    "Hanja candidate has expired"
                );
                committed = self.candidates[choice].clone();
                self.reset();
            }
            Command::Page(delta) => {
                let pages = self.candidates.len().div_ceil(PAGE_SIZE);
                self.page =
                    (self.page as i32 + delta).clamp(0, pages.saturating_sub(1) as i32) as usize;
            }
            _ => {}
        }
        let preedit = self.preedit()?;
        let begin = self.page * PAGE_SIZE;
        Ok(Snapshot {
            cursor: preedit.len(),
            preedit,
            candidates: self
                .candidates
                .iter()
                .skip(begin)
                .take(PAGE_SIZE)
                .cloned()
                .collect(),
            page: self.page,
            has_previous: self.page > 0,
            has_next: begin + PAGE_SIZE < self.candidates.len(),
            committed,
            control,
            ..Snapshot::default()
        })
    }
}
impl Drop for Korean {
    fn drop(&mut self) {
        unsafe {
            ffi::hangul_ic_delete(self.context);
            if !self.table.is_null() {
                ffi::hanja_table_delete(self.table);
            }
        }
    }
}
