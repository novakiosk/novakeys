pub mod timeouts {
    pub const POPUP_TIMER_MS: u64 = 200;
}

pub mod file_names {
    pub const STATUS_FILE: &str = "no.novaspektrum.novakeys.status.json";
}

pub mod geometry {
    pub const DEFAULT_WINDOW_HEIGHT: i32 = 260;
    pub const DEFAULT_KEYBOARD_WIDTH: i32 = 800;
    pub const MIN_KEY_HEIGHT: i32 = 30;
    pub const VERTICAL_PADDING_TOTAL: i32 = 8;
    pub const ROW_GAP: i32 = 1;
}

pub mod keyboard_actions {
    pub const BACKSPACE: &str = "backspace";
    pub const ENTER: &str = "enter";
    pub const SPACE: &str = "space";
    pub const SHIFT: &str = "shift";
    pub const LANGUAGE_SELECTOR: &str = "language_selector";
}

pub mod css_classes {
    pub const DARK_MODE: &str = "novakeys-dark";
    pub const CUSTOM_BUTTON: &str = "custom-button";
}
