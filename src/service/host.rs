use relm4::RelmApp;
use std::sync::Arc;

use crate::{
    config::AppConfig,
    ui::{UIMessage, UIModel},
    visibility::VisibilityManager,
};

use crate::ipc::Ipc;

/// Whether a failed delivery definitely sent nothing or may have reached the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    Rejected(String),
    OutcomeUnknown(String),
}
impl DeliveryError {
    pub fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}
impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(message) | Self::OutcomeUnknown(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for DeliveryError {}

/// Text delivery and physical controls for the active input context.
pub trait KeyboardHandle {
    /// Commit literal UTF-8; the native backend uses input-method-v2, not key presses.
    fn insert_text(&mut self, text: &str) -> Result<(), DeliveryError>;

    /// Native backends send a physical control key; the default inserts its character.
    fn control(&mut self, key: char) -> Result<(), DeliveryError> {
        self.insert_text(&key.to_string())
    }

    /// Send before_length Backspaces; the native backend rejects nonzero after_length.
    fn delete_text(&mut self, before_length: u32, after_length: u32) -> Result<(), DeliveryError>;

    fn set_context(&mut self, _context: crate::input_detection::InputContext) {}

    fn destroy(&mut self);
}

pub struct AppService<M: KeyboardHandle + 'static> {
    ui_handle: RelmApp<UIMessage>,
    keyboard_handle: M,
    ipc_handle: Ipc,
    app_config: AppConfig,
    visibility_manager: Arc<dyn VisibilityManager>,
}

impl<M: KeyboardHandle + 'static> AppService<M> {
    pub fn new(
        keyboard_handle: M,
        ipc_handle: Ipc,
        app_config: AppConfig,
        visibility_manager: Arc<dyn VisibilityManager>,
    ) -> Self {
        // Clap already consumed our arguments; do not let GTK parse them again.
        let ui = RelmApp::new("no.novaspektrum.novakeys").with_args(vec![]);

        Self {
            ui_handle: ui,
            keyboard_handle,
            ipc_handle,
            app_config,
            visibility_manager,
        }
    }

    pub fn run(self) {
        let ui_init_data = (
            Box::new(self.keyboard_handle) as Box<dyn KeyboardHandle>,
            self.ipc_handle,
            self.visibility_manager.clone(),
            self.app_config,
        );

        self.ui_handle.run::<UIModel>(ui_init_data);
    }
}
