//! Real GTK ownership checks; run explicitly under Xvfb or a desktop display.
use super::*;
use crate::service::host::KeyboardHandle;
use relm4::gtk::{self, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

struct Backend {
    inserted: Rc<RefCell<Vec<String>>>,
    fail: Rc<Cell<bool>>,
    unknown: Rc<Cell<bool>>,
}
impl KeyboardHandle for Backend {
    fn insert_text(&mut self, text: &str) -> Result<(), crate::service::host::DeliveryError> {
        if self.unknown.get() {
            return Err(crate::service::host::DeliveryError::OutcomeUnknown(
                "test uncertain delivery".into(),
            ));
        }
        if self.fail.get() {
            Err(crate::service::host::DeliveryError::Rejected(
                "test injection failure".into(),
            ))
        } else {
            self.inserted.borrow_mut().push(text.into());
            Ok(())
        }
    }
    fn delete_text(&mut self, _: u32, _: u32) -> Result<(), crate::service::host::DeliveryError> {
        Ok(())
    }
    fn destroy(&mut self) {}
}
fn drain() {
    let ctx = glib::MainContext::default();
    while ctx.pending() {
        ctx.iteration(false);
    }
}
fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    while Instant::now() < end {
        drain();
        std::thread::sleep(Duration::from_millis(2));
    }
    drain();
}
fn press(widget: &impl IsA<gtk::Widget>) {
    let controllers = widget.observe_controllers();
    let gesture = (0..controllers.n_items())
        .find_map(|i| {
            controllers
                .item(i)
                .and_then(|c| c.downcast::<gtk::GestureClick>().ok())
        })
        .unwrap();
    gesture.emit_by_name::<()>("pressed", &[&1i32, &0f64, &0f64]);
}
fn weak_tree(root: &gtk::Widget, refs: &mut Vec<glib::WeakRef<gtk::Widget>>) {
    refs.push(root.downgrade());
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        weak_tree(&widget, refs);
    }
}

#[test]
#[ignore = "requires a GTK display; run under xvfb-run with --ignored --test-threads=1"]
fn active_tree_releases_widgets_and_cancels_interactions() {
    // Set XDG paths before the child starts; never mutate a running test's environment.
    let Ok(config_root) = std::env::var("NOVAKEYS_LIFECYCLE_CONFIG") else {
        let directory = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::gtk_lifecycle::active_tree_releases_widgets_and_cancels_interactions",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("NOVAKEYS_LIFECYCLE_CONFIG", directory.path())
            .env("XDG_CONFIG_HOME", directory.path())
            .env("XDG_CONFIG_DIRS", directory.path())
            .status()
            .unwrap();
        assert!(status.success(), "isolated GTK lifecycle test failed");
        return;
    };
    let config_directory = std::path::Path::new(&config_root).join("novakeys");
    std::fs::create_dir_all(&config_directory).unwrap();
    let config_path = config_directory.join("config");
    gtk::init().unwrap();
    let window = gtk::Window::builder()
        .default_width(800)
        .default_height(260)
        .build();
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&container));
    let config = crate::config::AppConfig::test_builtins();
    let providers = window_setup::setup_css_providers(&config).unwrap();
    window_setup::install_css_providers(&providers);
    let fail = Rc::new(Cell::new(false));
    let unknown = Rc::new(Cell::new(false));
    let inserted = Rc::new(RefCell::new(Vec::new()));
    let mut model = UIModel {
        input_context: crate::input_detection::InputContext::default(),
        input_epoch: Rc::new(Cell::new(0)),
        keyboard_handle: Box::new(Backend {
            inserted: inserted.clone(),
            fail: fail.clone(),
            unknown: unknown.clone(),
        }),
        window: window.clone(),
        app_config: config,
        container,
        window_height: 260,
        layout_state: state::LayoutState::default(),
        composition: super::composition::Composition::default(),
        css_providers: providers,
        shutdown_sources: Vec::new(),
        language_selector_popover: RefCell::new(None),
    };
    let (tx, _receiver) = relm4::channel();
    let sender = main_view::InputSender {
        sender: tx,
        epoch: model.input_epoch.clone(),
    };
    // Real signals must reach AppQuit through the production GLib sources.
    let (signal_tx, signal_rx) = relm4::channel();
    model.install_shutdown_handlers(&signal_tx);
    let signal_sources: Vec<_> = model
        .shutdown_sources
        .iter()
        .map(|id| {
            gtk::glib::MainContext::default()
                .find_source_by_id(id)
                .unwrap()
        })
        .collect();
    for signal in [
        rustix::process::Signal::INT,
        rustix::process::Signal::TERM,
        rustix::process::Signal::HUP,
    ] {
        rustix::process::kill_process(rustix::process::getpid(), signal).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut receive = std::pin::pin!(signal_rx.recv());
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            drain();
            if let std::task::Poll::Ready(message) =
                std::future::Future::poll(receive.as_mut(), &mut context)
            {
                assert!(matches!(message, Some(main_view::UIMessage::AppQuit)));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "shutdown signal was not dispatched"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    model.rebuild_keyboard(&sender);
    window.present();
    pump(Duration::from_millis(100));

    // Unchanged refreshes and shift changes retain the actual label objects.
    let key = model.layout_state.widgets.as_ref().unwrap().text_keys[0]
        .2
        .clone();
    let label = key.first_child().unwrap();
    for shift in [false, true, false, false] {
        model.layout_state.shift_pressed = shift;
        model.refresh_keys();
        assert_eq!(key.first_child().unwrap(), label);
    }
    drop((key, label));

    // Reset while still mapped must cancel both stages of repetition.
    let repeat = model.layout_state.widgets.as_ref().unwrap().repeat_keys[0].clone();
    let activations = Rc::new(Cell::new(0));
    let count = activations.clone();
    repeat.connect_local("activated", false, move |_| {
        count.set(count.get() + 1);
        None
    });
    press(&repeat.first_child().unwrap());
    model.reset_input();
    pump(Duration::from_millis(350));
    assert_eq!(activations.get(), 1);
    press(&repeat.first_child().unwrap());
    pump(Duration::from_millis(450));
    assert!(activations.get() > 2);
    model.reset_input();
    let stopped = activations.get();
    pump(Duration::from_millis(250));
    assert_eq!(activations.get(), stopped);
    drop(repeat);

    let alternative = model
        .layout_state
        .widgets
        .as_ref()
        .unwrap()
        .text_keys
        .iter()
        .find(|(_, _, key)| !key.alternatives().is_empty())
        .unwrap()
        .2
        .clone();
    press(&alternative);
    model.reset_input();
    pump(Duration::from_millis(250));
    assert!(
        alternative
            .last_child()
            .unwrap()
            .downcast_ref::<gtk::Popover>()
            .is_none()
    );
    press(&alternative);
    pump(Duration::from_millis(250));
    let popover = alternative
        .last_child()
        .unwrap()
        .downcast::<gtk::Popover>()
        .unwrap()
        .downgrade();
    model.reset_input();
    drain();
    assert!(popover.upgrade().is_none());
    for selection in [false, true, false, true] {
        press(&alternative);
        pump(Duration::from_millis(250));
        let popup = alternative
            .last_child()
            .unwrap()
            .downcast::<gtk::Popover>()
            .unwrap();
        let weak = popup.downgrade();
        if selection {
            popup
                .child()
                .unwrap()
                .first_child()
                .unwrap()
                .downcast::<gtk::Button>()
                .unwrap()
                .emit_clicked();
        } else {
            popup.popdown();
        }
        drop(popup);
        pump(Duration::from_millis(50));
        assert!(weak.upgrade().is_none());
    }
    drop(alternative);

    // Production callback reads the epoch when emitting, including after a mapped reset.
    let (tx, rx) = relm4::channel();
    let event_sender = main_view::InputSender {
        sender: tx,
        epoch: model.input_epoch.clone(),
    };
    let definition = model.app_config.get_language_manager().current_layout();
    let button = widget_factory::KeyboardWidgetFactory::create_text_button(
        &definition.layout[1][0],
        40,
        40,
        1,
        0,
        &event_sender,
    );
    button.emit_by_name::<()>("pressed", &[]);
    model.change_language("fr", &sender).unwrap();
    let UIMessage::Input { epoch, event } = rx.recv_sync().unwrap() else {
        panic!("expected input")
    };
    model.handle_input(epoch, event, &sender);
    assert!(inserted.borrow().is_empty());
    model.reset_input();
    button.emit_by_name::<()>("pressed", &[]);
    let UIMessage::Input { epoch, event } = rx.recv_sync().unwrap() else {
        panic!("expected input")
    };
    assert_eq!(epoch, model.input_epoch.get());
    model.handle_input(epoch, event, &sender);
    assert_eq!(inserted.borrow().len(), 1);
    inserted.borrow_mut().clear();
    let old_epoch = model.input_epoch.get();
    model.reset_input();
    model.handle_input(
        old_epoch,
        UIInput::AlternativeSelected("old".into()),
        &sender,
    );
    assert!(inserted.borrow().is_empty());
    drop(button);

    // Select an actual shifted popup button and deliver exactly what it displays.
    model.change_language("fr", &sender).unwrap();
    model.layout_state.shift_pressed = true;
    model.refresh_keys();
    let definition = model.app_config.get_language_manager().current_layout();
    let alternative = model
        .layout_state
        .widgets
        .as_ref()
        .unwrap()
        .text_keys
        .iter()
        .find(|(row, key, _)| definition.layout[*row][*key].text.as_deref() == Some("a"))
        .unwrap()
        .2
        .clone();
    let selected = Rc::new(RefCell::new(None::<String>));
    let captured = selected.clone();
    alternative.connect_local("alternative-selected", false, move |args| {
        *captured.borrow_mut() = Some(args[1].get::<String>().unwrap());
        None
    });
    press(&alternative);
    pump(Duration::from_millis(250));
    let popup = alternative
        .last_child()
        .unwrap()
        .downcast::<gtk::Popover>()
        .unwrap();
    let popup_key = popup
        .child()
        .unwrap()
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    let expected = popup_key.label().unwrap().to_string();
    assert_eq!(expected, "À");
    popup_key.emit_clicked();
    model.handle_input(
        model.input_epoch.get(),
        UIInput::AlternativeSelected(selected.borrow_mut().take().unwrap()),
        &sender,
    );
    assert_eq!(inserted.borrow_mut().pop().unwrap(), expected);
    assert!(!model.layout_state.shift_pressed);
    assert_eq!(alternative.alternatives()[0], "à");
    drop((popup_key, popup, alternative, definition));
    model.change_language("tr", &sender).unwrap();
    for (text, expected) in [("i", "İ"), ("ı", "I")] {
        model.layout_state.shift_pressed = true;
        model.refresh_keys();
        let definition = model.app_config.get_language_manager().current_layout();
        let (row, key, button) = model
            .layout_state
            .widgets
            .as_ref()
            .unwrap()
            .text_keys
            .iter()
            .find(|(row, key, _)| definition.layout[*row][*key].text.as_deref() == Some(text))
            .unwrap();
        assert_eq!(button.primary_content().as_deref(), Some(expected));
        let (row, key) = (*row, *key);
        model.handle_text_key(row, key, &sender);
        assert_eq!(inserted.borrow_mut().pop().unwrap(), expected);
        assert!(!model.layout_state.shift_pressed);
    }

    // Korean direct input uses ordinary one-shot Latin Shift, including private fields.
    model.change_language("ko", &sender).unwrap();
    for (mode, purpose) in [(crate::ime::Mode::Latin, 0), (crate::ime::Mode::Native, 8)] {
        model.composition.mode = mode;
        model.input_context.content_purpose = purpose;
        for (row, key, expected, plain) in [(2, 0, "A", "a"), (1, 5, "Y", "y")] {
            model.handle_action_key(crate::constants::keyboard_actions::SHIFT, &sender);
            let button = model
                .layout_state
                .widgets
                .as_ref()
                .unwrap()
                .text_keys
                .iter()
                .find(|(r, k, _)| (*r, *k) == (row, key))
                .unwrap()
                .2
                .clone();
            assert_eq!(button.primary_content().as_deref(), Some(expected));
            model.handle_text_key(row, key, &sender);
            assert_eq!(inserted.borrow_mut().pop().unwrap(), expected);
            assert!(!model.layout_state.shift_pressed);
            assert_eq!(button.primary_content().as_deref(), Some(plain));
            model.handle_text_key(row, key, &sender);
            assert_eq!(inserted.borrow_mut().pop().unwrap(), plain);
        }
        model.handle_action_key(crate::constants::keyboard_actions::SHIFT, &sender);
        let popup_key = model
            .layout_state
            .widgets
            .as_ref()
            .unwrap()
            .text_keys
            .iter()
            .find(|(r, k, _)| (*r, *k) == (2, 5))
            .unwrap()
            .2
            .clone();
        let alternative = popup_key.alternatives()[0].clone();
        assert_eq!(alternative, "hk");
        model.handle_input(
            model.input_epoch.get(),
            UIInput::AlternativeSelected(alternative.clone()),
            &sender,
        );
        assert_eq!(inserted.borrow_mut().pop().unwrap(), alternative);
        assert!(!model.layout_state.shift_pressed);
        let space = model
            .layout_state
            .widgets
            .as_ref()
            .unwrap()
            .text_keys
            .iter()
            .find(|(r, k, _)| (*r, *k) == (4, 2))
            .unwrap()
            .2
            .clone();
        assert_eq!(space.primary_content().as_deref(), Some("Space"));
    }
    model.input_context.content_purpose = 0;
    model.composition.mode = crate::ime::Mode::Native;
    // Native Shift still sends Dubeolsik keys; pending ABC taps use their destination mode.
    let (worker, responses, jobs) = crate::ime::Worker::fixture();
    model.composition.worker = Some(worker);
    for (row, key, label, physical) in [(1, 0, "ㅃ", "Q"), (1, 5, "ㅛ", "y"), (1, 8, "ㅒ", "O")]
    {
        model.handle_action_key(crate::constants::keyboard_actions::SHIFT, &sender);
        let button = model
            .layout_state
            .widgets
            .as_ref()
            .unwrap()
            .text_keys
            .iter()
            .find(|(r, k, _)| (*r, *k) == (row, key))
            .unwrap()
            .2
            .clone();
        assert_eq!(button.primary_content().as_deref(), Some(label));
        model.handle_text_key(row, key, &sender);
        assert!(!model.layout_state.shift_pressed);
        let job = jobs.try_recv().unwrap();
        assert!(matches!(job.command, crate::ime::Command::Text(ref text) if text == physical));
        responses
            .send(crate::ime::Response {
                token: job.token,
                revision: job.revision,
                result: Ok(crate::ime::Snapshot {
                    preedit: "가".into(),
                    ..Default::default()
                }),
            })
            .unwrap();
        model.receive_engine(&sender);
        assert!(inserted.borrow().is_empty());
    }
    model.handle_action_key(crate::constants::keyboard_actions::SHIFT, &sender);
    model.request_transition(
        super::composition::Transition::Mode(crate::ime::Mode::Latin),
        &sender,
    );
    let finish = jobs.try_recv().unwrap();
    assert!(matches!(finish.command, crate::ime::Command::Finish));
    let button = model
        .layout_state
        .widgets
        .as_ref()
        .unwrap()
        .text_keys
        .iter()
        .find(|(r, k, _)| (*r, *k) == (2, 0))
        .unwrap()
        .2
        .clone();
    assert_eq!(button.primary_content().as_deref(), Some("A"));
    model.handle_text_key(2, 0, &sender);
    assert!(!model.layout_state.shift_pressed);
    assert!(
        matches!(model.composition.transition_input.front(), Some(crate::ime::Command::Text(text)) if text == "A")
    );
    assert!(inserted.borrow().is_empty());
    responses
        .send(crate::ime::Response {
            token: finish.token,
            revision: finish.revision,
            result: Ok(crate::ime::Snapshot {
                committed: "가".into(),
                ..Default::default()
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "가A");
    assert_eq!(model.composition.mode, crate::ime::Mode::Latin);
    assert!(model.composition.transition_input.is_empty());
    drop(button);
    model.composition.worker = None;

    model.change_language("zh", &sender).unwrap();
    model.composition.snapshot.preedit = "nihao".into();
    model.composition.snapshot.cursor = 5;
    fail.set(true);
    assert!(!model.deliver_engine(crate::ime::Snapshot {
        committed: "你好，".into(),
        ..Default::default()
    }));
    assert_eq!(
        model.composition.failed_output.as_ref().unwrap().committed,
        "你好，"
    );
    assert_eq!(model.composition.snapshot.preedit, "nihao");
    fail.set(false);
    model.retry_engine(&sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "你好，");
    assert!(model.composition.failed_output.is_none());
    model.reset_input();
    assert!(model.composition.snapshot.preedit.is_empty());

    // Current-generation engine commits retain FIFO order and coalesce per UI drain.
    let (worker, responses, jobs) = crate::ime::Worker::fixture();
    model.composition.worker = Some(worker);
    model.input_context = crate::input_detection::InputContext {
        generation: 42,
        active: true,
        ..Default::default()
    };
    model.start_composition(&sender);
    let reset = jobs.try_iter().last().unwrap();
    responses
        .send(crate::ime::Response {
            token: reset.token,
            revision: reset.revision,
            result: Ok(Default::default()),
        })
        .unwrap();
    model.receive_engine(&sender);
    for command in [
        crate::ime::Command::Text("nihao".into()),
        crate::ime::Command::Space,
        crate::ime::Command::Text("，".into()),
    ] {
        model.send_engine(command, &sender);
    }
    for (index, job) in jobs.try_iter().enumerate() {
        responses
            .send(crate::ime::Response {
                token: job.token,
                revision: job.revision,
                result: Ok(match index {
                    0 => crate::ime::Snapshot {
                        preedit: "nihao".into(),
                        cursor: 5,
                        ..Default::default()
                    },
                    1 => crate::ime::Snapshot {
                        committed: "你好".into(),
                        ..Default::default()
                    },
                    _ => crate::ime::Snapshot {
                        committed: "，".into(),
                        ..Default::default()
                    },
                }),
            })
            .unwrap();
    }
    model.receive_engine(&sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "你好，");
    let revision = model.composition.applied;
    model.handle_input(
        model.input_epoch.get(),
        UIInput::CompositionCandidate {
            revision: revision.wrapping_sub(1),
            index: 0,
        },
        &sender,
    );
    assert!(jobs.try_recv().is_err());
    // A rejected nonmutating edit preserves the working composition and enables Finish.
    model.composition.snapshot.preedit = "nihao".into();
    model.send_engine(crate::ime::Command::Text("overflow".into()), &sender);
    let rejected = jobs.try_iter().last().unwrap();
    responses
        .send(crate::ime::Response {
            token: rejected.token,
            revision: rejected.revision,
            result: Err(crate::ime::EngineError {
                message: "Commit the current text before typing more".into(),
                recoverable: true,
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert_eq!(model.composition.snapshot.preedit, "nihao");
    assert!(model.composition.recoverable_error);
    assert!(model.send_engine(crate::ime::Command::Enter, &sender));
    let finish = jobs.try_iter().last().unwrap();
    assert!(model.composition.error.is_none());
    responses
        .send(crate::ime::Response {
            token: finish.token,
            revision: finish.revision,
            result: Ok(crate::ime::Snapshot {
                committed: "你好".into(),
                ..Default::default()
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "你好");

    // Queue-full rejects only the new key; accepted work drains and editing resumes.
    for _ in 0..32 {
        assert!(model.send_engine(crate::ime::Command::Text("n".into()), &sender));
    }
    assert!(!model.send_engine(crate::ime::Command::Text("rejected".into()), &sender));
    assert!(model.composition.recoverable_error);
    for job in jobs.try_iter() {
        responses
            .send(crate::ime::Response {
                token: job.token,
                revision: job.revision,
                result: Ok(crate::ime::Snapshot {
                    preedit: "nihao".into(),
                    ..Default::default()
                }),
            })
            .unwrap();
        model.receive_engine(&sender);
    }
    assert_eq!(model.composition.pending, 0);
    assert!(model.send_engine(crate::ime::Command::Backspace, &sender));
    let edit = jobs.try_iter().last().unwrap();
    responses
        .send(crate::ime::Response {
            token: edit.token,
            revision: edit.revision,
            result: Ok(crate::ime::Snapshot {
                preedit: "niha".into(),
                ..Default::default()
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert_eq!(model.composition.snapshot.preedit, "niha");
    // ABC transition accepts the immediately following taps and combines their text.
    model.composition.snapshot.preedit = "nihao".into();
    let epoch = model.input_epoch.get();
    model.request_transition(
        super::composition::Transition::Mode(crate::ime::Mode::Latin),
        &sender,
    );
    let finish = jobs.try_iter().last().unwrap();
    for text in ["a", "b", "c"] {
        model.handle_input(epoch, UIInput::AlternativeSelected(text.into()), &sender);
    }
    assert_eq!(model.composition.transition_input.len(), 3);
    let definition = model.app_config.get_language_manager().current_layout();
    let held = model
        .layout_state
        .widgets
        .as_ref()
        .unwrap()
        .text_keys
        .iter()
        .find(|(r, k, _)| definition.layout[*r][*k].text.as_deref() == Some("."))
        .unwrap()
        .2
        .clone();
    assert!(!held.alternatives().is_empty());
    let presses = Rc::new(Cell::new(0));
    let captured = presses.clone();
    held.connect_local("pressed", false, move |_| {
        captured.set(captured.get() + 1);
        None
    });
    press(&held);
    assert_eq!(presses.get(), 0);
    responses
        .send(crate::ime::Response {
            token: finish.token,
            revision: finish.revision,
            result: Ok(crate::ime::Snapshot {
                committed: "你好".into(),
                ..Default::default()
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "你好abc");
    assert_eq!(model.composition.mode, crate::ime::Mode::Latin);
    assert_eq!(model.input_epoch.get(), epoch);
    fn release(widget: &impl IsA<gtk::Widget>) {
        let controllers = widget.observe_controllers();
        let gesture = (0..controllers.n_items())
            .find_map(|i| controllers.item(i).and_downcast::<gtk::GestureClick>())
            .unwrap();
        gesture.emit_by_name::<()>("released", &[&1i32, &0f64, &0f64]);
    }
    release(&held);
    assert_eq!(
        presses.get(),
        1,
        "mode completion must preserve a held ordinary key"
    );
    press(&held);
    assert!(model.composition.transition_input.is_empty());
    // A release captured before the same-field mode completion remains valid.
    model.handle_input(epoch, UIInput::AlternativeSelected("d".into()), &sender);
    assert_eq!(inserted.borrow_mut().pop().unwrap(), "d");
    model.reset_input();
    release(&held);
    assert_eq!(presses.get(), 1, "focus reset must cancel the held key");
    drop(held);
    model.handle_input(epoch, UIInput::AlternativeSelected("stale".into()), &sender);
    assert!(inserted.borrow().is_empty());
    model.composition.mode = crate::ime::Mode::Native;
    model.start_composition(&sender);
    let reset_current = jobs.try_iter().last().unwrap();
    responses
        .send(crate::ime::Response {
            token: reset_current.token,
            revision: reset_current.revision,
            result: Ok(Default::default()),
        })
        .unwrap();
    model.receive_engine(&sender);
    // A failed queued control retains later accepted text until explicit retry succeeds.
    model.composition.mode = crate::ime::Mode::Latin;
    model.composition.transition_input.extend([
        crate::ime::Command::Enter,
        crate::ime::Command::Text("abc".into()),
    ]);
    fail.set(true);
    model.receive_engine(&sender);
    assert_eq!(
        model.composition.failed_output.as_ref().unwrap().control,
        Some('\n')
    );
    assert_eq!(model.composition.transition_input.len(), 1);
    assert!(inserted.borrow().is_empty());
    fail.set(false);
    model.retry_engine(&sender);
    assert_eq!(*inserted.borrow(), vec!["\n", "abc"]);
    inserted.borrow_mut().clear();
    assert!(model.composition.transition_input.is_empty());
    model.composition.mode = crate::ime::Mode::Native;
    // A failed Finish must not release queued ABC text into the old composition.
    model.composition.snapshot.preedit = "nihao".into();
    model.request_transition(
        super::composition::Transition::Mode(crate::ime::Mode::Latin),
        &sender,
    );
    let finish = jobs.try_iter().last().unwrap();
    model.handle_input(
        model.input_epoch.get(),
        UIInput::AlternativeSelected("abc".into()),
        &sender,
    );
    responses
        .send(crate::ime::Response {
            token: finish.token,
            revision: finish.revision,
            result: Err(crate::ime::EngineError {
                message: "Conversion unavailable".into(),
                recoverable: false,
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert!(inserted.borrow().is_empty());
    assert_eq!(model.composition.transition_input.len(), 1);
    assert_eq!(model.composition.snapshot.preedit, "nihao");
    model.reset_input();
    // Unknown outcome is retained but cannot be retried, even if the backend later succeeds.
    unknown.set(true);
    assert!(!model.deliver_engine(crate::ime::Snapshot {
        committed: "不重复".into(),
        ..Default::default()
    }));
    unknown.set(false);
    model.retry_engine(&sender);
    assert!(inserted.borrow().is_empty());
    assert!(model.composition.failed_output.is_some());
    model.discard_composition(&sender);
    assert!(model.composition.failed_output.is_none());
    while jobs.try_recv().is_ok() {}
    let old = reset.token;
    responses
        .send(crate::ime::Response {
            token: old,
            revision: 999,
            result: Ok(crate::ime::Snapshot {
                committed: "stale".into(),
                ..Default::default()
            }),
        })
        .unwrap();
    model.receive_engine(&sender);
    assert!(inserted.borrow().is_empty());
    model.composition.worker = None;
    model.reset_input();
    // Repeated composition renders reuse widgets, including with long phrases.
    model.render_composition(&sender);
    let mut stable = Vec::new();
    weak_tree(
        model.composition.bar.as_ref().unwrap().root.upcast_ref(),
        &mut stable,
    );
    for selected in [Some(1), None, Some(0), None] {
        model.composition.snapshot = crate::ime::Snapshot {
            preedit: "漢字".into(),
            candidates: vec!["漢字".into(), "感じ".into()],
            selected,
            ..Default::default()
        };
        model.render_composition(&sender);
        for (index, candidate) in ["漢字", "感じ"].iter().enumerate() {
            let button = stable
                .iter()
                .filter_map(|widget| widget.upgrade())
                .find(|widget| {
                    widget.is::<gtk::Button>()
                        && widget.tooltip_text().as_deref() == Some(*candidate)
                })
                .unwrap();
            assert_eq!(
                button.has_css_class("suggested-action"),
                selected == Some(index)
            );
        }
    }
    for _ in 0..100 {
        model.composition.snapshot = crate::ime::Snapshot {
            preedit: "中".repeat(1024),
            cursor: 3072,
            candidates: vec!["很长的候选词".repeat(8); 6],
            ..Default::default()
        };
        model.render_composition(&sender);
    }
    let mut current = Vec::new();
    weak_tree(
        model.composition.bar.as_ref().unwrap().root.upcast_ref(),
        &mut current,
    );
    assert_eq!(stable.len(), current.len());
    assert!(
        stable
            .iter()
            .zip(&current)
            .all(|(a, b)| a.upgrade() == b.upgrade())
    );
    model.reset_input();

    // Disable theme transitions so mapping/allocation, not animation timing, is tested.
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    // A cached language popup follows the live key grid across layouts and backdrops.
    let mut cached_selector = None;
    // Match a bottom-anchored layer surface; a top-left X11 window would place
    // the tall menu above the test screen instead of over the keyboard.
    model.container.set_valign(gtk::Align::End);
    for (keyboard_width, backdrop_width) in [(800, 800), (480, 480), (800, 1200)] {
        window.set_default_size(backdrop_width, 900);
        for language in ["ja", "no", "hi", "zh", "no"] {
            model.change_language(language, &sender).unwrap();
            model.composition.bar.take();
            model.layout_state.widgets.take();
            let grid = gtk::Box::new(gtk::Orientation::Vertical, 0);
            model.container.append(&grid);
            let definition = model.app_config.get_language_manager().current_layout();
            model.layout_state.widgets = Some(layout_builder::build_keyboard_layout(
                layout_builder::KeyboardLayoutParams {
                    container: &grid,
                    keyboard_definition: &definition,
                    geometry_unit: style::css_utils::cal_geometry_unit(260, definition.height),
                    keyboard_width,
                    sender: &sender,
                    shift_pressed: false,
                    current_language_flag: "".into(),
                    show_language_switcher: true,
                },
            ));
            model.start_composition(&sender);
            pump(Duration::from_millis(40));
            assert!(
                window.width() >= backdrop_width,
                "Switcher geometry requires a display at least1200px wide (use Xvfb1600x1200)"
            );
            super::language_selector::show_language_selector(&model, &sender);
            pump(Duration::from_millis(40));
            let popup = model
                .language_selector_popover
                .borrow()
                .as_ref()
                .unwrap()
                .clone();
            if let Some(cached) = &cached_selector {
                assert_eq!(cached, &popup);
            } else {
                cached_selector = Some(popup.clone());
            }
            let row = model
                .layout_state
                .widgets
                .as_ref()
                .unwrap()
                .rows
                .last()
                .unwrap();
            let bounds = row.compute_bounds(&window).unwrap();
            let (has_anchor, anchor) = popup.pointing_to();
            assert!(has_anchor);
            assert_eq!(
                anchor.x(),
                (bounds.x() + bounds.width() / 2.0).round() as i32
            );
            assert_eq!(anchor.y(), (bounds.y() + bounds.height()).round() as i32);
            assert_eq!(anchor.width(), 1);
            assert!(popup.is_autohide());
            let grid = popup.child().unwrap().downcast::<gtk::Grid>().unwrap();
            let mut count = 0;
            let mut child = grid.first_child();
            while let Some(button) = child {
                child = button.next_sibling();
                assert!(
                    button.is_visible() && button.width() > 0 && button.height() > 0,
                    "{language}: popup mapped={} visible={} child visible={} size={}x{}",
                    popup.is_mapped(),
                    popup.is_visible(),
                    button.is_visible(),
                    button.width(),
                    button.height()
                );
                count += 1;
            }
            assert_eq!(count, 21);
            println!(
                "Switcher {language}: keyboard={keyboard_width} backdrop={backdrop_width} anchor={},{} popup={}x{}",
                anchor.x(),
                anchor.y(),
                popup.width(),
                popup.height()
            );
            if let Ok(directory) = std::env::var("NOVAKEYS_GEOMETRY_DIR") {
                let snapshot = gtk::Snapshot::new();
                gtk::WidgetPaintable::new(Some(&popup)).snapshot(
                    &snapshot,
                    popup.width() as f64,
                    popup.height() as f64,
                );
                let node = snapshot.to_node().unwrap();
                popup
                    .renderer()
                    .unwrap()
                    .render_texture(&node, None)
                    .save_to_png(format!(
                        "{directory}/switcher-{language}-{keyboard_width}-{backdrop_width}.png"
                    ))
                    .unwrap();
            }
            model.reset_input();
            assert!(!popup.is_visible());
        }
    }
    drop(cached_selector);
    let popup = model.language_selector_popover.borrow_mut().take().unwrap();
    let mut popup_widgets = Vec::new();
    weak_tree(popup.upcast_ref(), &mut popup_widgets);
    popup.unparent();
    drop(popup);
    drain();
    assert!(
        popup_widgets
            .iter()
            .all(|widget| widget.upgrade().is_none())
    );
    model.container.set_valign(gtk::Align::Fill);
    window.set_default_size(800, 260);
    model.rebuild_keyboard(&sender);

    // User CSS may style the keyboard, but cannot paint the click-through root.
    let opaque_root = gtk::CssProvider::new();
    opaque_root.load_from_data("window.novakeys-overlay { background: red; border: 8px solid red; box-shadow: 0 0 10px red; }");
    let display = gtk::gdk::Display::default().unwrap();
    gtk::style_context_add_provider_for_display(
        &display,
        &opaque_root,
        gtk::STYLE_PROVIDER_PRIORITY_USER,
    );
    // The surface stays stable; direct mode clips away the two unused rows.
    for language in ["zh", "ja", "ko"] {
        let view = super::composition_view::View::new(language, &sender);
        let mut state = super::composition::Composition::default();
        for width in [800, 480] {
            let preview = Some(std::env::var("NOVAKEYS_GEOMETRY_DIR").ok()).map(|directory| {
                let preview: gtk::Window = super::overlay_window::Window::new().upcast();
                preview.set_default_size(width, 900);
                preview.set_resizable(false);
                let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
                backdrop.add_css_class("novakeys-backdrop");
                backdrop.set_valign(gtk::Align::End);
                let keyboard = gtk::Box::new(gtk::Orientation::Vertical, 0);
                keyboard.add_css_class("novakeys-keyboard");
                backdrop.append(&keyboard);
                keyboard.append(&view.root);
                let grid = gtk::Box::new(gtk::Orientation::Vertical, 0);
                keyboard.append(&grid);
                let definition = model
                    .app_config
                    .get_language_manager()
                    .layout(language)
                    .unwrap();
                let widgets =
                    layout_builder::build_keyboard_layout(layout_builder::KeyboardLayoutParams {
                        container: &grid,
                        keyboard_definition: &definition,
                        geometry_unit: style::css_utils::cal_geometry_unit(260, definition.height),
                        keyboard_width: width,
                        sender: &sender,
                        shift_pressed: false,
                        current_language_flag: "".into(),
                        show_language_switcher: true,
                    });
                widgets.refresh(&definition, false, true);
                preview.set_child(Some(&backdrop));
                preview.present();
                pump(Duration::from_millis(80));
                (preview, keyboard, widgets, directory)
            });
            let mut expected = None;
            let mut mode_position = None;
            for phase in 0..9 {
                state.mode = if phase == 4 {
                    crate::ime::Mode::Latin
                } else {
                    crate::ime::Mode::Native
                };
                state.pending = usize::from(phase == 1);
                state.snapshot = if phase >= 2 {
                    crate::ime::Snapshot {
                        preedit: "日本語の長い入力文".repeat(10),
                        cursor: 3,
                        candidates: vec![
                            "日本語の長い候補".repeat(4);
                            if phase == 2 { 1 } else { 6 }
                        ],
                        segments: if phase == 3 { 3 } else { 0 },
                        has_previous: phase == 3,
                        has_next: phase == 3,
                        ..Default::default()
                    }
                } else {
                    Default::default()
                };
                state.error =
                    (phase >= 6).then(|| "Input rejected; existing text is preserved".into());
                state.failed_output = (phase >= 7).then(crate::ime::Snapshot::default);
                state.failed_unknown = phase == 8;
                state.transition = (phase == 1).then_some(super::composition::Transition::Mode(
                    crate::ime::Mode::Latin,
                ));
                view.update(&state, phase == 5);
                let minimum_width = view.root.measure(gtk::Orientation::Horizontal, -1).0;
                assert!(
                    minimum_width <= width,
                    "{language} minimum {minimum_width} exceeds {width}"
                );
                let height = view.root.measure(gtk::Orientation::Vertical, width).1;
                assert!(
                    height <= if width == 800 { 160 } else { 210 },
                    "{language} compact panel too tall: {height}"
                );
                let mut tree = Vec::new();
                weak_tree(view.root.upcast_ref(), &mut tree);
                for widget in tree.into_iter().filter_map(|weak| weak.upgrade()) {
                    if let Ok(button) = widget.downcast::<gtk::Button>() {
                        assert!(button.width_request() >= 44 && button.height_request() >= 44);
                        if button.opacity() == 0.0 {
                            assert!(!button.can_target());
                        }
                    }
                }
                assert_eq!(
                    *expected.get_or_insert(height),
                    height,
                    "{language} width {width} phase {phase} resized the stable surface"
                );
                if let Some((preview, _, _, directory)) = &preview {
                    super::overlay_window::set_visible_top(preview, view.visible_top().as_ref());
                    if phase == 5 {
                        preview.set_visible(false);
                        preview.present();
                    }
                    pump(Duration::from_millis(30));
                    let modes = view.root.last_child().unwrap().last_child().unwrap();
                    let bounds = modes.compute_bounds(preview).unwrap();
                    if matches!(phase, 4 | 5) {
                        let root_bounds = view.root.compute_bounds(preview).unwrap();
                        let visible_height =
                            root_bounds.y() + root_bounds.height() - bounds.y() + 4.0;
                        assert!(
                            visible_height <= 54.0,
                            "{language} direct visible height {visible_height}"
                        );
                    }
                    let position = (bounds.x(), bounds.y(), bounds.height());
                    assert_eq!(
                        *mode_position.get_or_insert(position),
                        position,
                        "{language} width {width} phase {phase} moved modes"
                    );
                    let snapshot = gtk::Snapshot::new();
                    gtk::WidgetPaintable::new(Some(preview)).snapshot(
                        &snapshot,
                        preview.width() as f64,
                        preview.height() as f64,
                    );
                    let node = snapshot.to_node().unwrap();
                    let viewport = gtk::graphene::Rect::new(
                        0.0,
                        0.0,
                        preview.width() as f32,
                        preview.height() as f32,
                    );
                    let texture = preview
                        .renderer()
                        .unwrap()
                        .render_texture(&node, Some(&viewport));
                    let stride = texture.width() as usize * 4;
                    let mut pixels = vec![0; stride * texture.height() as usize];
                    texture.download(&mut pixels, stride);
                    let root_bounds = view.root.compute_bounds(preview).unwrap();
                    let sample_y = root_bounds.y() as usize + 12;
                    let alpha = pixels[sample_y * stride + 12 * 4 + 3];
                    assert_eq!(
                        alpha == 0,
                        matches!(phase, 4 | 5),
                        "{language} phase {phase} must clip only direct/private upper pixels"
                    );
                    if matches!(phase, 3 | 4 | 7)
                        && let Some(directory) = directory
                    {
                        let snapshot = gtk::Snapshot::new();
                        gtk::WidgetPaintable::new(Some(preview)).snapshot(
                            &snapshot,
                            preview.width() as f64,
                            preview.height() as f64,
                        );
                        let node = snapshot.to_node().unwrap();
                        preview
                            .renderer()
                            .unwrap()
                            .render_texture(&node, None)
                            .save_to_png(format!(
                                "{directory}/compact-{language}-{width}-{phase}.png"
                            ))
                            .unwrap();
                    }
                }
            }
            println!(
                "Stable composition geometry {language}: width={width} height={}",
                expected.unwrap()
            );
            if let Some((preview, keyboard, _, _)) = preview {
                keyboard.remove(&view.root);
                preview.close();
                drain();
            }
        }
    }

    gtk::style_context_remove_provider_for_display(&display, &opaque_root);
    let ordinary_recovery = super::composition_view::View::new("no", &sender);
    let mut recovered = super::composition::Composition {
        error: Some("Rejected".into()),
        ..Default::default()
    };
    ordinary_recovery.update(&recovered, false);
    assert!(ordinary_recovery.root.is_visible());
    recovered.error = None;
    ordinary_recovery.update(&recovered, false);
    assert!(!ordinary_recovery.root.is_visible());
    assert!(ordinary_recovery.visible_top().is_none());

    let switcher = model
        .app_config
        .get_language_manager()
        .should_show_language_switcher();
    model.composition.failed_output = Some(crate::ime::Snapshot {
        committed: "keep".into(),
        ..Default::default()
    });
    assert!(model.set_switcher_visibility(!switcher, &sender).is_err());
    assert!(model.handle_config_reload(&sender).is_err());
    assert_eq!(
        model.composition.failed_output.as_ref().unwrap().committed,
        "keep"
    );
    assert_eq!(
        model
            .app_config
            .get_language_manager()
            .should_show_language_switcher(),
        switcher
    );
    assert!(model.set_switcher_visibility(switcher, &sender).is_ok());
    model.reset_input();

    // A composition-button press from an old focus cannot activate on release.
    {
        fn ready<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
            let mut future = std::pin::pin!(future);
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => Some(value),
                std::task::Poll::Pending => None,
            }
        }
        let (tx, rx) = relm4::channel();
        let epoch = Rc::new(Cell::new(1));
        let input = main_view::InputSender {
            sender: tx,
            epoch: epoch.clone(),
        };
        let view = super::composition_view::View::new("zh", &input);
        let row = view.root.last_child().unwrap().last_child().unwrap();
        let mut child = row.first_child();
        let mut mode = None;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<gtk::Button>()
                && button.label().as_deref() == Some("ABC")
            {
                mode = Some(button);
                break;
            }
        }
        let mode = mode.unwrap();
        let controllers = mode.observe_controllers();
        let gesture = (0..controllers.n_items())
            .find_map(|i| {
                controllers
                    .item(i)
                    .and_downcast::<gtk::GestureClick>()
                    .filter(|gesture| gesture.propagation_phase() == gtk::PropagationPhase::Capture)
            })
            .unwrap();
        gesture.emit_by_name::<()>("pressed", &[&1i32, &0f64, &0f64]);
        epoch.set(2);
        view.cancel_interaction();
        mode.emit_clicked();
        assert!(ready(rx.recv()).is_none());
        gesture.emit_by_name::<()>("pressed", &[&1i32, &0f64, &0f64]);
        mode.emit_clicked();
        assert!(matches!(
            ready(rx.recv()).flatten(),
            Some(UIMessage::Input {
                epoch: 2,
                event: UIInput::CompositionMode(crate::ime::Mode::Latin)
            })
        ));
    }

    // Render expanded layouts and the longest popup at desktop and narrow widths.
    for width in [800, 480, 240] {
        for language in ["hi", "ja", "ar"] {
            let definition = model
                .app_config
                .get_language_manager()
                .layout(language)
                .unwrap();
            let preview = gtk::Window::builder()
                .default_width(width)
                .default_height(320)
                .build();
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            preview.set_child(Some(&container));
            let widgets =
                layout_builder::build_keyboard_layout(layout_builder::KeyboardLayoutParams {
                    container: &container,
                    keyboard_definition: &definition,
                    geometry_unit: style::css_utils::cal_geometry_unit(320, definition.height),
                    keyboard_width: width,
                    sender: &sender,
                    shift_pressed: false,
                    current_language_flag: "".into(),
                    show_language_switcher: true,
                });
            widgets.refresh(&definition, false, false);
            preview.present();
            pump(Duration::from_millis(100));
            let minimum_key_width = widgets
                .text_keys
                .iter()
                .map(|(_, _, key)| key.width())
                .min()
                .unwrap();
            println!(
                "Geometry {language}: requested={width} actual={}x{} minimum_key_width={minimum_key_width}",
                preview.width(),
                preview.height()
            );
            assert!(
                widgets
                    .text_keys
                    .iter()
                    .all(|(_, _, key)| key.width() > 0 && key.height() > 0)
            );
            if let Ok(directory) = std::env::var("NOVAKEYS_GEOMETRY_DIR") {
                let snapshot = gtk::Snapshot::new();
                gtk::WidgetPaintable::new(Some(&preview)).snapshot(
                    &snapshot,
                    preview.width() as f64,
                    preview.height() as f64,
                );
                let node = snapshot.to_node().unwrap();
                preview
                    .renderer()
                    .unwrap()
                    .render_texture(&node, None)
                    .save_to_png(format!("{directory}/{language}-{width}.png"))
                    .unwrap();
            }
            if language == "ar" {
                let button = &widgets
                    .text_keys
                    .iter()
                    .max_by_key(|(_, _, key)| key.alternatives().len())
                    .unwrap()
                    .2;
                assert_eq!(button.alternatives().len(), 8);
                press(button);
                pump(Duration::from_millis(250));
                let popup = button
                    .last_child()
                    .unwrap()
                    .downcast::<gtk::Popover>()
                    .unwrap();
                println!(
                    "Popup requested={width} actual={}x{}",
                    popup.width(),
                    popup.height()
                );
                if let Ok(directory) = std::env::var("NOVAKEYS_GEOMETRY_DIR") {
                    let snapshot = gtk::Snapshot::new();
                    gtk::WidgetPaintable::new(Some(&popup)).snapshot(
                        &snapshot,
                        popup.width() as f64,
                        popup.height() as f64,
                    );
                    let node = snapshot.to_node().unwrap();
                    popup
                        .renderer()
                        .unwrap()
                        .render_texture(&node, None)
                        .save_to_png(format!("{directory}/popup-{width}.png"))
                        .unwrap();
                }
                popup.popdown();
            }
            widgets.cancel_interaction();
            drop(widgets);
            preview.close();
            drain();
        }
    }

    let languages: Vec<_> = model
        .app_config
        .get_language_manager()
        .get_available_languages()
        .iter()
        .map(|lang| lang.code.clone())
        .collect();
    let mut checked = 0;
    for _ in 0..10 {
        for language in &languages {
            if language == model.current_language_code() {
                continue;
            }
            let mut old = Vec::new();
            weak_tree(
                model
                    .layout_state
                    .widgets
                    .as_ref()
                    .unwrap()
                    .container
                    .upcast_ref(),
                &mut old,
            );
            model.change_language(language, &sender).unwrap();
            drain();
            assert!(
                old.iter().all(|widget| widget.upgrade().is_none()),
                "old {language} tree retained objects"
            );
            checked += old.len();
        }
    }
    // Actual reloads preserve same-language modes, but reset language-specific modes
    // when the configured language changes. Invalid reloads leave the active tree intact.
    for (language, mode, next) in [
        ("ja", crate::ime::Mode::Katakana, "zh"),
        ("zh", crate::ime::Mode::Traditional, "ko"),
        ("ko", crate::ime::Mode::Latin, "ja"),
    ] {
        model.change_language(language, &sender).unwrap();
        model.composition.mode = mode;
        std::fs::write(&config_path, format!("default_language = \"{language}\"\n")).unwrap();
        model.handle_config_reload(&sender).unwrap();
        assert_eq!(model.current_language_code(), language);
        assert_eq!(model.composition.mode, mode);
        let epoch = model.input_epoch.get();
        let container = model.container.clone();
        for invalid in ["invalid TOML", "default_language = \"missing\""] {
            std::fs::write(&config_path, invalid).unwrap();
            assert!(model.handle_config_reload(&sender).is_err());
            assert_eq!(model.current_language_code(), language);
            assert_eq!(model.composition.mode, mode);
            assert_eq!(model.input_epoch.get(), epoch);
            assert_eq!(model.container, container);
        }
        std::fs::write(&config_path, format!("default_language = \"{next}\"\n")).unwrap();
        model.handle_config_reload(&sender).unwrap();
        assert_eq!(model.current_language_code(), next);
        assert_eq!(model.composition.mode, crate::ime::Mode::Native);
    }
    eprintln!("Verified same-language, cross-language and invalid configuration reloads");

    let mut old = Vec::new();
    weak_tree(
        model
            .layout_state
            .widgets
            .as_ref()
            .unwrap()
            .container
            .upcast_ref(),
        &mut old,
    );
    model.composition.bar.take();
    model.layout_state.widgets.take();
    drain();
    assert!(old.iter().all(|widget| widget.upgrade().is_none()));
    window.destroy();
    drop(model);
    assert!(signal_sources.iter().all(|source| source.is_destroyed()));
    eprintln!("Verified release of {checked} GTK objects across 210 language transitions");
}
