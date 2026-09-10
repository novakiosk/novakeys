//! Key input, native composition, and one-shot Shift transitions.
use super::{UIInput, UIModel, composition::Transition, main_view::InputSender};
use crate::{constants::keyboard_actions, ime::Command};
// Direct Korean keys use Latin Shift after mapping a jamo to its physical key(s).
// Keep compound alternatives literal: ordinary Shift changes only one character.
pub(super) fn direct_korean_text(text: &str, shift: bool) -> String {
    crate::layout::parse::shifted_text(&crate::ime::korean_keys(text, shift), shift, None)
}

impl UIModel {
    pub(super) fn native_key_input(&self) -> bool {
        self.composition_enabled()
            && !matches!(
                self.composition.transition,
                Some(Transition::Mode(crate::ime::Mode::Latin))
            )
    }

    pub(super) fn handle_text_key(&mut self, row: usize, key: usize, sender: &InputSender) {
        let layout = self.app_config.get_language_manager().current_layout();
        let Some(text) = layout
            .layout
            .get(row)
            .and_then(|row| row.get(key))
            .and_then(|key| key.get_insert_text())
        else {
            return;
        };
        let text = crate::layout::parse::shifted_text(
            text,
            self.layout_state.shift_pressed,
            layout.language_code.as_deref(),
        );
        self.insert_literal(&text, sender);
        self.consume_shift();
    }
    fn insert_literal(&mut self, text: &str, sender: &InputSender) {
        let text = if self.current_language_code() == "ko" {
            if self.native_key_input() {
                crate::ime::korean_keys(text, self.layout_state.shift_pressed)
            } else {
                direct_korean_text(text, self.layout_state.shift_pressed)
            }
        } else {
            text.to_owned()
        };
        self.dispatch_key_command(
            if text == " " {
                Command::Space
            } else {
                Command::Text(text)
            },
            sender,
        );
    }
    pub(super) fn dispatch_key_command(&mut self, command: Command, sender: &InputSender) -> bool {
        if self.composition.transition.is_some() {
            let bytes: usize = self
                .composition
                .transition_input
                .iter()
                .map(|command| match command {
                    Command::Text(text) => text.len(),
                    _ => 1,
                })
                .sum();
            let added = match &command {
                Command::Text(text) => text.len(),
                _ => 1,
            };
            if self.composition.transition_input.len() < 24 && bytes + added <= 4096 {
                self.composition.transition_input.push_back(command);
                return true;
            } else {
                self.composition.error =
                    Some("Input is busy; the last key was not accepted".into());
                self.composition.recoverable_error = true;
                self.render_composition(sender);
            }
            return false;
        }
        if !self.input_ready() {
            return false;
        }
        if self.composition_enabled() {
            self.send_engine(command, sender)
        } else {
            let output = match command {
                Command::Text(text) => crate::ime::Snapshot {
                    committed: text,
                    ..Default::default()
                },
                Command::Space => crate::ime::Snapshot {
                    committed: " ".into(),
                    ..Default::default()
                },
                Command::Enter => crate::ime::Snapshot {
                    control: Some('\n'),
                    ..Default::default()
                },
                Command::Backspace => crate::ime::Snapshot {
                    control: Some('\u{0008}'),
                    ..Default::default()
                },
                _ => return false,
            };
            self.deliver_engine(output);
            self.render_composition(sender);
            // A failed delivery owns this action in failed_output; never enqueue it twice.
            true
        }
    }

    fn consume_shift(&mut self) {
        if self.layout_state.shift_pressed {
            self.layout_state.shift_pressed = false;
            self.refresh_keys();
        }
    }
    pub(super) fn handle_action_key(&mut self, action: &str, sender: &InputSender) {
        if action == keyboard_actions::SHIFT {
            self.layout_state.shift_pressed = !self.layout_state.shift_pressed;
            self.refresh_keys();
            return;
        }
        if action == keyboard_actions::LANGUAGE_SELECTOR {
            sender.emit(UIInput::ShowLanguageSelector);
            return;
        }
        let command = match action {
            keyboard_actions::BACKSPACE => Command::Backspace,
            keyboard_actions::ENTER => Command::Enter,
            keyboard_actions::SPACE => Command::Space,
            _ => return,
        };
        self.dispatch_key_command(command, sender);
    }

    pub(super) fn handle_input(&mut self, epoch: u64, event: UIInput, sender: &InputSender) {
        if epoch != self.input_epoch.get() {
            return;
        }
        match event {
            UIInput::TextKeyAtPosition {
                row_index,
                key_index,
            } => self.handle_text_key(row_index.get(), key_index.get(), sender),
            UIInput::ActionKey(action) => self.handle_action_key(&action, sender),
            UIInput::AlternativeSelected(text) => {
                self.insert_literal(&text, sender);
                self.consume_shift();
            }
            UIInput::CompositionMode(mode) => {
                self.request_transition(Transition::Mode(mode), sender)
            }
            UIInput::CompositionCommand(command) => {
                if self.composition.transition.is_none() {
                    self.send_engine(command, sender);
                }
            }
            UIInput::CompositionCandidate { revision, index } => {
                if revision == self.composition.applied
                    && self.composition.pending == 0
                    && self.composition.transition.is_none()
                {
                    self.send_engine(Command::Select(index), sender);
                }
            }
            UIInput::RetryComposition => self.retry_engine(sender),
            UIInput::DiscardComposition => self.discard_composition(sender),
            UIInput::SetLanguage(code) => {
                self.request_transition(Transition::Language(code.to_string(), None), sender)
            }
            UIInput::ShowLanguageSelector => {
                super::language_selector::show_language_selector(self, sender)
            }
        }
    }
}
