use super::{Command, Mode, PAGE_SIZE, Snapshot, append_commit, copy_text, ffi, profiles::Paths};
use anyhow::{Result, ensure};
use std::{ffi::CString, fs};
pub(super) struct Chinese {
    session: usize,
    _paths: [CString; 2],
}
impl Chinese {
    pub fn new(paths: &Paths) -> Result<Self> {
        for file in [
            "default.yaml",
            "symbols.yaml",
            "essay.txt",
            "luna_pinyin.schema.yaml",
            "luna_pinyin_simp.schema.yaml",
        ] {
            ensure!(
                std::path::Path::new("/usr/share/rime-data")
                    .join(file)
                    .is_file(),
                "Missing Rime data: {file}; install the complete Luna Pinyin data packages"
            );
        }
        let user = paths.directory("rime")?;
        fs::write(
            user.join("default.custom.yaml"),
            "patch:\n  schema_list:\n    - schema: luna_pinyin_simp\n    - schema: luna_pinyin\n  menu/page_size: 6\n",
        )?;
        for schema in ["luna_pinyin", "luna_pinyin_simp"] {
            fs::write(
                user.join(format!("{schema}.custom.yaml")),
                "patch:\n  translator/enable_user_dict: false\n  custom_phrase/enable_user_dict: false\n  menu/page_size: 6\n",
            )?;
        }
        let cache = super::deployment::prepare(&user, &paths.cache, paths.cancellation.as_deref())?;
        let arguments = [
            CString::new(user.as_os_str().as_encoded_bytes())?,
            CString::new(cache.as_os_str().as_encoded_bytes())?,
        ];
        ensure!(
            unsafe { ffi::nk_rime_init(arguments[0].as_ptr(), arguments[1].as_ptr(), 0) } != 0,
            "Rime dictionaries could not be prepared with private learning disabled"
        );
        Ok(Self {
            session: 0,
            _paths: arguments,
        })
    }
    pub fn clear_session(&mut self) {
        if self.session != 0 {
            unsafe {
                ffi::nk_rime_delete(self.session);
            }
            self.session = 0;
        }
    }
    pub fn reset(&mut self, mode: Mode) -> Result<()> {
        self.clear_session();
        self.session = unsafe { ffi::nk_rime_new(i32::from(mode == Mode::Traditional)) };
        ensure!(self.session != 0, "Could not create Chinese input session");
        Ok(())
    }
    fn take_commit(&self) -> Result<String> {
        let pointer = unsafe { ffi::nk_rime_commit(self.session) };
        let result = unsafe { copy_text(pointer) };
        unsafe {
            ffi::nk_rime_free_text(pointer);
        }
        result
    }
    fn snapshot(&self) -> Result<Snapshot> {
        struct Context(*mut std::ffi::c_void);
        impl Drop for Context {
            fn drop(&mut self) {
                unsafe {
                    ffi::nk_rime_free_snapshot(self.0);
                }
            }
        }
        let context = Context(unsafe { ffi::nk_rime_snapshot(self.session) });
        ensure!(!context.0.is_null(), "Chinese input context unavailable");
        let (mut count, mut page, mut last, mut selected, mut cursor, mut start, mut end) =
            (0, 0, 0, 0, 0, 0, 0);
        unsafe {
            ffi::nk_rime_info(
                context.0,
                &mut count,
                &mut page,
                &mut last,
                &mut selected,
                &mut cursor,
                &mut start,
                &mut end,
            );
        }
        let preedit = unsafe { copy_text(ffi::nk_rime_preedit(context.0)) }?;
        let candidates = (0..count.max(0).min(PAGE_SIZE as i32))
            .map(|index| unsafe { copy_text(ffi::nk_rime_candidate(context.0, index as u32)) })
            .collect::<Result<Vec<_>>>()?;
        let cursor = (cursor.max(0) as usize).min(preedit.len());
        ensure!(
            preedit.is_char_boundary(cursor),
            "Invalid Chinese preedit cursor"
        );
        let selection = (start >= 0 && end >= start && (end as usize) <= preedit.len())
            .then_some((start as usize, end as usize));
        Ok(Snapshot {
            preedit,
            cursor,
            selection,
            selected: usize::try_from(selected)
                .ok()
                .filter(|index| *index < candidates.len()),
            candidates,
            page: page.max(0) as usize,
            has_previous: page > 0,
            has_next: count > 0 && last == 0,
            ..Snapshot::default()
        })
    }
    pub fn command(&mut self, command: &Command) -> Result<Snapshot> {
        let before = self.snapshot()?;
        let mut committed = String::new();
        let mut control = None;
        match command {
            Command::Reset => unsafe {
                ffi::nk_rime_clear(self.session);
            },
            Command::Text(text) => {
                super::check_input_limit(before.preedit.len() + text.len())?;
                for ch in text.chars() {
                    let ch = if matches!(ch, 'ü' | 'Ü') { 'v' } else { ch };
                    if !ch.is_ascii() {
                        unsafe {
                            ffi::nk_rime_finish(self.session);
                        }
                        append_commit(&mut committed, &self.take_commit()?)?;
                        append_commit(&mut committed, &ch.to_string())?;
                        continue;
                    }
                    let consumed = unsafe { ffi::nk_rime_key(self.session, ch as i32) } != 0;
                    append_commit(&mut committed, &self.take_commit()?)?;
                    if !consumed {
                        unsafe {
                            ffi::nk_rime_finish(self.session);
                        }
                        append_commit(&mut committed, &self.take_commit()?)?;
                        append_commit(&mut committed, &ch.to_string())?;
                    }
                }
            }
            Command::Select(index) => {
                ensure!(*index < before.candidates.len(), "Candidate has expired");
                ensure!(
                    unsafe { ffi::nk_rime_select(self.session, *index as u32) } != 0,
                    "Candidate has expired"
                );
            }
            Command::Finish => unsafe {
                ffi::nk_rime_finish(self.session);
            },
            other => {
                let key = match other {
                    Command::Backspace => 0xff08,
                    Command::Enter => 0xff0d,
                    Command::Space => 32,
                    Command::Escape => 0xff1b,
                    Command::Left => 0xff51,
                    Command::Right => 0xff53,
                    Command::Page(delta) => {
                        if *delta < 0 {
                            0xff55
                        } else {
                            0xff56
                        }
                    }
                    _ => 0,
                };
                if key != 0 {
                    let consumed = unsafe { ffi::nk_rime_key(self.session, key) } != 0;
                    if !consumed {
                        match other {
                            Command::Enter => control = Some('\n'),
                            Command::Backspace => control = Some('\u{0008}'),
                            Command::Escape => control = Some('\u{001b}'),
                            Command::Space => committed.push(' '),
                            _ => {}
                        }
                    }
                }
            }
        }
        append_commit(&mut committed, &self.take_commit()?)?;
        let mut snapshot = self.snapshot()?;
        snapshot.committed = committed;
        snapshot.control = control;
        Ok(snapshot)
    }
}
impl Drop for Chinese {
    fn drop(&mut self) {
        self.clear_session();
        unsafe {
            ffi::nk_rime_finalize();
        }
    }
}
