use crate::ui::main_view::InputSender;
use relm4::gtk;
use relm4::gtk::prelude::{ButtonExt, ObjectExt, ToggleButtonExt, WidgetExt};

use crate::{
    constants::keyboard_actions,
    layout::parse::Key,
    types::{KeyIndex, RowIndex},
    ui::{
        UIInput,
        components::{ActionButton, ButtonEX},
    },
};

pub struct KeyboardWidgetFactory;

impl KeyboardWidgetFactory {
    pub fn create_action_button(
        action: &str,
        display_text: &str,
        width: i32,
        height: i32,
        sender: &InputSender,
    ) -> gtk::Button {
        let button = gtk::Button::builder()
            .label(display_text)
            .width_request(width)
            .height_request(height)
            .build();
        let sender = sender.clone();
        let action = action.to_owned();
        button.connect_clicked(move |_| sender.emit(UIInput::ActionKey(action.clone())));
        button
    }

    pub fn create_repeating_action_button(
        action: &str,
        display_text: &str,
        width: i32,
        height: i32,
        sender: &InputSender,
    ) -> ActionButton {
        let button = ActionButton::new(action, display_text);
        button.set_size_request(width, height);
        let sender = sender.clone();
        let action = action.to_owned();
        button.connect_local("activated", false, move |_| {
            sender.emit(UIInput::ActionKey(action.clone()));
            None
        });
        button
    }

    pub fn create_shift_toggle(
        display_text: &str,
        width: i32,
        height: i32,
        is_active: bool,
        sender: &InputSender,
    ) -> gtk::ToggleButton {
        let toggle = gtk::ToggleButton::builder()
            .label(display_text)
            .width_request(width)
            .height_request(height)
            .build();

        toggle.add_css_class("key-shift");
        toggle.set_active(is_active);

        let action_sender = sender.clone();
        toggle.connect_clicked(move |_btn| {
            action_sender.emit(UIInput::ActionKey(keyboard_actions::SHIFT.to_string()));
        });

        toggle
    }

    pub fn create_language_selector_button(
        flag_text: &str,
        width: i32,
        height: i32,
        sender: &InputSender,
    ) -> gtk::Button {
        let button = Self::create_action_button(
            keyboard_actions::LANGUAGE_SELECTOR,
            flag_text,
            width,
            height,
            sender,
        );
        button.add_css_class("language-selector-button");
        button.add_css_class("language-flag-button");
        button
    }

    pub fn create_text_button(
        key: &Key,
        width: i32,
        height: i32,
        row_index: usize,
        key_index: usize,
        sender: &InputSender,
    ) -> ButtonEX {
        let display_text = key.get_display_text().into_owned();
        let button = ButtonEX::default();
        button.set_primary_content(display_text);
        button.set_size_request(width, height);

        let alternatives = key.get_alternatives();
        if !alternatives.is_empty() {
            button.set_alternatives(alternatives.to_vec());
        }

        let text_sender = sender.clone();
        button.connect_local("pressed", true, move |_| {
            text_sender.emit(UIInput::TextKeyAtPosition {
                row_index: RowIndex::from(row_index),
                key_index: KeyIndex::from(key_index),
            });
            None
        });

        let alt_sender = sender.clone();
        button.connect_local("alternative-selected", true, move |values| {
            if let Some(alt) = values.get(1).and_then(|v| v.get::<String>().ok()) {
                alt_sender.emit(UIInput::AlternativeSelected(alt));
            }
            None
        });

        button
    }

    pub fn create_spacer(width: i32, height: i32) -> gtk::Box {
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_width_request(width);
        spacer.set_height_request(height);
        spacer
    }
}
