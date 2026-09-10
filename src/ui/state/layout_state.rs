//! The single active keyboard tree and its directly addressable controls.
use crate::{
    layout::parse::{LayoutDefinition, shifted_text},
    ui::components::{ActionButton, ButtonEX},
};
use relm4::gtk::{self, prelude::*};

#[derive(Default)]
pub struct LayoutState {
    pub shift_pressed: bool,
    pub widgets: Option<LayoutWidgets>,
}

pub struct LayoutWidgets {
    pub container: gtk::Box,
    pub rows: Vec<gtk::Box>,
    pub text_keys: Vec<(usize, usize, ButtonEX)>,
    pub shift_keys: Vec<gtk::ToggleButton>,
    pub action_keys: Vec<gtk::Button>,
    pub repeat_keys: Vec<ActionButton>,
    pub language_keys: Vec<gtk::Button>,
}

impl LayoutWidgets {
    pub fn new(container: gtk::Box) -> Self {
        Self {
            container,
            rows: Vec::new(),
            text_keys: Vec::new(),
            shift_keys: Vec::new(),
            language_keys: Vec::new(),
            repeat_keys: Vec::new(),
            action_keys: Vec::new(),
        }
    }
    pub fn cancel_interaction(&self) {
        for button in self.action_keys.iter().chain(&self.language_keys).chain(
            self.shift_keys
                .iter()
                .map(|key| key.upcast_ref::<gtk::Button>()),
        ) {
            let controllers = button.observe_controllers();
            for i in 0..controllers.n_items() {
                if let Some(controller) = controllers
                    .item(i)
                    .and_then(|item| item.downcast::<gtk::EventController>().ok())
                {
                    controller.reset();
                }
            }
        }
        for (_, _, key) in &self.text_keys {
            key.cancel_interaction();
        }
        for key in &self.repeat_keys {
            key.cancel_interaction();
        }
    }
    pub fn refresh(&self, layout: &LayoutDefinition, shift: bool, native: bool) {
        for (row, key, button) in &self.text_keys {
            let key = &layout.layout[*row][*key];
            let label = key.display_text.clone().unwrap_or_else(|| {
                if layout.language_code.as_deref() == Some("ko") && !native {
                    return crate::ui::input_handler::direct_korean_text(
                        key.text.as_deref().unwrap_or_default(),
                        shift,
                    );
                }
                shifted_text(
                    key.text.as_deref().unwrap_or_default(),
                    shift,
                    layout.language_code.as_deref(),
                )
            });
            let alternatives: Vec<String> = key
                .get_alternatives()
                .iter()
                .map(|text| {
                    if layout.language_code.as_deref() == Some("ko") && !native {
                        crate::ui::input_handler::direct_korean_text(text, shift)
                    } else {
                        shifted_text(text, shift, layout.language_code.as_deref())
                    }
                })
                .collect();
            if button.alternatives() != alternatives {
                button.set_alternatives(alternatives);
            }
            if button.primary_content().as_deref() != Some(&label) {
                button.set_primary_content(label);
            }
        }
        for key in &self.shift_keys {
            key.set_active(shift);
        }
    }
}

impl Drop for LayoutWidgets {
    fn drop(&mut self) {
        self.cancel_interaction();
        if let Some(parent) = self
            .container
            .parent()
            .and_then(|parent| parent.downcast::<gtk::Box>().ok())
        {
            parent.remove(&self.container);
        }
    }
}
