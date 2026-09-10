//! Physical key events for Return, Tab, Escape and Backspace.
mod virtual_keyboard;
pub use virtual_keyboard::VirtualKeyboardV1;
pub(crate) use virtual_keyboard::control_code;
