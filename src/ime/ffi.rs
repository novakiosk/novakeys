//! Native declarations for the serialized engine worker and dictionary helper process.
use std::ffi::{c_char, c_int, c_void};
#[repr(C)]
pub struct AnthyInputPreedit {
    pub state: c_int,
    pub commit: *const c_char,
    pub cut: *const c_char,
    pub segment: *const AnthyInputSegment,
    pub current: *const AnthyInputSegment,
}
#[repr(C)]
pub struct AnthyInputSegment {
    pub text: *const c_char,
    pub candidate: c_int,
    pub noconv: c_int,
    pub count: c_int,
    pub flags: c_int,
    pub next: *const AnthyInputSegment,
}
#[repr(C)]
pub struct AnthyStat {
    pub segments: c_int,
}
#[repr(C)]
pub struct AnthySegment {
    pub candidates: c_int,
    pub length: c_int,
}
unsafe extern "C" {
    pub fn nk_rime_version() -> *const c_char;
    pub fn nk_rime_init(user: *const c_char, cache: *const c_char, deploy: c_int) -> c_int;
    pub fn nk_rime_finalize();
    pub fn nk_rime_new(traditional: c_int) -> usize;
    pub fn nk_rime_delete(session: usize);
    pub fn nk_rime_key(session: usize, key: c_int) -> c_int;
    pub fn nk_rime_select(session: usize, index: u32) -> c_int;
    pub fn nk_rime_finish(session: usize) -> c_int;
    pub fn nk_rime_clear(session: usize);
    // Commit strings need nk_rime_free_text. Preedit/candidate strings borrow the
    // snapshot; copy text that must outlive it before nk_rime_free_snapshot.
    pub fn nk_rime_commit(session: usize) -> *mut c_char;
    pub fn nk_rime_free_text(text: *mut c_char);
    pub fn nk_rime_snapshot(session: usize) -> *mut c_void;
    pub fn nk_rime_free_snapshot(context: *mut c_void);
    pub fn nk_rime_preedit(context: *mut c_void) -> *const c_char;
    pub fn nk_rime_candidate(context: *mut c_void, index: u32) -> *const c_char;
    pub fn nk_rime_info(
        context: *mut c_void,
        count: *mut c_int,
        page: *mut c_int,
        last: *mut c_int,
        selected: *mut c_int,
        cursor: *mut c_int,
        start: *mut c_int,
        end: *mut c_int,
    );
    pub fn anthy_conf_override(name: *const c_char, value: *const c_char);
    pub fn anthy_input_init() -> c_int;
    pub fn anthy_input_create_config() -> *mut c_void;
    pub fn anthy_input_free_config(config: *mut c_void);
    pub fn anthy_input_edit_rk_config(
        config: *mut c_void,
        map: c_int,
        from: *const c_char,
        to: *const c_char,
        follow: *const c_char,
    ) -> c_int;
    pub fn anthy_input_change_config(config: *mut c_void);
    pub fn anthy_input_create_context(config: *mut c_void) -> *mut c_void;
    pub fn anthy_input_free_context(context: *mut c_void);
    pub fn anthy_input_map_select(context: *mut c_void, map: c_int) -> c_int;
    pub fn anthy_input_erase_prev(context: *mut c_void);
    pub fn anthy_input_move(context: *mut c_void, delta: c_int);
    pub fn anthy_input_str(context: *mut c_void, text: *const c_char);
    pub fn anthy_input_get_state(context: *mut c_void) -> c_int;
    pub fn anthy_input_get_preedit(context: *mut c_void) -> *mut AnthyInputPreedit;
    pub fn anthy_input_free_preedit(preedit: *mut AnthyInputPreedit);
    pub fn anthy_input_commit(context: *mut c_void);
    pub fn anthy_quit();
    pub fn anthy_create_context() -> *mut c_void;
    pub fn anthy_release_context(context: *mut c_void);
    pub fn anthy_reset_context(context: *mut c_void);
    pub fn anthy_context_set_encoding(context: *mut c_void, encoding: c_int) -> c_int;
    pub fn anthy_set_string(context: *mut c_void, text: *const c_char) -> c_int;
    pub fn anthy_get_stat(context: *mut c_void, stat: *mut AnthyStat) -> c_int;
    pub fn anthy_get_segment_stat(
        context: *mut c_void,
        segment: c_int,
        stat: *mut AnthySegment,
    ) -> c_int;
    pub fn anthy_get_segment(
        context: *mut c_void,
        segment: c_int,
        candidate: c_int,
        buffer: *mut c_char,
        length: c_int,
    ) -> c_int;
    pub fn anthy_resize_segment(context: *mut c_void, segment: c_int, delta: c_int);
    pub fn hangul_ic_new(keyboard: *const c_char) -> *mut c_void;
    pub fn hangul_ic_delete(context: *mut c_void);
    pub fn hangul_ic_process(context: *mut c_void, key: c_int) -> bool;
    pub fn hangul_ic_backspace(context: *mut c_void) -> bool;
    pub fn hangul_ic_reset(context: *mut c_void);
    pub fn hangul_ic_get_preedit_string(context: *mut c_void) -> *const u32;
    pub fn hangul_ic_get_commit_string(context: *mut c_void) -> *const u32;
    pub fn hangul_ic_flush(context: *mut c_void) -> *const u32;
    pub fn hanja_table_load(path: *const c_char) -> *mut c_void;
    pub fn hanja_table_delete(table: *mut c_void);
    pub fn hanja_table_match_exact(table: *mut c_void, text: *const c_char) -> *mut c_void;
    pub fn hanja_list_get_size(list: *mut c_void) -> c_int;
    pub fn hanja_list_get_nth_value(list: *mut c_void, index: u32) -> *const c_char;
    pub fn hanja_list_delete(list: *mut c_void);
}
