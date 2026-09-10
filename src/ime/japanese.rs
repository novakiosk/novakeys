use super::{Command, Mode, PAGE_SIZE, Snapshot, cstring, ffi, profiles::Paths};
use anyhow::{Result, ensure};
use std::{
    ffi::{CString, c_void},
    ptr,
};
pub(super) struct Japanese {
    romaji: Option<super::romaji::Romaji>,
    context: *mut c_void,
    mode: Mode,
    choices: Vec<usize>,
    segment: usize,
    page: usize,
    _profile: CString,
}
impl Japanese {
    pub fn new(paths: &Paths) -> Result<Self> {
        ensure!(
            std::env::var_os("ANTHY_HISTORY_FILE").is_none_or(|value| value.is_empty()),
            "ANTHY_HISTORY_FILE must be unset for private Japanese input"
        );
        let profile = CString::new(paths.directory("anthy")?.as_os_str().as_encoded_bytes())?;
        unsafe {
            ffi::anthy_conf_override(c"HOME".as_ptr(), profile.as_ptr());
            ffi::anthy_conf_override(c"XDG_CONFIG_HOME".as_ptr(), profile.as_ptr());
        }
        ensure!(
            unsafe { ffi::anthy_input_init() } == 0,
            "Japanese dictionary could not be initialized"
        );
        let romaji = match super::romaji::Romaji::new() {
            Ok(value) => value,
            Err(error) => {
                unsafe {
                    ffi::anthy_quit();
                }
                return Err(error);
            }
        };
        let context = unsafe { ffi::anthy_create_context() };
        if context.is_null() {
            drop(romaji);
            unsafe {
                ffi::anthy_quit();
            }
            anyhow::bail!("Japanese conversion context unavailable");
        }
        unsafe {
            ffi::anthy_context_set_encoding(context, 2);
        }
        Ok(Self {
            romaji: Some(romaji),
            context,
            mode: Mode::Native,
            choices: Vec::new(),
            segment: 0,
            page: 0,
            _profile: profile,
        })
    }
    pub fn reset(&mut self) {
        unsafe {
            ffi::anthy_reset_context(self.context);
        }
        self.romaji.as_mut().unwrap().reset();
        self.choices.clear();
        self.segment = 0;
        self.page = 0;
    }
    fn segment_count(&self) -> Result<usize> {
        let mut stat = ffi::AnthyStat { segments: 0 };
        ensure!(
            unsafe { ffi::anthy_get_stat(self.context, &mut stat) } == 0
                && (0..=64).contains(&stat.segments),
            "Invalid Japanese segment count"
        );
        Ok(stat.segments as usize)
    }
    fn candidate_count(&self, segment: usize) -> Result<usize> {
        let mut stat = ffi::AnthySegment {
            candidates: 0,
            length: 0,
        };
        ensure!(
            unsafe { ffi::anthy_get_segment_stat(self.context, segment as i32, &mut stat) } == 0
                && (1..=10000).contains(&stat.candidates),
            "Japanese candidates unavailable"
        );
        Ok(stat.candidates as usize)
    }
    fn candidate(&self, segment: usize, candidate: usize) -> Result<String> {
        let length = unsafe {
            ffi::anthy_get_segment(
                self.context,
                segment as i32,
                candidate as i32,
                ptr::null_mut(),
                0,
            )
        };
        ensure!(
            (0..=4096).contains(&length),
            "Japanese candidate exceeds text limit"
        );
        let mut buffer = vec![0u8; length as usize + 1];
        ensure!(
            unsafe {
                ffi::anthy_get_segment(
                    self.context,
                    segment as i32,
                    candidate as i32,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as i32,
                )
            } >= 0,
            "Japanese candidate unavailable"
        );
        buffer.truncate(length as usize);
        Ok(String::from_utf8(buffer)?)
    }
    fn convert(&mut self) -> Result<()> {
        // Keep the live parser for cancellation instead of reconstructing its kana/cursor.
        // input_str parses keys; reinserting finalized UTF-8 is not a buffer setter.
        if self.romaji.as_ref().unwrap().snapshot()?.pending == "n" {
            self.romaji.as_mut().unwrap().insert("'")?;
        }
        let reading = self.romaji.as_ref().unwrap().snapshot()?.text;
        let reading = cstring(&reading)?;
        ensure!(
            unsafe { ffi::anthy_set_string(self.context, reading.as_ptr()) } == 0,
            "Japanese conversion failed"
        );
        self.choices = vec![0; self.segment_count()?];
        self.segment = 0;
        self.page = 0;
        Ok(())
    }
    fn snapshot(&self) -> Result<Snapshot> {
        if self.choices.is_empty() {
            let reading = self.romaji.as_ref().unwrap().snapshot()?;
            return Ok(Snapshot {
                preedit: reading.text,
                cursor: reading.cursor,
                pending_romaji: !reading.pending.is_empty(),
                ..Default::default()
            });
        }
        let mut preedit = String::new();
        let mut selection = None;
        for (index, choice) in self.choices.iter().enumerate() {
            let start = preedit.len();
            preedit.push_str(&self.candidate(index, *choice)?);
            if index == self.segment {
                selection = Some((start, preedit.len()));
            }
        }
        let count = self.candidate_count(self.segment)?;
        let begin = self.page * PAGE_SIZE;
        let candidates = (begin..(begin + PAGE_SIZE).min(count))
            .map(|choice| self.candidate(self.segment, choice))
            .collect::<Result<Vec<_>>>()?;
        Ok(Snapshot {
            cursor: selection.map_or(preedit.len(), |(_, end)| end),
            preedit,
            selection,
            selected: self.choices[self.segment]
                .checked_sub(begin)
                .filter(|index| *index < candidates.len()),
            candidates,
            page: self.page,
            has_previous: self.page > 0,
            has_next: begin + PAGE_SIZE < count,
            segments: self.choices.len(),
            active_segment: self.segment,
            ..Snapshot::default()
        })
    }
    fn finish(&mut self) -> Result<String> {
        let text = if self.choices.is_empty() {
            self.romaji.as_mut().unwrap().finish()?
        } else {
            self.snapshot()?.preedit
        };
        self.reset();
        Ok(text)
    }
    pub fn command(&mut self, command: &Command, mode: Mode) -> Result<Snapshot> {
        if self.mode != mode {
            self.reset();
            self.mode = mode;
            self.romaji.as_mut().unwrap().mode(mode);
        }
        let mut committed = String::new();
        let mut control = None;
        match command {
            Command::Reset => self.reset(),
            Command::Text(text) => {
                super::check_input_limit(
                    self.romaji.as_ref().unwrap().snapshot()?.text.len() + text.len(),
                )?;
                for ch in text.chars() {
                    if ch.is_ascii_alphabetic() || ch == '\'' || ch == '-' {
                        if !self.choices.is_empty() {
                            committed.push_str(&self.finish()?);
                        }
                        self.romaji
                            .as_mut()
                            .unwrap()
                            .insert(&ch.to_ascii_lowercase().to_string())?;
                    } else {
                        committed.push_str(&self.finish()?);
                        committed.push(ch);
                    }
                }
            }
            Command::Backspace => {
                if !self.choices.is_empty() {
                    self.choices.clear();
                } else if self.romaji.as_ref().unwrap().snapshot()?.text.is_empty() {
                    control = Some('\u{0008}');
                } else {
                    self.romaji.as_mut().unwrap().backspace()?;
                }
            }

            Command::Space => {
                if !self.choices.is_empty() {
                    let count = self.candidate_count(self.segment)?;
                    self.choices[self.segment] = (self.choices[self.segment] + 1) % count;
                    self.page = self.choices[self.segment] / PAGE_SIZE;
                } else if self.romaji.as_ref().unwrap().snapshot()?.text.is_empty() {
                    committed.push(' ');
                } else if mode == Mode::Katakana {
                    committed = self.finish()?;
                } else {
                    self.convert()?;
                }
            }
            Command::Enter | Command::Finish => {
                if !self.choices.is_empty() {
                    committed = self.finish()?;
                } else if self.romaji.as_ref().unwrap().snapshot()?.text.is_empty() {
                    if matches!(command, Command::Enter) {
                        control = Some('\n');
                    }
                } else {
                    committed = self.finish()?;
                }
            }
            Command::Escape => {
                if !self.choices.is_empty() {
                    self.choices.clear();
                } else {
                    self.reset();
                }
            }
            Command::Select(index) => {
                ensure!(
                    !self.choices.is_empty() && self.segment < self.choices.len(),
                    "Candidate has expired"
                );
                let choice = self.page * PAGE_SIZE + index;
                ensure!(
                    *index < PAGE_SIZE && choice < self.candidate_count(self.segment)?,
                    "Candidate has expired"
                );
                self.choices[self.segment] = choice;
            }
            Command::Page(delta) => {
                if !self.choices.is_empty() {
                    let pages = self.candidate_count(self.segment)?.div_ceil(PAGE_SIZE);
                    self.page = (self.page as i32 + delta).clamp(0, pages.saturating_sub(1) as i32)
                        as usize;
                }
            }
            Command::Segment(delta) => {
                if !self.choices.is_empty() {
                    self.segment = (self.segment as i32 + delta)
                        .clamp(0, self.choices.len() as i32 - 1)
                        as usize;
                    self.page = self.choices[self.segment] / PAGE_SIZE;
                }
            }
            Command::Resize(delta) => {
                if !self.choices.is_empty() {
                    unsafe {
                        ffi::anthy_resize_segment(
                            self.context,
                            self.segment as i32,
                            delta.signum(),
                        );
                    }
                    self.choices = vec![0; self.segment_count()?];
                    self.segment = self.segment.min(self.choices.len().saturating_sub(1));
                    self.page = 0;
                }
            }
            Command::Left | Command::Right => {
                if self.choices.is_empty() {
                    self.romaji.as_mut().unwrap().move_cursor(
                        if matches!(command, Command::Left) {
                            -1
                        } else {
                            1
                        },
                    )?;
                } else {
                    let delta = if matches!(command, Command::Left) {
                        -1
                    } else {
                        1
                    };
                    self.segment = (self.segment as i32 + delta)
                        .clamp(0, self.choices.len() as i32 - 1)
                        as usize;
                    self.page = 0;
                }
            }
            Command::Hanja => {}
        }
        let mut snapshot = self.snapshot()?;
        snapshot.committed = committed;
        snapshot.control = control;
        Ok(snapshot)
    }
}
impl Drop for Japanese {
    fn drop(&mut self) {
        self.romaji.take();
        unsafe {
            ffi::anthy_release_context(self.context);
            ffi::anthy_quit();
        }
    }
}
