pub(crate) mod components;
pub(crate) mod composition;
mod composition_view;
pub(crate) mod input_handler;
pub(crate) mod language_selector;
pub(crate) mod layout;
pub(crate) mod layout_builder;
pub mod main_view;
pub(crate) mod message_router;
mod overlay_window;
pub(crate) mod state;
pub(crate) mod status;
pub(crate) mod style;
pub(crate) mod visibility_controller;
pub(crate) mod widget_factory;
pub(crate) mod window_setup;

pub use main_view::*;
pub use visibility_controller::UIVisibilityController;

#[cfg(test)]
mod gtk_lifecycle;
