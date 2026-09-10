mod action_button;
mod button_ex;

pub use action_button::ActionButton;
pub use button_ex::ButtonEX;

// GTK can retain a focused popup descendant after its popup is unparented.
pub(crate) fn clear_popover_focus(popover: &relm4::gtk::Popover) {
    use relm4::gtk::prelude::*;
    if let Some(root) = popover.root()
        && root.focus().is_some_and(|focus| focus.is_ancestor(popover))
    {
        root.set_focus(None::<&relm4::gtk::Widget>);
    }
}
