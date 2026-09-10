//! Local composition and delivery of completed, owned engine output.
use super::{UIModel, main_view::InputSender};
use crate::ime::{self, Command, Mode, Snapshot, Token};
use relm4::gtk::prelude::*;
use std::sync::mpsc::SyncSender;
#[derive(Default)]
pub(super) struct Composition {
    pub worker: Option<ime::Worker>,
    pub mode: Mode,
    pub snapshot: Snapshot,
    pub requested: u64,
    pub applied: u64,
    pub pending: usize,
    pub error: Option<String>,
    pub recoverable_error: bool,
    pub transition_input: std::collections::VecDeque<Command>,
    pub failed_unknown: bool,
    pub failed_output: Option<Snapshot>,
    pub deferred: std::collections::VecDeque<Snapshot>,
    pub transition: Option<Transition>,
    pub bar: Option<super::composition_view::View>,
}
pub(super) enum Transition {
    Language(String, Option<SyncSender<crate::service::IPCResponse>>),
    Mode(Mode),
}
impl UIModel {
    fn token(&self) -> Token {
        Token {
            generation: self.input_context.generation,
            epoch: self.input_epoch.get(),
        }
    }
    pub(super) fn composition_enabled(&self) -> bool {
        matches!(self.current_language_code(), "zh" | "ja" | "ko")
            && self.composition.mode != Mode::Latin
            && !self.input_context.sensitive()
    }
    pub(super) fn reset_composition(&mut self) {
        if let Some(Transition::Language(_, Some(reply))) = self.composition.transition.take() {
            let _ = reply.send(crate::service::IPCResponse::error(
                "Input context changed before language switch".into(),
            ));
        }
        self.composition.snapshot = Snapshot::default();
        self.composition.pending = 0;
        self.composition.failed_output = None;
        self.composition.deferred.clear();
        self.composition.error = None;
        self.composition.recoverable_error = false;
        self.composition.transition_input.clear();
        self.composition.failed_unknown = false;
        self.composition.applied = self.composition.requested;
        if let Some(worker) = &self.composition.worker {
            worker.reset_context(self.token());
            while worker.receive().is_some() {}
        }
    }
    pub(super) fn start_composition(&mut self, sender: &InputSender) {
        if let Some(worker) = &self.composition.worker {
            worker.reset_context(self.token());
        }
        if self.composition_enabled() && self.input_context.active {
            self.send_engine(Command::Reset, sender);
        }
        self.render_composition(sender);
    }
    pub(super) fn input_ready(&mut self) -> bool {
        if self.composition.error.is_some()
            && self.composition.recoverable_error
            && self.composition.pending == 0
            && self.composition.failed_output.is_none()
        {
            self.composition.error = None;
            self.composition.recoverable_error = false;
        }
        self.composition.error.is_none()
    }
    pub(super) fn send_engine(&mut self, command: Command, sender: &InputSender) -> bool {
        if !self.input_ready() || !self.composition_enabled() {
            return false;
        }
        let revision = self.composition.requested.wrapping_add(1);
        let job = ime::Job {
            token: self.token(),
            revision,
            language: self.current_language_code().to_owned(),
            mode: self.composition.mode,
            command,
        };
        let mut accepted = false;
        if let Some(worker) = &self.composition.worker {
            match worker.send(job) {
                Ok(()) => {
                    self.composition.requested = revision;
                    self.composition.pending += 1;
                    accepted = true;
                }
                Err(error) => {
                    self.composition.recoverable_error = error.recoverable;
                    self.composition.error = Some(error.message);
                }
            }
        }
        self.render_composition(sender);
        accepted
    }
    pub(super) fn receive_engine(&mut self, sender: &InputSender) {
        if self.composition.failed_output.is_some() {
            self.render_composition(sender);
            return;
        }
        let mut combined: Option<Snapshot> = None;
        while let Some(response) = self
            .composition
            .worker
            .as_ref()
            .and_then(ime::Worker::receive)
        {
            if response.token != self.token() {
                continue;
            }
            self.composition.pending = self.composition.pending.saturating_sub(1);
            self.composition.applied = response.revision;
            match response.result {
                Err(error) => {
                    if let Some(output) = combined.take() {
                        self.composition.deferred.push_back(output);
                    }
                    self.composition.recoverable_error = error.recoverable;
                    self.composition.error = Some(error.message);
                    // Accepted later jobs must still drain after a nonmutating rejection.
                    if !self.composition.recoverable_error {
                        break;
                    }
                }
                Ok(mut snapshot) => {
                    if let Some(previous) = combined.take() {
                        if previous.control.is_some()
                            || previous.committed.len() + snapshot.committed.len() > 4096
                        {
                            self.composition.deferred.push_back(previous);
                        } else {
                            snapshot.committed.insert_str(0, &previous.committed);
                        }
                    }
                    let control = snapshot.control.is_some();
                    combined = Some(snapshot);
                    if control {
                        break;
                    }
                }
            }
        }
        if let Some(output) = combined {
            self.composition.deferred.push_back(output);
        }
        if self.composition.pending == 0
            && (self.composition.error.is_none() || self.composition.recoverable_error)
            && matches!(
                self.composition.transition,
                Some(Transition::Mode(Mode::Latin))
            )
            && let Some(output) = self.composition.deferred.back_mut()
            && output.control.is_none()
        {
            // Coalesce queued ABC text into Finish up to the next control or output limit.
            while let Some(command) = self.composition.transition_input.front() {
                let text = match command {
                    Command::Text(text) => text.as_str(),
                    Command::Space => " ",
                    _ => break,
                };
                if output.committed.len() + text.len() > 4096 {
                    break;
                }
                output.committed.push_str(text);
                self.composition.transition_input.pop_front();
            }
        }
        while let Some(output) = self.composition.deferred.pop_front() {
            if !self.deliver_engine(output) {
                break;
            }
        }
        if self.composition.pending == 0
            && (self.composition.error.is_none() || self.composition.recoverable_error)
            && self.composition.failed_output.is_none()
            && let Some(transition) = self.composition.transition.take()
        {
            self.apply_transition(transition, sender);
        }
        self.drain_transition_input(sender);
        self.render_composition(sender);
    }
    pub(super) fn deliver_engine(&mut self, mut output: Snapshot) -> bool {
        if !output.committed.is_empty() {
            if let Err(error) = self.keyboard_handle.insert_text(&output.committed) {
                self.composition.recoverable_error = false;
                self.composition.failed_unknown = error.outcome_unknown();
                self.composition.error = Some(if error.outcome_unknown() {
                    format!("{error}. Restart the keyboard before continuing.")
                } else {
                    error.to_string()
                });
                self.composition.failed_output = Some(output);
                return false;
            }
            output.committed.clear();
        }
        if let Some(control) = output.control {
            if let Err(error) = if control == '\u{0008}' {
                self.keyboard_handle.delete_text(1, 0)
            } else {
                self.keyboard_handle.control(control)
            } {
                self.composition.recoverable_error = false;
                self.composition.failed_unknown = error.outcome_unknown();
                self.composition.error = Some(if error.outcome_unknown() {
                    format!("{error}. Restart the keyboard before continuing.")
                } else {
                    error.to_string()
                });
                self.composition.failed_output = Some(output);
                return false;
            }
            output.control = None;
        }
        self.composition.snapshot = output;
        true
    }
    pub(super) fn retry_engine(&mut self, sender: &InputSender) {
        if self.composition.failed_unknown {
            return;
        }
        self.composition.error = None;
        if let Some(output) = self.composition.failed_output.take() {
            self.deliver_engine(output);
        }
        self.receive_engine(sender);
    }
    pub(super) fn request_transition(&mut self, transition: Transition, sender: &InputSender) {
        if let Transition::Language(code, reply) = &transition {
            let known = self
                .app_config
                .get_language_manager()
                .layout(code)
                .is_some();
            if !known || code == self.current_language_code() {
                if let Some(reply) = reply {
                    let _ = reply.send(if known {
                        crate::service::IPCResponse {
                            status: None,
                            error: None,
                        }
                    } else {
                        crate::service::IPCResponse::error("Unknown language".into())
                    });
                }
                return;
            }
        }
        if matches!(&transition,Transition::Mode(mode) if *mode==self.composition.mode) {
            return;
        }

        if self.composition.transition.is_some()
            || self.composition.failed_output.is_some()
            || !self.composition.transition_input.is_empty()
        {
            if let Transition::Language(_, Some(reply)) = transition {
                let _ = reply.send(crate::service::IPCResponse::error(
                    "Finish or reset current input first".into(),
                ));
            }
            return;
        }
        self.input_ready();
        if self.composition.error.is_some() {
            if !self.composition.snapshot.preedit.is_empty()
                || self.composition.pending > 0
                || !self.composition.deferred.is_empty()
            {
                if let Transition::Language(_, Some(reply)) = transition {
                    let _ = reply.send(crate::service::IPCResponse::error(
                        "Finish or explicitly discard current input first".into(),
                    ));
                }
                return;
            }
            self.reset_input();
            self.apply_transition(transition, sender);
            return;
        }
        if self.composition_enabled()
            && (!self.composition.snapshot.preedit.is_empty() || self.composition.pending > 0)
        {
            if self.send_engine(Command::Finish, sender) {
                self.composition.transition = Some(transition);
                self.refresh_keys();
            } else if let Transition::Language(_, Some(reply)) = transition {
                let _ = reply.send(crate::service::IPCResponse::error(
                    "Input is busy; language was not changed".into(),
                ));
            }
            self.render_composition(sender);
        } else {
            self.apply_transition(transition, sender);
        }
    }
    fn apply_transition(&mut self, transition: Transition, sender: &InputSender) {
        // Preserve queued taps across the transition's reset; a focus reset discards them.
        let queued = std::mem::take(&mut self.composition.transition_input);
        match transition {
            Transition::Language(code, reply) => {
                let result = self.change_language(&code, sender);
                if let Some(reply) = reply {
                    let _ = reply.send(match result {
                        Ok(()) => crate::service::IPCResponse {
                            status: None,
                            error: None,
                        },
                        Err(error) => crate::service::IPCResponse::error(error),
                    });
                } else if let Err(error) = result {
                    self.composition.error = Some(error);
                }
            }
            Transition::Mode(mode) => {
                // Same field/layout: preserve ordinary held keys across completion.
                // Toolbar/candidate presses still belong to the previous mode.
                if let Some(view) = &self.composition.bar {
                    view.cancel_interaction();
                }
                self.reset_composition();
                self.composition.mode = mode;
                self.refresh_keys();
                self.start_composition(sender);
            }
        }
        self.composition.transition_input = queued;
        self.drain_transition_input(sender);
    }
    fn drain_transition_input(&mut self, sender: &InputSender) {
        if self.composition.transition.is_some() {
            return;
        }
        while !self.composition.transition_input.is_empty() {
            if !self.input_ready() {
                return;
            }
            let mut command = self.composition.transition_input.pop_front().unwrap();
            if let Command::Text(text) = &mut command {
                while let Some(Command::Text(next)) = self.composition.transition_input.front() {
                    if text.len() + next.len() > 4096 {
                        break;
                    }
                    text.push_str(next);
                    self.composition.transition_input.pop_front();
                }
            }
            if !self.dispatch_key_command(command.clone(), sender) {
                self.composition.transition_input.push_front(command);
                return;
            }
        }
    }
    pub(super) fn discard_composition(&mut self, sender: &InputSender) {
        self.reset_input();
        self.start_composition(sender);
    }
    pub(super) fn render_composition(&mut self, sender: &InputSender) {
        if self.composition.bar.is_none()
            && (matches!(self.current_language_code(), "zh" | "ja" | "ko")
                || self.composition.error.is_some())
            && let Some(widgets) = &self.layout_state.widgets
        {
            let view = super::composition_view::View::new(self.current_language_code(), sender);
            widgets.container.prepend(&view.root);
            self.composition.bar = Some(view);
        }
        if let Some(view) = &self.composition.bar {
            view.update(&self.composition, self.input_context.sensitive());
            super::overlay_window::set_visible_top(&self.window, view.visible_top().as_ref());
        }
    }
}
