use crate::ui::main_view::InputSender;
use relm4::gtk;
use relm4::gtk::prelude::{BoxExt, WidgetExt};

use super::{state::LayoutWidgets, widget_factory::KeyboardWidgetFactory};
use crate::layout::parse::{Key, LayoutDefinition};

pub struct KeyboardLayoutParams<'a> {
    pub container: &'a gtk::Box,
    pub keyboard_definition: &'a LayoutDefinition,
    pub geometry_unit: i32,
    pub keyboard_width: i32,
    pub sender: &'a InputSender,
    pub shift_pressed: bool,
    pub current_language_flag: String,
    pub show_language_switcher: bool,
}

pub fn build_keyboard_layout(params: KeyboardLayoutParams<'_>) -> LayoutWidgets {
    let mut widgets = LayoutWidgets::new(params.container.clone());
    let keyboard_width = params.keyboard_width;

    params
        .keyboard_definition
        .layout
        .iter()
        .enumerate()
        .for_each(|(row_index, row)| {
            let row_container = create_new_row_container();

            let mut effective_keys: Vec<&Key> = Vec::new();
            let mut total_width_units = 0.0f32;
            let mut key_indices = Vec::new();

            for (original_index, key) in row.iter().enumerate() {
                if let Some(action) = &key.action
                    && action == "language_selector"
                    && !params.show_language_switcher
                {
                    continue;
                }
                effective_keys.push(key);
                key_indices.push(original_index);
                total_width_units += key.get_width() as f32;
            }

            let row_total_width = total_width_units.max(1.0);
            let spacing = row_container.spacing();
            let gaps = spacing * (effective_keys.len() as i32).saturating_sub(1);
            let available_width = keyboard_width - gaps;

            let mut total_allocated = 0;
            let key_count = effective_keys.len();

            effective_keys
                .iter()
                .enumerate()
                .for_each(|(effective_index, key)| {
                    let original_key_index = key_indices[effective_index];
                    let key_width_ratio = key.get_width() as f32 / row_total_width;
                    let mut width = (available_width as f32 * key_width_ratio) as i32;

                    if effective_index == key_count.saturating_sub(1) {
                        width = available_width - total_allocated;
                    } else {
                        total_allocated += width;
                    }

                    create_widget_for_key(
                        &mut widgets,
                        KeyWidgetParams {
                            row_container: &row_container,
                            key,
                            width,
                            height: params.geometry_unit,
                            row_index,
                            key_index: original_key_index,
                            sender: params.sender,
                            shift_pressed: params.shift_pressed,
                            current_language_flag: &params.current_language_flag,
                        },
                    );
                });

            params.container.append(&row_container);
            widgets.rows.push(row_container.clone());

            row_container.set_width_request(keyboard_width);
            row_container.set_halign(gtk::Align::Center);
            row_container.set_homogeneous(false);
        });
    widgets
}

fn create_new_row_container() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .homogeneous(false)
        .spacing(1)
        .build()
}

struct KeyWidgetParams<'a> {
    row_container: &'a gtk::Box,
    key: &'a Key,
    width: i32,
    height: i32,
    row_index: usize,
    key_index: usize,
    sender: &'a InputSender,
    shift_pressed: bool,
    current_language_flag: &'a str,
}

fn create_widget_for_key(widgets: &mut LayoutWidgets, params: KeyWidgetParams) {
    if let Some(action) = &params.key.action {
        match action.as_str() {
            "shift" => {
                let toggle = KeyboardWidgetFactory::create_shift_toggle(
                    &params.key.get_display_text(),
                    params.width,
                    params.height,
                    params.shift_pressed,
                    params.sender,
                );
                params.row_container.append(&toggle);
                widgets.shift_keys.push(toggle);
            }
            "backspace" => {
                let button = KeyboardWidgetFactory::create_repeating_action_button(
                    action,
                    &params.key.get_display_text(),
                    params.width,
                    params.height,
                    params.sender,
                );
                params.row_container.append(&button);
                widgets.repeat_keys.push(button);
            }
            "language_selector" => {
                let flag = params
                    .key
                    .get_display_text_with_flag(Some(params.current_language_flag));
                let button = KeyboardWidgetFactory::create_language_selector_button(
                    &flag,
                    params.width,
                    params.height,
                    params.sender,
                );
                params.row_container.append(&button);
                widgets.language_keys.push(button);
            }
            _ => {
                let button = KeyboardWidgetFactory::create_action_button(
                    action,
                    &params.key.get_display_text(),
                    params.width,
                    params.height,
                    params.sender,
                );
                params.row_container.append(&button);
                widgets.action_keys.push(button);
            }
        }
    } else if params.key.is_text_key() {
        let button = KeyboardWidgetFactory::create_text_button(
            params.key,
            params.width,
            params.height,
            params.row_index,
            params.key_index,
            params.sender,
        );
        params.row_container.append(&button);
        widgets
            .text_keys
            .push((params.row_index, params.key_index, button));
    } else {
        let spacer = KeyboardWidgetFactory::create_spacer(params.width, params.height);
        params.row_container.append(&spacer);
    }
}
