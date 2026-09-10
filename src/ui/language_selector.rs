//! Language selector popover UI.

use crate::ui::main_view::InputSender;
use relm4::gtk;
use relm4::gtk::Grid;
use relm4::gtk::prelude::*;

use super::UIModel;
use crate::types::LanguageCode;
use crate::ui::UIInput;

pub fn show_language_selector(model: &UIModel, sender: &InputSender) {
    let language_manager = model.app_config_ref().get_language_manager();

    let available_count = language_manager.get_available_languages().len();
    if available_count <= 1 {
        return;
    }

    if model
        .layout_state
        .widgets
        .as_ref()
        .is_some_and(|widgets| !widgets.language_keys.is_empty())
    {
        let popover = get_or_create_language_popover(model, sender);
        let Some(bounds) = model
            .layout_state
            .widgets
            .as_ref()
            .and_then(|widgets| widgets.rows.last())
            .and_then(|row| row.compute_bounds(model.window_ref()))
        else {
            return;
        };
        // Refresh the cached popup from the live key grid, not a prior window size.
        // Top placement at the bottom edge overlays the grid on every layout.
        let anchor = gtk::gdk::Rectangle::new(
            (bounds.x() + bounds.width() / 2.0).round() as i32,
            (bounds.y() + bounds.height()).round() as i32,
            1,
            1,
        );
        popover.set_pointing_to(Some(&anchor));
        popover.popup();
    }
}

fn get_or_create_language_popover(model: &UIModel, sender: &InputSender) -> gtk::Popover {
    let mut popover_ref = model.language_selector_popover_ref().borrow_mut();

    if let Some(ref popover) = *popover_ref {
        update_current_language_visibility(model, popover);
        return popover.clone();
    }

    let popover = gtk::Popover::new();
    popover.set_parent(model.window_ref());
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);
    popover.set_autohide(true);
    popover.add_css_class("language-selector");
    // Popup CSS also matches the theme class directly, outside the window selector.
    if model.window_ref().has_css_class("novakeys-dark") {
        popover.add_css_class("novakeys-dark");
    }

    let grid = Grid::new();
    grid.set_margin_top(4);
    grid.set_margin_bottom(4);
    grid.set_margin_start(4);
    grid.set_margin_end(4);

    let all_languages = model
        .app_config_ref()
        .get_language_manager()
        .get_available_languages();
    let mut sorted_languages: Vec<_> = all_languages.iter().collect();
    sorted_languages.sort_by(|a, b| a.name.cmp(&b.name));

    for (index, lang_info) in sorted_languages.iter().enumerate() {
        let row = index / 4;
        let col = index % 4;

        let lang_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let flag_label = gtk::Label::builder()
            .label(&lang_info.flag)
            .width_request(24)
            .halign(gtk::Align::Center)
            .build();

        let name_label = gtk::Label::builder()
            .label(&lang_info.name)
            .halign(gtk::Align::Start)
            .build();

        lang_container.append(&flag_label);
        lang_container.append(&name_label);

        let lang_button = gtk::Button::builder().child(&lang_container).build();

        lang_button.add_css_class(&format!("language-{}", lang_info.code));

        lang_button.add_css_class("novakeys-language-button");

        if model.window_ref().has_css_class("novakeys-dark") {
            lang_button.add_css_class("novakeys-dark");
        }

        let sender_clone = sender.clone();
        let lang_code = lang_info.code.clone();
        let popover_weak = popover.downgrade();

        lang_button.connect_clicked(move |_| {
            sender_clone.emit(UIInput::SetLanguage(LanguageCode::from(lang_code.clone())));
            if let Some(popover_ref) = popover_weak.upgrade() {
                super::components::clear_popover_focus(&popover_ref);
                popover_ref.set_visible(false);
            }
        });

        grid.attach(&lang_button, col as i32, row as i32, 1, 1);
    }

    popover.set_child(Some(&grid));

    update_current_language_visibility(model, &popover);

    *popover_ref = Some(popover.clone());
    popover
}

fn update_current_language_visibility(model: &UIModel, popover: &gtk::Popover) {
    if let Some(grid) = popover
        .child()
        .and_then(|child| child.downcast::<Grid>().ok())
    {
        let current_language = model.current_language_code();
        let current_language_class = format!("language-{}", current_language);

        let mut child = grid.first_child();
        while let Some(widget) = child {
            let next_sibling = widget.next_sibling();

            if let Ok(button) = widget.downcast::<gtk::Button>() {
                button.set_visible(true);

                if !button.has_css_class("novakeys-language-button") {
                    button.add_css_class("novakeys-language-button");
                }

                if model.window_ref().has_css_class("novakeys-dark")
                    && !button.has_css_class("novakeys-dark")
                {
                    button.add_css_class("novakeys-dark");
                }

                if button.has_css_class(&current_language_class) {
                    button.set_sensitive(false);
                    button.add_css_class("current-language");
                    if let Some(child_widget) = button.child()
                        && let Some(container) = child_widget.downcast_ref::<gtk::Box>()
                    {
                        let mut has_indicator = false;
                        let mut container_child = container.first_child();
                        while let Some(container_widget) = container_child {
                            let next_container_sibling = container_widget.next_sibling();
                            if let Some(label) = container_widget.downcast_ref::<gtk::Label>()
                                && label.has_css_class("current-indicator")
                            {
                                has_indicator = true;
                                break;
                            }
                            container_child = next_container_sibling;
                        }

                        if !has_indicator {
                            let indicator = gtk::Label::builder()
                                .label("\u{2713}")
                                .margin_start(4)
                                .build();
                            indicator.add_css_class("current-indicator");
                            container.append(&indicator);
                        }
                    }
                } else {
                    button.set_sensitive(true);
                    button.remove_css_class("current-language");
                    if let Some(child_widget) = button.child()
                        && let Some(container) = child_widget.downcast_ref::<gtk::Box>()
                    {
                        let mut container_child = container.first_child();
                        while let Some(container_widget) = container_child {
                            let next_container_sibling = container_widget.next_sibling();
                            if let Some(label) = container_widget.downcast_ref::<gtk::Label>()
                                && label.has_css_class("current-indicator")
                            {
                                container.remove(&container_widget);
                            }
                            container_child = next_container_sibling;
                        }
                    }
                }
            }

            child = next_sibling;
        }
    }
}
