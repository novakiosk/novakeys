//! Transactional language/configuration changes and ownership of the active tree.
use super::{
    UIModel,
    layout_builder::{self, KeyboardLayoutParams},
    window_setup,
};
use crate::ui::main_view::InputSender;
use crate::{config::AppConfig, constants::css_classes};
use relm4::gtk::{self, prelude::*};

impl UIModel {
    pub(super) fn refresh_keys(&self) {
        if let Some(widgets) = &self.layout_state.widgets {
            widgets.refresh(
                &self.app_config.get_language_manager().current_layout(),
                self.layout_state.shift_pressed,
                self.native_key_input(),
            );
        }
    }
    pub(super) fn reset_input(&mut self) {
        self.input_epoch.set(self.input_epoch.get().wrapping_add(1));
        if let Some(popover) = self.language_selector_popover.borrow().as_ref() {
            super::components::clear_popover_focus(popover);
            popover.popdown();
        }
        if let Some(widgets) = &self.layout_state.widgets {
            widgets.cancel_interaction();
        }
        if let Some(view) = &self.composition.bar {
            view.cancel_interaction();
        }
        self.reset_composition();
        self.layout_state.shift_pressed = false;
        self.refresh_keys();
    }
    pub(super) fn rebuild_keyboard(&mut self, sender: &InputSender) {
        super::overlay_window::set_visible_top(&self.window, None);
        self.input_epoch.set(self.input_epoch.get().wrapping_add(1));
        self.composition.bar.take();
        self.layout_state.widgets.take();
        let layout = self.app_config.get_language_manager().current_layout();
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_hexpand(true);
        container.set_halign(gtk::Align::Fill);
        let widgets = layout_builder::build_keyboard_layout(KeyboardLayoutParams {
            container: &container,
            keyboard_definition: &layout,
            geometry_unit: super::style::css_utils::cal_geometry_unit(
                self.window_height,
                layout.height,
            ),
            keyboard_width: self.app_config.keyboard_width(),
            sender,
            shift_pressed: self.layout_state.shift_pressed,
            current_language_flag: self
                .app_config
                .get_language_manager()
                .get_current_language_flag()
                .to_owned(),
            show_language_switcher: self
                .app_config
                .get_language_manager()
                .should_show_language_switcher(),
        });
        self.container.append(&container);
        self.layout_state.widgets = Some(widgets);
        self.refresh_keys();
        self.start_composition(sender);
    }
    pub(super) fn change_language(
        &mut self,
        code: &str,
        sender: &InputSender,
    ) -> Result<(), String> {
        self.app_config
            .get_language_manager()
            .layout(code)
            .ok_or("Unknown language")?;
        if code == self.current_language_code() {
            return Ok(());
        }
        self.reset_input();
        self.app_config
            .get_language_manager_mut()
            .set_current_language(code);
        self.composition.mode = crate::ime::Mode::Native;
        self.rebuild_keyboard(sender);
        Ok(())
    }
    pub(super) fn set_switcher_visibility(
        &mut self,
        show: bool,
        sender: &InputSender,
    ) -> Result<(), String> {
        if self
            .app_config
            .get_language_manager()
            .should_show_language_switcher()
            == show
        {
            return Ok(());
        }
        if !self.composition.snapshot.preedit.is_empty()
            || self.composition.pending > 0
            || self.composition.failed_output.is_some()
            || !self.composition.deferred.is_empty()
            || !self.composition.transition_input.is_empty()
        {
            return Err(
                "Finish or explicitly discard current input before changing the language switcher"
                    .into(),
            );
        }
        let manager = self.app_config.get_language_manager_mut();
        let old = manager.should_show_language_switcher();
        manager.set_language_switcher_visibility(show);
        if old == manager.should_show_language_switcher() {
            return Ok(());
        }
        self.reset_composition();
        if let Some(popover) = self.language_selector_popover.borrow().as_ref() {
            super::components::clear_popover_focus(popover);
            popover.popdown();
        }
        self.rebuild_keyboard(sender);
        Ok(())
    }
    pub(super) fn handle_config_reload(&mut self, sender: &InputSender) -> Result<(), String> {
        if !self.composition.snapshot.preedit.is_empty()
            || self.composition.pending > 0
            || self.composition.failed_output.is_some()
            || !self.composition.deferred.is_empty()
            || !self.composition.transition_input.is_empty()
        {
            return Err(
                "Finish or explicitly discard current input before reloading configuration".into(),
            );
        }
        let config = AppConfig::with_config_loaded().map_err(|e| e.to_string())?;
        let providers = window_setup::setup_css_providers(&config).map_err(|e| e.to_string())?;
        if let Some(popover) = self.language_selector_popover.borrow_mut().take() {
            super::components::clear_popover_focus(&popover);
            popover.popdown();
            popover.unparent();
        }
        self.reset_input();
        self.layout_state.widgets.take();
        if self.current_language_code() != config.get_language_manager().get_current_language() {
            self.composition.mode = crate::ime::Mode::Native;
        }
        self.app_config = config;
        if let Some(display) = gtk::gdk::Display::default() {
            for provider in self.css_providers.drain(..) {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        }
        window_setup::install_css_providers(&providers);
        self.css_providers = providers;
        let (height, width, stretch) =
            window_setup::configure_window_layout(&self.window, &self.app_config);
        self.window_height = height;
        self.container = window_setup::create_container_hierarchy(&self.window, width, stretch);
        if self.app_config.is_dark_mode() {
            self.window.add_css_class(css_classes::DARK_MODE);
        } else {
            self.window.remove_css_class(css_classes::DARK_MODE);
        }
        self.rebuild_keyboard(sender);
        Ok(())
    }
}
