//! GTK component lifecycle and dispatch.
use super::{message_router, state::LayoutState, window_setup};
use crate::{
    config::AppConfig,
    constants::css_classes,
    ipc::Ipc,
    service::host::KeyboardHandle,
    types::{KeyIndex, LanguageCode, RowIndex},
    visibility::VisibilityManager,
};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Debug)]
pub enum UIMessage {
    EngineReady,
    ContextChanged(crate::input_detection::InputContext),
    RemoteCommand(
        crate::service::IPCMessage,
        std::sync::mpsc::SyncSender<crate::service::IPCResponse>,
    ),
    AppQuit,
    Input {
        epoch: u64,
        event: UIInput,
    },
}

#[derive(Debug, Clone)]
pub enum UIInput {
    TextKeyAtPosition {
        row_index: RowIndex,
        key_index: KeyIndex,
    },
    ActionKey(String),
    AlternativeSelected(String),
    CompositionMode(crate::ime::Mode),
    CompositionCommand(crate::ime::Command),
    CompositionCandidate {
        revision: u64,
        index: usize,
    },
    RetryComposition,
    DiscardComposition,
    SetLanguage(LanguageCode),
    ShowLanguageSelector,
}

// GTK callbacks read the current context epoch at emission; queued old events expire on reset.
#[derive(Clone)]
pub(crate) struct InputSender {
    pub sender: relm4::Sender<UIMessage>,
    pub epoch: Rc<Cell<u64>>,
}
impl InputSender {
    pub fn emit(&self, event: UIInput) {
        self.sender.emit(UIMessage::Input {
            epoch: self.epoch.get(),
            event,
        });
    }
}

pub struct UIModel {
    pub(super) input_context: crate::input_detection::InputContext,
    pub(super) input_epoch: Rc<Cell<u64>>,
    pub(super) keyboard_handle: Box<dyn KeyboardHandle>,
    pub(super) window: gtk::Window,
    pub(super) app_config: AppConfig,
    pub(super) container: gtk::Box,
    pub(super) window_height: i32,
    pub(super) layout_state: LayoutState,
    pub(super) composition: super::composition::Composition,
    pub(super) shutdown_sources: Vec<gtk::glib::SourceId>,
    pub(super) css_providers: Vec<gtk::CssProvider>,
    pub(super) language_selector_popover: RefCell<Option<gtk::Popover>>,
}

impl UIModel {
    pub(super) fn install_shutdown_handlers(&mut self, sender: &relm4::Sender<UIMessage>) {
        use rustix::process::Signal;
        for signal in [Signal::INT, Signal::TERM, Signal::HUP] {
            let sender = sender.clone();
            self.shutdown_sources.push(glib_unix::unix_signal_add_local(
                signal.as_raw(),
                move || {
                    sender.emit(UIMessage::AppQuit);
                    gtk::glib::ControlFlow::Continue
                },
            ));
        }
    }
}

impl Drop for UIModel {
    fn drop(&mut self) {
        for source in self.shutdown_sources.drain(..) {
            source.remove();
        }
    }
}

impl SimpleComponent for UIModel {
    type Init = (
        Box<dyn KeyboardHandle>,
        Ipc,
        Arc<dyn VisibilityManager>,
        AppConfig,
    );
    type Input = UIMessage;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();
    fn init_root() -> Self::Root {
        super::overlay_window::Window::new().upcast()
    }
    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (keyboard_handle, ipc, visibility_manager, app_config) = init;
        let css_providers = window_setup::setup_css_providers(&app_config)
            .unwrap_or_else(|e| crate::error::ErrorHandler::fatal(e));
        window_setup::install_css_providers(&css_providers);
        let (window_height, width, stretch) =
            window_setup::configure_window_layout(&root, &app_config);
        let container = window_setup::create_container_hierarchy(&root, width, stretch);
        if app_config.is_dark_mode() {
            root.add_css_class(css_classes::DARK_MODE);
        }
        let mut model = Self {
            input_context: crate::input_detection::InputContext::default(),
            input_epoch: Rc::new(Cell::new(0)),
            keyboard_handle,
            window: root,
            app_config,
            container,
            window_height,
            css_providers,
            shutdown_sources: Vec::new(),
            layout_state: LayoutState::default(),
            composition: super::composition::Composition::default(),
            language_selector_popover: RefCell::new(None),
        };
        let engine_sender = sender.input_sender().clone();
        model.composition.worker = Some(
            crate::ime::Worker::new(move || engine_sender.emit(UIMessage::EngineReady))
                .unwrap_or_else(|error| crate::error::ErrorHandler::fatal(error)),
        );
        model.rebuild_keyboard(&InputSender {
            sender: sender.input_sender().clone(),
            epoch: model.input_epoch.clone(),
        });
        visibility_manager
            .set_controller(Box::new(super::UIVisibilityController::new(sender.clone())));
        model.install_shutdown_handlers(sender.input_sender());
        message_router::setup_ipc_listener(ipc, sender);
        ComponentParts { model, widgets: () }
    }
    fn update(&mut self, msg: UIMessage, sender: ComponentSender<Self>) {
        let input = InputSender {
            sender: sender.input_sender().clone(),
            epoch: self.input_epoch.clone(),
        };
        match msg {
            UIMessage::EngineReady => self.receive_engine(&input),
            UIMessage::ContextChanged(context) => {
                self.reset_input();
                self.input_context = context;
                self.keyboard_handle.set_context(context);
                self.refresh_keys();
                self.start_composition(&input);
                self.window.set_visible(context.active);
            }
            UIMessage::RemoteCommand(command, reply) => {
                use crate::service::{CommandKind, IPCResponse};
                let result = match command.kind {
                    CommandKind::SetLanguage => {
                        if let Some(code) = command.value {
                            self.request_transition(
                                super::composition::Transition::Language(code, Some(reply)),
                                &input,
                            );
                        } else {
                            let _ = reply.send(IPCResponse::error("Missing language code".into()));
                        }
                        return;
                    }
                    CommandKind::ReloadConfig => self.handle_config_reload(&input).map(|()| None),
                    CommandKind::GetStatus => (|| {
                        let status = self.status_json()?;
                        super::status::write_status_snapshot(&status)?;
                        Ok(Some(status))
                    })(),
                    CommandKind::HideLanguageSwitcher => {
                        self.set_switcher_visibility(false, &input).map(|()| None)
                    }
                    CommandKind::ShowLanguageSwitcher => {
                        self.set_switcher_visibility(true, &input).map(|()| None)
                    }
                    CommandKind::Close => Ok(None),
                };
                let response = match result {
                    Ok(status) => IPCResponse {
                        status,
                        error: None,
                    },
                    Err(error) => IPCResponse::error(error),
                };
                let _ = reply.send(response);
            }
            UIMessage::Input { epoch, event } => self.handle_input(epoch, event, &input),
            UIMessage::AppQuit => {
                self.composition.worker.take();
                self.keyboard_handle.destroy();
                relm4::main_application().quit();
            }
        }
    }
    fn update_view(&self, _: &mut (), _: ComponentSender<Self>) {}
}

impl UIModel {
    pub fn window_ref(&self) -> &gtk::Window {
        &self.window
    }
    pub fn app_config_ref(&self) -> &AppConfig {
        &self.app_config
    }
    pub fn language_selector_popover_ref(&self) -> &RefCell<Option<gtk::Popover>> {
        &self.language_selector_popover
    }
    pub fn current_language_code(&self) -> &str {
        self.app_config
            .get_language_manager()
            .get_current_language()
    }
    pub(super) fn status_json(&self) -> Result<serde_json::Value, String> {
        let languages = self
            .app_config
            .get_language_manager()
            .get_available_languages()
            .iter()
            .map(|lang| lang.code.as_str())
            .collect();
        serde_json::to_value(super::status::Status {
            current_language: self.current_language_code(),
            languages,
            shift_pressed: self.layout_state.shift_pressed,
            composition_enabled: self.composition_enabled(),
        })
        .map_err(|error| error.to_string())
    }
}
