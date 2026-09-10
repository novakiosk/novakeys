use std::{cell::RefCell, sync::OnceLock, time::Duration};

use relm4::gtk::{
    glib::{self, Properties, timeout_add_local},
    prelude::*,
    subclass::prelude::*,
};

use crate::constants::{css_classes, timeouts};

#[derive(Debug, Default, Properties)]
#[properties(wrapper_type = ButtonEX)]
pub struct ButtonInner {
    #[property(get, set)]
    primary_content: RefCell<Option<String>>,
    #[property(get, set)]
    alternatives: RefCell<Vec<String>>,
    primary_label: RefCell<Option<relm4::gtk::Label>>,
    popup_timer: RefCell<Option<glib::SourceId>>,
    is_pressed: RefCell<bool>,
    popup_shown: RefCell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ButtonInner {
    const NAME: &'static str = "ExButton";
    type Type = ButtonEX;
    type ParentType = relm4::gtk::Widget;

    fn class_init(class: &mut Self::Class) {
        class.set_layout_manager_type::<relm4::gtk::BinLayout>();

        class.set_css_name(css_classes::CUSTOM_BUTTON);

        class.set_accessible_role(relm4::gtk::AccessibleRole::Button);
    }
}

#[glib::derived_properties]
impl ObjectImpl for ButtonInner {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        let primary = relm4::gtk::Label::new(None);
        primary.set_valign(relm4::gtk::Align::Center);
        primary.set_parent(&*obj);
        *self.primary_label.borrow_mut() = Some(primary);
        obj.update_view();

        obj.connect_primary_content_notify(|obj| {
            obj.update_view();
        });

        let gesture = relm4::gtk::GestureClick::new();

        let obj_press_cb = obj.downgrade();
        gesture.connect_pressed(move |gesture, _, _, _| {
            let Some(obj_press_cb) = obj_press_cb.upgrade() else {
                return;
            };
            gesture.set_state(relm4::gtk::EventSequenceState::Claimed);

            *obj_press_cb.imp().is_pressed.borrow_mut() = true;
            *obj_press_cb.imp().popup_shown.borrow_mut() = false;

            obj_press_cb.cancel_popup_timer();

            let alternatives = obj_press_cb.alternatives();

            if alternatives.is_empty() {
                obj_press_cb.emit_by_name::<()>("pressed", &[]);
            } else {
                obj_press_cb.start_popup_timer();
            }
        });

        let obj_release_cb = obj.downgrade();
        gesture.connect_released(move |gesture, _, _, _| {
            let Some(obj_release_cb) = obj_release_cb.upgrade() else {
                return;
            };
            gesture.set_state(relm4::gtk::EventSequenceState::Claimed);

            let was_pressed = *obj_release_cb.imp().is_pressed.borrow();
            let popup_shown = *obj_release_cb.imp().popup_shown.borrow();

            *obj_release_cb.imp().is_pressed.borrow_mut() = false;
            obj_release_cb.cancel_popup_timer();

            // Alternative keys defer their primary text until release so a long press
            // opens the popup without also inserting the primary character.
            if was_pressed && !popup_shown && !obj_release_cb.alternatives().is_empty() {
                obj_release_cb.emit_by_name::<()>("pressed", &[]);
            }

            obj_release_cb.emit_by_name::<()>("released", &[]);
        });

        let weak = obj.downgrade();
        gesture.connect_cancel(move |_, _| {
            if let Some(obj) = weak.upgrade() {
                *obj.imp().is_pressed.borrow_mut() = false;
                obj.cancel_popup_timer();
            }
        });
        obj.add_controller(gesture);
    }

    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("pressed").build(),
                glib::subclass::Signal::builder("released").build(),
                glib::subclass::Signal::builder("alternative-selected")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn dispose(&self) {
        self.obj().cancel_popup_timer();

        self.obj().cleanup_all_popovers();

        self.primary_label.borrow_mut().take();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ButtonInner {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);
        let mut child = self.obj().first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(popover) = widget.downcast::<relm4::gtk::Popover>() {
                popover.present();
            }
        }
    }

    fn unmap(&self) {
        self.obj().cancel_interaction();
        self.parent_unmap();
    }
}

glib::wrapper! {
    pub struct ButtonEX(ObjectSubclass<ButtonInner>)
        @extends relm4::gtk::Widget,
        @implements relm4::gtk::Accessible, relm4::gtk::Buildable, relm4::gtk::ConstraintTarget;
}

impl ButtonEX {
    pub fn cancel_interaction(&self) {
        *self.imp().is_pressed.borrow_mut() = false;
        self.cancel_popup_timer();
        self.cleanup_all_popovers();
    }

    pub fn cancel_popup_timer(&self) {
        if let Ok(mut popup_timer) = self.imp().popup_timer.try_borrow_mut()
            && let Some(timer) = popup_timer.take()
        {
            timer.remove();
        }
    }

    pub fn cleanup_all_popovers(&self) {
        let mut child = self.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(popover) = widget.downcast::<relm4::gtk::Popover>() {
                super::clear_popover_focus(&popover);
                popover.popdown();
                if popover.parent().is_some() {
                    popover.unparent();
                }
            }
        }
    }

    fn start_popup_timer(&self) {
        let obj_weak = self.downgrade();
        let timer_id =
            timeout_add_local(Duration::from_millis(timeouts::POPUP_TIMER_MS), move || {
                if let Some(obj) = obj_weak.upgrade() {
                    *obj.imp().popup_timer.borrow_mut() = None;
                    if *obj.imp().is_pressed.borrow() && obj.is_realized() && obj.root().is_some() {
                        obj.show_alternatives();
                    }
                }
                glib::ControlFlow::Break
            });
        *self.imp().popup_timer.borrow_mut() = Some(timer_id);
    }

    fn show_alternatives(&self) {
        let alternatives = self.alternatives();
        if alternatives.is_empty() {
            return;
        }

        if !self.is_realized() || self.root().is_none() {
            tracing::warn!("Widget not ready for popup - skipping alternatives");
            return;
        }

        *self.imp().popup_shown.borrow_mut() = true;

        self.cleanup_all_popovers();

        let popover = relm4::gtk::Popover::new();
        popover.set_parent(self);
        popover.set_has_arrow(true);
        popover.add_css_class("alternative-chars");
        // Inherit dark theme if the application window is dark
        if let Some(root) = self.root()
            && let Ok(window) = root.downcast::<relm4::gtk::Window>()
            && window.has_css_class("novakeys-dark")
        {
            popover.add_css_class("novakeys-dark");
        }

        let button_weak = self.downgrade();
        popover.connect_closed(move |popover| {
            super::clear_popover_focus(popover);
            if let Some(button_ref) = button_weak.upgrade() {
                *button_ref.imp().is_pressed.borrow_mut() = false;
                button_ref.cancel_popup_timer();
            }
            if popover.parent().is_some() {
                popover.unparent();
            }
        });

        let container = relm4::gtk::Box::builder()
            .orientation(relm4::gtk::Orientation::Horizontal)
            .spacing(5)
            .margin_top(5)
            .margin_bottom(5)
            .margin_start(5)
            .margin_end(5)
            .build();

        for alternative in alternatives {
            let button = relm4::gtk::Button::builder()
                .label(&alternative)
                .width_request(40)
                .height_request(40)
                .build();

            let alt_clone = alternative.clone();
            let button_weak_alt = self.downgrade();
            let popover_weak = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(button_ref) = button_weak_alt.upgrade() {
                    *button_ref.imp().is_pressed.borrow_mut() = false;
                    button_ref.cancel_popup_timer();

                    if let Some(popover_ref) = popover_weak.upgrade() {
                        super::clear_popover_focus(&popover_ref);
                        popover_ref.popdown();
                        if popover_ref.parent().is_some() {
                            popover_ref.unparent();
                        }
                    }

                    button_ref.emit_by_name::<()>("alternative-selected", &[&alt_clone]);
                }
            });

            container.append(&button);
        }

        popover.set_child(Some(&container));

        popover.popup();
    }

    pub fn update_view(&self) {
        if let Some(label) = self.imp().primary_label.borrow().as_ref() {
            let content = self.primary_content();
            let text = content.as_deref().unwrap_or_default();
            if label.text() != text {
                label.set_text(text);
            }
            label.set_visible(!text.is_empty());
        }
    }
}

impl Default for ButtonEX {
    fn default() -> Self {
        glib::Object::new()
    }
}
