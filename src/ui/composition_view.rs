//! One reusable bounded composition view per active layout.
use super::{UIInput, main_view::InputSender};
use crate::ime::{Command, Mode};
use relm4::gtk::{self, prelude::*};
use std::{cell::Cell, rc::Rc};
pub(super) struct View {
    pub root: gtk::Box,
    modes: Vec<(Mode, gtk::Button)>,
    reading: gtk::Label,
    error: gtk::Label,
    candidates: Vec<gtk::Button>,
    controls: Vec<(Command, gtk::Button)>,
    page: gtk::Label,
    retry: gtk::Button,
    discard: gtk::Button,
    revision: Rc<Cell<u64>>,
    body: gtk::Box,
    footer_content: gtk::Stack,
    status: gtk::Stack,
    toolbar: gtk::Stack,
    direct: gtk::Label,
    language: String,
    collapsed: Cell<bool>,
}
impl View {
    pub fn new(language: &str, sender: &InputSender) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 3);
        root.add_css_class("composition");
        let mode_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let modes = match language {
            "zh" => vec![
                ("简", Mode::Native),
                ("繁", Mode::Traditional),
                ("ABC", Mode::Latin),
            ],
            "ja" => vec![
                ("ひらがな", Mode::Native),
                ("カタカナ", Mode::Katakana),
                ("ABC", Mode::Latin),
            ],
            "ko" => vec![("한글", Mode::Native), ("ABC", Mode::Latin)],
            _ => vec![],
        }
        .into_iter()
        .map(|(label, mode)| {
            let button = button(label, sender, UIInput::CompositionMode(mode));
            mode_row.append(&button);
            (mode, button)
        })
        .collect();
        mode_row.set_halign(gtk::Align::End);
        mode_row.set_valign(gtk::Align::End);
        let reading = gtk::Label::new(None);
        reading.set_xalign(0.0);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(44)
            .max_content_height(44)
            .propagate_natural_width(false)
            .hexpand(true)
            .child(&reading)
            .build();
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.set_lines(2);
        error.set_width_chars(1);
        error.set_ellipsize(gtk::pango::EllipsizeMode::End);
        error.set_xalign(0.0);
        let direct = gtk::Label::new(Some("ABC"));
        direct.set_xalign(0.0);
        let status = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .hexpand(true)
            .build();
        status.set_height_request(44);
        status.add_named(&scroll, Some("reading"));
        status.add_named(&error, Some("error"));

        let retry = button("Retry delivery", sender, UIInput::RetryComposition);
        let discard = button("Discard / reset", sender, UIInput::DiscardComposition);
        let recovery = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        recovery.append(&retry);
        recovery.append(&discard);
        let native = gtk::Box::new(gtk::Orientation::Vertical, 3);
        let body = native.clone();
        let reading_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        reading_row.append(&status);
        native.append(&reading_row);
        root.append(&body);
        let revision = Rc::new(Cell::new(0));
        let candidate_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        candidate_row.set_homogeneous(true);
        let mut candidates = Vec::new();
        for index in 0..crate::ime::PAGE_SIZE {
            let candidate = gtk::Button::new();
            candidate.set_focusable(false);
            candidate.set_height_request(44);
            candidate.set_width_request(44);
            candidate.set_hexpand(true);
            let label = gtk::Label::new(None);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_width_chars(1);
            label.set_max_width_chars(8);
            candidate.set_child(Some(&label));
            let pressed = Rc::new(Cell::new(None));
            let gesture = gtk::GestureClick::new();
            gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
            let current = revision.clone();
            let epoch = sender.epoch.clone();
            let start = pressed.clone();
            gesture
                .connect_pressed(move |_, _, _, _| start.set(Some((current.get(), epoch.get()))));
            candidate.add_controller(gesture);
            let current = revision.clone();
            let target = sender.clone();
            candidate.connect_clicked(move |_| {
                let revision = current.get();
                if pressed
                    .take()
                    .is_none_or(|old| old == (revision, target.epoch.get()))
                {
                    target.emit(UIInput::CompositionCandidate { revision, index });
                }
            });
            candidate_row.append(&candidate);
            candidates.push(candidate);
        }
        let paging_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let previous = button("‹", sender, UIInput::CompositionCommand(Command::Page(-1)));
        let next = button("›", sender, UIInput::CompositionCommand(Command::Page(1)));
        paging_row.append(&previous);
        candidate_row.set_hexpand(true);
        paging_row.append(&candidate_row);
        paging_row.append(&next);
        native.append(&paging_row);
        let controls_row = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(1)
            .max_children_per_line(10)
            .build();
        let mut commands = Vec::new();
        if language != "ko" {
            commands.extend([("←", Command::Left), ("→", Command::Right)]);
        }
        commands.push(("Esc", Command::Escape));
        if language == "ja" {
            commands.extend([
                ("変換", Command::Space),
                ("前文節", Command::Segment(-1)),
                ("次文節", Command::Segment(1)),
                ("縮める", Command::Resize(-1)),
                ("伸ばす", Command::Resize(1)),
            ]);
        }
        if language == "ko" {
            commands.push(("漢字", Command::Hanja));
        }
        let mut controls: Vec<_> = commands
            .into_iter()
            .map(|(label, command)| {
                let button = button(label, sender, UIInput::CompositionCommand(command.clone()));
                controls_row.insert(&button, -1);
                (command, button)
            })
            .collect();
        controls.extend([(Command::Page(-1), previous), (Command::Page(1), next)]);
        let page = gtk::Label::new(None);
        page.set_width_chars(8);
        page.set_max_width_chars(14);
        page.set_ellipsize(gtk::pango::EllipsizeMode::End);
        reading_row.append(&page);
        let toolbar = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .build();
        toolbar.add_named(&controls_row, Some("editing"));
        toolbar.add_named(&recovery, Some("recovery"));
        let footer_content = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .hexpand(true)
            .build();
        direct.set_height_request(44);
        direct.set_valign(gtk::Align::End);
        footer_content.add_named(&toolbar, Some("native"));
        footer_content.add_named(&direct, Some("direct"));
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        footer.append(&footer_content);
        footer.append(&mode_row);
        root.append(&footer);
        Self {
            root,
            modes,
            reading,
            error,
            candidates,
            controls,
            page,
            retry,
            discard,
            revision,
            body,
            footer_content,
            status,
            toolbar,
            direct,
            language: language.into(),
            collapsed: Cell::new(false),
        }
    }
    pub fn visible_top(&self) -> Option<(gtk::Widget, gtk::Widget)> {
        (self.collapsed.get() && self.root.is_visible()).then(|| {
            (
                self.root.last_child().unwrap().last_child().unwrap(),
                self.root.clone().upcast(),
            )
        })
    }
    pub fn cancel_interaction(&self) {
        for button in self
            .modes
            .iter()
            .map(|(_, b)| b)
            .chain(&self.candidates)
            .chain(self.controls.iter().map(|(_, b)| b))
            .chain([&self.retry, &self.discard])
        {
            let controllers = button.observe_controllers();
            for index in 0..controllers.n_items() {
                if let Some(controller) = controllers
                    .item(index)
                    .and_downcast::<gtk::EventController>()
                {
                    controller.reset();
                }
            }
        }
    }
    pub fn update(&self, state: &super::composition::Composition, sensitive: bool) {
        let snapshot = &state.snapshot;
        let mode = state.mode;
        let revision = state.applied;
        let pending = state.pending;
        let error = state.error.as_deref();
        let failed = state.failed_output.is_some();
        let transition = state.transition.is_some();
        let editable = error.is_none() || (state.recoverable_error && pending == 0 && !failed);
        self.revision.set(revision);
        for (value, button) in &self.modes {
            if *value == mode {
                button.add_css_class("suggested-action");
            } else {
                button.remove_css_class("suggested-action");
            }
            button.set_sensitive(!sensitive && !transition && !failed);
        }
        self.error.set_text(error.unwrap_or_default());
        self.error.set_tooltip_text(error);
        reserve(&self.retry, failed && !state.failed_unknown);
        reserve(&self.discard, error.is_some());
        self.direct.set_text(if sensitive {
            "ABC · private field"
        } else {
            "ABC"
        });
        self.status
            .set_visible_child_name(if error.is_some() { "error" } else { "reading" });
        self.toolbar.set_visible_child_name(if error.is_some() {
            "recovery"
        } else {
            "editing"
        });
        let native =
            error.is_some() || (!sensitive && mode != Mode::Latin && !self.modes.is_empty());
        self.collapsed.set(!native);
        reserve(&self.body, native);
        self.footer_content
            .set_visible_child_name(if native { "native" } else { "direct" });
        let text = &snapshot.preedit;
        if text.is_empty() {
            self.reading.set_text(if pending > 0 {
                "Preparing input…"
            } else {
                match self.language.as_str() {
                    "zh" => "输入拼音，选择候选词",
                    "ja" => "ローマ字で入力 · 変換して確定",
                    "ko" => "한글 입력 · 한자 변환",
                    _ => "",
                }
            });
        } else {
            let markup = if let Some((start, end)) = snapshot.selection.filter(|(a, b)| {
                a <= b && *b <= text.len() && text.is_char_boundary(*a) && text.is_char_boundary(*b)
            }) {
                format!(
                    "{}<b><u>{}</u></b>{}",
                    gtk::glib::markup_escape_text(&text[..start]),
                    gtk::glib::markup_escape_text(&text[start..end]),
                    gtk::glib::markup_escape_text(&text[end..])
                )
            } else {
                let mut cursor = snapshot.cursor.min(text.len());
                while !text.is_char_boundary(cursor) {
                    cursor = cursor.saturating_sub(1);
                }
                format!(
                    "{}│{}",
                    gtk::glib::markup_escape_text(&text[..cursor]),
                    gtk::glib::markup_escape_text(&text[cursor..])
                )
            };
            self.reading.set_markup(&markup);
        }
        for (index, button) in self.candidates.iter().enumerate() {
            let text = snapshot.candidates.get(index);
            reserve(button, text.is_some());
            button.set_sensitive(pending == 0 && editable && !transition);
            if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                label.set_text(text.map_or("", String::as_str));
            }
            button.set_tooltip_text(text.map(String::as_str));
            if snapshot.selected == Some(index) {
                button.add_css_class("suggested-action");
            } else {
                button.remove_css_class("suggested-action");
            }
        }
        for (command, button) in &self.controls {
            button.set_sensitive(
                editable
                    && !transition
                    && !(snapshot.pending_romaji
                        && matches!(command, Command::Left | Command::Right)),
            );
            button.set_tooltip_text(
                if snapshot.pending_romaji && matches!(command, Command::Left | Command::Right) {
                    Some("音節を完成するか、Backspaceで戻してください")
                } else {
                    None
                },
            );
            reserve(
                button,
                match command {
                    Command::Page(delta) => {
                        if *delta < 0 {
                            snapshot.has_previous
                        } else {
                            snapshot.has_next
                        }
                    }
                    Command::Segment(_) | Command::Resize(_) => snapshot.segments > 0,
                    Command::Left | Command::Right => self.language != "ko",
                    _ => true,
                },
            );
        }
        reserve(
            &self.page,
            snapshot.has_previous || snapshot.has_next || snapshot.segments > 0,
        );
        let page_status = match self.language.as_str() {
            "zh" => format!("第 {} 页", snapshot.page + 1),
            "ja" => format!(
                "{} ページ · 文節 {}/{}",
                snapshot.page + 1,
                snapshot.active_segment + 1,
                snapshot.segments.max(1)
            ),
            _ => format!("{} 페이지", snapshot.page + 1),
        };
        self.page.set_tooltip_text(Some(&page_status));
        self.page
            .update_property(&[gtk::accessible::Property::Label(&page_status)]);
        self.page.set_text(&match self.language.as_str() {
            "ja" => format!(
                "{}頁 · {}/{}",
                snapshot.page + 1,
                snapshot.active_segment + 1,
                snapshot.segments.max(1)
            ),
            _ => page_status,
        });
        self.root
            .set_visible(!self.modes.is_empty() || error.is_some());
    }
}
fn button(label: &str, sender: &InputSender, event: UIInput) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_focusable(false);
    button.set_height_request(44);
    button.set_width_request(44);
    let pressed = Rc::new(Cell::new(None));
    let gesture = gtk::GestureClick::new();
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let start = pressed.clone();
    let epoch = sender.epoch.clone();
    gesture.connect_pressed(move |_, _, _, _| start.set(Some(epoch.get())));
    button.add_controller(gesture);
    let sender = sender.clone();
    button.connect_clicked(move |_| {
        if pressed
            .take()
            .is_none_or(|epoch| epoch == sender.epoch.get())
        {
            sender.emit(event.clone());
        }
    });
    button
}

/// Reserve layout space while hiding inactive controls from input and accessibility.
fn reserve(widget: &(impl IsA<gtk::Widget> + IsA<gtk::Accessible>), shown: bool) {
    widget.set_opacity(if shown { 1.0 } else { 0.0 });
    widget.set_can_target(shown);
    widget.update_state(&[gtk::accessible::State::Hidden(!shown)]);
}
