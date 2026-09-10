use std::{cell::RefCell, sync::OnceLock, time::Duration};

use relm4::gtk::{
    glib::{self, Properties, timeout_add_local},
    prelude::*,
    subclass::prelude::*,
};

#[derive(Debug, Default, Properties)]
#[properties(wrapper_type = ActionButton)]
pub struct ActionButtonInner {
    #[property(get, set)]
    action: RefCell<Option<String>>,
    #[property(get, set)]
    label: RefCell<Option<String>>,
    repeat_timer: RefCell<Option<glib::SourceId>>,
    is_pressed: RefCell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ActionButtonInner {
    const NAME: &'static str = "ActionButton";
    type Type = ActionButton;
    type ParentType = relm4::gtk::Widget;

    fn class_init(class: &mut Self::Class) {
        class.set_layout_manager_type::<relm4::gtk::BinLayout>();
        // Use a custom CSS name so outer widget doesn't get styled as a regular GTK button
        class.set_css_name("action-button");
        class.set_accessible_role(relm4::gtk::AccessibleRole::Button);
    }
}

#[glib::derived_properties]
impl ObjectImpl for ActionButtonInner {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        let button = relm4::gtk::Button::new();
        button.set_parent(&*obj);

        let button_clone = button.clone();
        obj.connect_label_notify(move |obj| {
            if let Some(label) = obj.label() {
                button_clone.set_label(&label);
            }
        });

        let gesture = relm4::gtk::GestureClick::new();

        let obj_press_cb = obj.downgrade();
        gesture.connect_pressed(move |gesture, _, _, _| {
            let Some(obj_press_cb) = obj_press_cb.upgrade() else {
                return;
            };
            gesture.set_state(relm4::gtk::EventSequenceState::Claimed);

            *obj_press_cb.imp().is_pressed.borrow_mut() = true;
            obj_press_cb.cancel_repeat_timer();

            obj_press_cb.emit_by_name::<()>("activated", &[]);

            let action = obj_press_cb.action().unwrap_or_default();
            let delay_ms = match action.as_str() {
                "backspace" => 200,
                _ => 500,
            };

            if action == "backspace" {
                obj_press_cb.start_repeat_timer(delay_ms);
            }
        });

        let obj_release_cb = obj.downgrade();
        gesture.connect_released(move |gesture, _, _, _| {
            let Some(obj_release_cb) = obj_release_cb.upgrade() else {
                return;
            };
            gesture.set_state(relm4::gtk::EventSequenceState::Claimed);

            *obj_release_cb.imp().is_pressed.borrow_mut() = false;
            obj_release_cb.cancel_repeat_timer();
        });

        let weak = obj.downgrade();
        gesture.connect_cancel(move |_, _| {
            if let Some(obj) = weak.upgrade() {
                *obj.imp().is_pressed.borrow_mut() = false;
                obj.cancel_repeat_timer();
            }
        });
        button.add_controller(gesture);
    }

    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("activated").build()])
    }

    fn dispose(&self) {
        if let Some(timer) = self.repeat_timer.borrow_mut().take() {
            timer.remove();
        }

        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ActionButtonInner {
    fn unmap(&self) {
        *self.is_pressed.borrow_mut() = false;
        self.obj().cancel_repeat_timer();
        self.parent_unmap();
    }
}

glib::wrapper! {
    pub struct ActionButton(ObjectSubclass<ActionButtonInner>)
        @extends relm4::gtk::Widget,
        @implements relm4::gtk::Accessible, relm4::gtk::Buildable, relm4::gtk::ConstraintTarget;
}

impl ActionButton {
    pub fn cancel_interaction(&self) {
        *self.imp().is_pressed.borrow_mut() = false;
        self.cancel_repeat_timer();
    }

    pub fn new(action: &str, label: &str) -> Self {
        let button: Self = glib::Object::new();
        button.set_action(action.to_string());
        button.set_label(label.to_string());
        button
    }

    fn cancel_repeat_timer(&self) {
        if let Some(timer) = self.imp().repeat_timer.borrow_mut().take() {
            timer.remove();
        }
    }

    fn start_repeat_timer(&self, delay_ms: u32) {
        let obj_weak = self.downgrade();
        let timer_id = timeout_add_local(Duration::from_millis(delay_ms as u64), move || {
            if let Some(obj) = obj_weak.upgrade() {
                *obj.imp().repeat_timer.borrow_mut() = None;
                if *obj.imp().is_pressed.borrow() {
                    obj.start_repetition();
                }
            }
            glib::ControlFlow::Break
        });
        *self.imp().repeat_timer.borrow_mut() = Some(timer_id);
    }

    fn start_repetition(&self) {
        let obj_weak = self.downgrade();
        let timer_id = timeout_add_local(Duration::from_millis(100), move || {
            if let Some(obj) = obj_weak.upgrade() {
                if *obj.imp().is_pressed.borrow() {
                    obj.emit_by_name::<()>("activated", &[]);
                    glib::ControlFlow::Continue
                } else {
                    *obj.imp().repeat_timer.borrow_mut() = None;
                    glib::ControlFlow::Break
                }
            } else {
                glib::ControlFlow::Break
            }
        });
        *self.imp().repeat_timer.borrow_mut() = Some(timer_id);
    }
}

impl Default for ActionButton {
    fn default() -> Self {
        glib::Object::new()
    }
}
