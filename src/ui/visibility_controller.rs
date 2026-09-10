use crate::{
    ui::{UIMessage, UIModel},
    visibility::VisibilityController,
};
use relm4::ComponentSender;

pub struct UIVisibilityController {
    sender: ComponentSender<UIModel>,
}

impl UIVisibilityController {
    pub fn new(sender: ComponentSender<UIModel>) -> Self {
        Self { sender }
    }
}

impl VisibilityController for UIVisibilityController {
    fn context_changed(&self, context: crate::input_detection::InputContext) {
        self.sender.input(UIMessage::ContextChanged(context));
    }
}
