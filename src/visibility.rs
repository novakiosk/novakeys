//! Bridge committed input-method contexts into GTK visibility and composition resets.
use crate::input_detection::{InputContext, InputDetectionManager, TextInputHandler};
use std::sync::{Arc, Mutex};
pub trait VisibilityManager: Send + Sync {
    fn set_controller(&self, controller: Box<dyn VisibilityController + Send>);
}
pub trait VisibilityController: Send {
    fn context_changed(&self, context: InputContext);
}
#[derive(Default)]
struct VisibilityState {
    context: InputContext,
    controller: Option<Box<dyn VisibilityController + Send>>,
}
pub struct KeyboardVisibilityManager {
    state: Arc<Mutex<VisibilityState>>,
}
impl KeyboardVisibilityManager {
    pub fn new(input: &InputDetectionManager) -> Self {
        let state = Arc::new(Mutex::new(VisibilityState::default()));
        input.set_handler(Box::new(VisibilityHandler(state.clone())));
        Self { state }
    }
}
impl VisibilityManager for KeyboardVisibilityManager {
    fn set_controller(&self, controller: Box<dyn VisibilityController + Send>) {
        let mut state = self.state.lock().unwrap();
        controller.context_changed(state.context);
        state.controller = Some(controller);
    }
}
struct VisibilityHandler(Arc<Mutex<VisibilityState>>);
impl TextInputHandler for VisibilityHandler {
    fn on_context_changed(&mut self, context: InputContext) {
        let mut state = self.0.lock().unwrap();
        state.context = context;
        if let Some(controller) = &state.controller {
            controller.context_changed(context);
        }
    }
}
