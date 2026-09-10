//! One input-method-v2 owner for focus, completed UTF-8 text and physical controls.
use crate::{
    native::{VirtualKeyboardV1, control_code},
    service::host::{DeliveryError, KeyboardHandle},
};
use anyhow::{Context, Result};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    protocol::{wl_registry, wl_seat::WlSeat},
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{Event as InputMethodEvent, ZwpInputMethodV2},
    },
    zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
    },
};

const MAX_TEXT_BYTES: usize = 4096;
// input-method-v2 caps each commit_string at 4000 UTF-8 bytes.
const MAX_PROTOCOL_BYTES: usize = 4000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputContext {
    pub generation: u64,
    pub active: bool,
    pub content_hint: u32,
    pub content_purpose: u32,
}
impl InputContext {
    pub fn sensitive(self) -> bool {
        // text-input-v3: hidden_text, sensitive_data; password and PIN purposes.
        self.content_hint & (0x40 | 0x80) != 0 || matches!(self.content_purpose, 8 | 9)
    }
}
#[derive(Default)]
struct ContextState {
    context: InputContext,
    serial: u32,
    failed: bool,
    closed: bool,
    transition: bool,
}
impl ContextState {
    fn validate(&self, expected: InputContext) -> Result<(), DeliveryError> {
        if self.closed {
            return Err(DeliveryError::Rejected("Input method is closed".into()));
        }
        if self.failed {
            return Err(DeliveryError::OutcomeUnknown(
                "Input transport failed; restart required; previous outcome unknown".into(),
            ));
        }
        if self.transition {
            return Err(DeliveryError::Rejected("Input context is changing".into()));
        }
        if !self.context.active
            || !expected.active
            || self.context.generation != expected.generation
        {
            return Err(DeliveryError::Rejected(
                "Input context expired or inactive".into(),
            ));
        }
        Ok(())
    }
    fn expire(&mut self) {
        self.closed = true;
        self.context.active = false;
        self.context.generation = self.context.generation.wrapping_add(1);
    }
}
pub trait TextInputHandler {
    fn on_context_changed(&mut self, context: InputContext);
}
type Handler = Arc<Mutex<Option<Box<dyn TextInputHandler + Send>>>>;

#[derive(Clone)]
pub struct InputMethodHandle {
    inner: Arc<Output>,
}
const OUTPUT_QUEUE_CAPACITY: usize = 32;
struct Output {
    requests: SyncSender<Request>,
    wake: UnixStream,
    state: Arc<Mutex<ContextState>>,
}
struct Request {
    expected: InputContext,
    command: Command,
    reply: SyncSender<Result<(), DeliveryError>>,
}
enum Command {
    Text(String),
    Control { key: char, count: u32 },
    Shutdown,
}
impl InputMethodHandle {
    pub fn context(&self) -> InputContext {
        self.inner.state.lock().unwrap().context
    }
    fn request(&self, expected: InputContext, command: Command) -> Result<(), DeliveryError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.inner
            .requests
            .try_send(Request {
                expected,
                command,
                reply,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    DeliveryError::Rejected("Input output queue is full".into())
                }
                mpsc::TrySendError::Disconnected(_) => {
                    DeliveryError::Rejected("Input method is closed".into())
                }
            })?;
        // A full nonblocking wake socket already contains a notification. The
        // request channel is authoritative; the socket never carries commands.
        let mut wake = &self.inner.wake;
        loop {
            match wake.write(&[1]) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    return Err(DeliveryError::OutcomeUnknown(format!(
                        "Input owner wake failed; outcome unknown: {error}"
                    )));
                }
            }
        }
        result.recv().unwrap_or_else(|_| {
            Err(DeliveryError::OutcomeUnknown(
                "Input owner stopped; outcome unknown".into(),
            ))
        })
    }
    fn insert_text(&self, expected: InputContext, text: &str) -> Result<(), DeliveryError> {
        validate_text(text)?;
        self.request(expected, Command::Text(text.to_owned()))
    }
    fn control(&self, expected: InputContext, key: char, count: u32) -> Result<(), DeliveryError> {
        control_code(key)
            .ok_or_else(|| DeliveryError::Rejected("Unsupported control key".into()))?;
        if count > 1024 {
            return Err(DeliveryError::Rejected("Too many control keys".into()));
        }
        self.request(expected, Command::Control { key, count })
    }
    fn destroy(&self, expected: InputContext) {
        let _ = self.request(expected, Command::Shutdown);
    }
}
// Only the event-loop owner can marshal protocol output or update its serial.
struct ProtocolOutput {
    connection: Connection,
    input_method: ZwpInputMethodV2,
    controls: VirtualKeyboardV1,
    state: Arc<Mutex<ContextState>>,
}
impl ProtocolOutput {
    fn flush(&self, state: &mut ContextState) -> Result<(), DeliveryError> {
        self.connection.flush().map_err(|error| {
            state.failed = true;
            DeliveryError::OutcomeUnknown(format!("Input flush failed; outcome unknown: {error}"))
        })
    }
    fn insert_text(&self, expected: InputContext, mut text: &str) -> Result<(), DeliveryError> {
        validate_text(text)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| DeliveryError::Rejected("Input state poisoned".into()))?;
        state.validate(expected)?;
        while !text.is_empty() {
            let end = chunk_end(text);
            self.input_method.commit_string(text[..end].to_owned());
            self.input_method.commit(state.serial);
            text = &text[end..];
        }
        // Success means locally flushed, not acknowledged by the application.
        self.flush(&mut state)
    }
    fn control(&self, expected: InputContext, key: char, count: u32) -> Result<(), DeliveryError> {
        let code = control_code(key)
            .ok_or_else(|| DeliveryError::Rejected("Unsupported control key".into()))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| DeliveryError::Rejected("Input state poisoned".into()))?;
        state.validate(expected)?;
        for _ in 0..count {
            self.controls.key(code);
        }
        self.flush(&mut state)
    }
}
fn chunk_end(text: &str) -> usize {
    let mut end = text.len().min(MAX_PROTOCOL_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}
fn validate_text(text: &str) -> Result<(), DeliveryError> {
    if text.len() > MAX_TEXT_BYTES || text.contains('\0') {
        Err(DeliveryError::Rejected(
            "Text exceeds 4096 bytes or contains NUL".into(),
        ))
    } else {
        Ok(())
    }
}
pub struct InputMethodKeyboard {
    handle: InputMethodHandle,
    expected: InputContext,
}
impl KeyboardHandle for InputMethodKeyboard {
    fn insert_text(&mut self, text: &str) -> Result<(), DeliveryError> {
        self.handle.insert_text(self.expected, text)
    }
    fn control(&mut self, key: char) -> Result<(), DeliveryError> {
        self.handle.control(self.expected, key, 1)
    }
    fn delete_text(&mut self, before: u32, after: u32) -> Result<(), DeliveryError> {
        if before > 1024 || after != 0 {
            return Err(DeliveryError::Rejected(
                "Only up to 1024 backspaces are supported".into(),
            ));
        }
        self.handle.control(self.expected, '\u{0008}', before)
    }
    fn set_context(&mut self, context: InputContext) {
        self.expected = context;
    }
    fn destroy(&mut self) {
        self.handle.destroy(self.expected);
    }
}
pub struct InputDetectionManager {
    handler: Handler,
    output: InputMethodHandle,
}
impl InputDetectionManager {
    pub fn new() -> Result<Self> {
        let handler = Arc::new(Mutex::new(None));
        let (listener, output) = Listener::new(handler.clone())?;
        std::thread::spawn(move || {
            if let Err(error) = listener.run() {
                tracing::warn!(%error, "Input method event loop stopped");
            }
        });
        Ok(Self { handler, output })
    }
    pub fn keyboard(&self) -> InputMethodKeyboard {
        InputMethodKeyboard {
            handle: self.output.clone(),
            expected: InputContext::default(),
        }
    }
    pub fn set_handler(&self, handler: Box<dyn TextInputHandler + Send>) {
        let mut slot = self.handler.lock().unwrap();
        *slot = Some(handler);
        slot.as_mut()
            .unwrap()
            .on_context_changed(self.output.context());
    }
}
struct MethodState {
    manager: Option<ZwpInputMethodManagerV2>,
    vk_manager: Option<ZwpVirtualKeyboardManagerV1>,
    seat: Option<WlSeat>,
    shared: Arc<Mutex<ContextState>>,
    handler: Handler,
    pending: InputContext,
    reset: bool,
    unavailable: bool,
}
impl MethodState {
    fn new(shared: Arc<Mutex<ContextState>>, handler: Handler) -> Self {
        Self {
            manager: None,
            vk_manager: None,
            seat: None,
            shared,
            handler,
            pending: InputContext::default(),
            reset: false,
            unavailable: false,
        }
    }
    fn notify(&self, _context: InputContext) {
        if let Some(handler) = self.handler.lock().unwrap().as_mut() {
            let context = self.shared.lock().unwrap().context;
            handler.on_context_changed(context);
        }
    }
    fn disconnect(&mut self) {
        let context = {
            let mut state = self.shared.lock().unwrap();
            state.expire();
            state.context
        };
        self.notify(context);
    }
    fn on_event(&mut self, event: InputMethodEvent) {
        match event {
            InputMethodEvent::Activate => {
                self.shared.lock().unwrap().transition = true;
                self.pending = InputContext {
                    active: true,
                    ..InputContext::default()
                };
                self.reset = true;
            }
            InputMethodEvent::Deactivate => {
                self.shared.lock().unwrap().transition = true;
                self.pending.active = false;
                self.reset = true;
            }
            InputMethodEvent::ContentType { hint, purpose } => {
                self.shared.lock().unwrap().transition = true;
                self.pending.content_hint = hint.into();
                self.pending.content_purpose = purpose.into();
            }
            InputMethodEvent::TextChangeCause { cause } if u32::from(cause) == 1 => {
                self.shared.lock().unwrap().transition = true;
                self.reset = true;
            }
            InputMethodEvent::Done => {
                let changed = {
                    let mut state = self.shared.lock().unwrap();
                    state.serial = state.serial.wrapping_add(1);
                    state.transition = false;
                    let changed = self.reset
                        || state.context.active != self.pending.active
                        || state.context.content_hint != self.pending.content_hint
                        || state.context.content_purpose != self.pending.content_purpose;
                    if changed {
                        self.pending.generation = state.context.generation.wrapping_add(1);
                        state.context = self.pending;
                    }
                    changed.then_some(state.context)
                };
                self.reset = false;
                if let Some(context) = changed {
                    self.notify(context);
                }
            }
            InputMethodEvent::Unavailable => {
                self.unavailable = true;
                self.disconnect();
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for MethodState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwp_input_method_manager_v2" => {
                    state.manager = Some(registry.bind(name, version.min(1), qh, ()))
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    state.vk_manager = Some(registry.bind(name, version.min(1), qh, ()))
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<ZwpInputMethodV2, ()> for MethodState {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodV2,
        event: InputMethodEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.on_event(event);
    }
}
wayland_client::delegate_noop!(MethodState: ignore WlSeat);
wayland_client::delegate_noop!(MethodState: ignore ZwpInputMethodManagerV2);
wayland_client::delegate_noop!(MethodState: ignore ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(MethodState: ignore ZwpVirtualKeyboardV1);
struct Listener {
    queue: EventQueue<MethodState>,
    state: MethodState,
    output: ProtocolOutput,
    requests: Receiver<Request>,
    wake: UnixStream,
}
impl Listener {
    fn new(handler: Handler) -> Result<(Self, InputMethodHandle)> {
        let connection = Connection::connect_to_env().context("Connect input method to Wayland")?;
        let mut queue = connection.new_event_queue();
        let qh = queue.handle();
        let _registry = connection.display().get_registry(&qh, ());
        let shared = Arc::new(Mutex::new(ContextState::default()));
        let mut state = MethodState::new(shared.clone(), handler);
        queue.roundtrip(&mut state)?;
        let manager = state
            .manager
            .as_ref()
            .context("Compositor lacks input-method-v2")?;
        let seat = state.seat.as_ref().context("Compositor has no seat")?;
        let vk_manager = state
            .vk_manager
            .as_ref()
            .context("Compositor lacks virtual-keyboard-v1")?;
        let input_method = manager.get_input_method(seat, &qh, ());
        let controls = VirtualKeyboardV1::new(vk_manager.create_virtual_keyboard(seat, &qh, ()))?;
        queue.roundtrip(&mut state)?;
        anyhow::ensure!(
            !state.unavailable,
            "Input method unavailable; another input method may own the seat"
        );
        let (requests, receiver) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        let (wake, listener_wake) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        listener_wake.set_nonblocking(true)?;
        let output = ProtocolOutput {
            connection,
            input_method,
            controls,
            state: shared.clone(),
        };
        let handle = InputMethodHandle {
            inner: Arc::new(Output {
                requests,
                wake,
                state: shared,
            }),
        };
        Ok((
            Self {
                queue,
                state,
                output,
                requests: receiver,
                wake: listener_wake,
            },
            handle,
        ))
    }
    fn run(mut self) -> Result<()> {
        let result = self.run_inner();
        self.state.disconnect();
        self.output.input_method.destroy();
        self.output.controls.destroy();
        let _ = self.output.connection.flush();
        // Returning drops the request receiver, releasing queued callers too.
        result
    }
    fn run_inner(&mut self) -> Result<()> {
        let mut pending = None;
        loop {
            self.queue.dispatch_pending(&mut self.state)?;
            anyhow::ensure!(!self.state.unavailable, "Input method became unavailable");
            if pending.is_none() {
                match self.requests.try_recv() {
                    Ok(request) => pending = Some(request),
                    Err(TryRecvError::Disconnected) => return Ok(()),
                    Err(TryRecvError::Empty) => {}
                }
            }
            let Some(guard) = self.queue.prepare_read() else {
                continue;
            };
            let mut fds = [
                PollFd::new(&self.output.connection, PollFlags::IN),
                PollFd::new(&self.wake, PollFlags::IN),
            ];
            let immediate = Timespec::default();
            match poll(&mut fds, pending.as_ref().map(|_| &immediate)) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => {
                    drop(guard);
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
            let wayland_ready = !fds[0].revents().is_empty();
            let wake_ready = !fds[1].revents().is_empty();
            if wayland_ready {
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(error))
                        if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.into()),
                }
                // Dispatch and drain available compositor state BEFORE output.
                continue;
            }
            drop(guard);
            if wake_ready {
                let mut bytes = [0; 256];
                loop {
                    match self.wake.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            if let Some(request) = pending.take() {
                let shutdown = matches!(request.command, Command::Shutdown);
                let result = match request.command {
                    Command::Text(text) => self.output.insert_text(request.expected, &text),
                    Command::Control { key, count } => {
                        self.output.control(request.expected, key, count)
                    }
                    Command::Shutdown => Ok(()),
                };
                let failed = self.state.shared.lock().unwrap().failed;
                let _ = request.reply.send(result);
                if shutdown || failed {
                    return Ok(());
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Recorder(Arc<Mutex<Vec<InputContext>>>);
    impl TextInputHandler for Recorder {
        fn on_context_changed(&mut self, context: InputContext) {
            self.0.lock().unwrap().push(context);
        }
    }
    fn detached_handle(capacity: usize) -> (InputMethodHandle, Receiver<Request>, UnixStream) {
        let (requests, receiver) = mpsc::sync_channel(capacity);
        let (wake, reader) = UnixStream::pair().unwrap();
        wake.set_nonblocking(true).unwrap();
        (
            InputMethodHandle {
                inner: Arc::new(Output {
                    requests,
                    wake,
                    state: Arc::new(Mutex::new(ContextState::default())),
                }),
            },
            receiver,
            reader,
        )
    }
    #[test]
    fn full_or_closed_output_queue_rejects_without_sending_another_command() {
        let (handle, receiver, _reader) = detached_handle(1);
        let (reply, _) = mpsc::sync_channel(1);
        handle
            .inner
            .requests
            .try_send(Request {
                expected: InputContext::default(),
                command: Command::Text("queued".into()),
                reply,
            })
            .unwrap_or_else(|_| panic!("empty queue"));
        assert_eq!(
            handle.insert_text(InputContext::default(), "kept"),
            Err(DeliveryError::Rejected("Input output queue is full".into()))
        );
        let queued = receiver.try_recv().unwrap();
        assert!(matches!(queued.command, Command::Text(text) if text == "queued"));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
        drop(receiver);
        assert_eq!(
            handle.insert_text(InputContext::default(), "kept"),
            Err(DeliveryError::Rejected("Input method is closed".into()))
        );
    }
    #[test]
    fn stopped_owner_releases_an_awaiting_caller_with_unknown_outcome() {
        let (handle, receiver, _reader) = detached_handle(1);
        let caller =
            std::thread::spawn(move || handle.insert_text(InputContext::default(), "kept"));
        let request = receiver.recv().unwrap();
        drop(request);
        assert!(caller.join().unwrap().unwrap_err().outcome_unknown());
    }
    #[test]
    fn contexts_commit_on_done_but_transitions_immediately_block_old_output() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::new(Mutex::new(ContextState::default()));
        let handler: Handler = Arc::new(Mutex::new(Some(Box::new(Recorder(events.clone())))));
        let mut state = MethodState::new(shared.clone(), handler);
        state.on_event(InputMethodEvent::Activate);
        assert!(events.lock().unwrap().is_empty());
        state.on_event(InputMethodEvent::Done);
        let old = shared.lock().unwrap().context;
        assert!(old.active);
        assert_eq!(shared.lock().unwrap().serial, 1);
        shared.lock().unwrap().validate(old).unwrap();
        state.on_event(InputMethodEvent::Done);
        assert_eq!(events.lock().unwrap().len(), 1);
        assert_eq!(shared.lock().unwrap().serial, 2);
        state.on_event(InputMethodEvent::Deactivate);
        assert!(shared.lock().unwrap().validate(old).is_err());
        assert_eq!(shared.lock().unwrap().context, old);
        state.on_event(InputMethodEvent::Activate);
        state.on_event(InputMethodEvent::Done);
        assert!(shared.lock().unwrap().validate(old).is_err());
        assert_eq!(shared.lock().unwrap().serial, 3);
        let current = shared.lock().unwrap().context;
        state.disconnect();
        assert!(shared.lock().unwrap().validate(current).is_err());
        assert!(!shared.lock().unwrap().context.active);
    }
    #[test]
    fn content_state_is_atomic_sensitive_and_reset_on_activation() {
        let shared = Arc::new(Mutex::new(ContextState::default()));
        let mut state = MethodState::new(shared.clone(), Arc::new(Mutex::new(None)));
        state.on_event(InputMethodEvent::Activate);
        state.on_event(InputMethodEvent::ContentType {
            hint: wayland_client::WEnum::Unknown(0x80),
            purpose: wayland_client::WEnum::Unknown(8),
        });
        assert!(!shared.lock().unwrap().context.sensitive());
        state.on_event(InputMethodEvent::Done);
        assert!(shared.lock().unwrap().context.sensitive());
        shared.lock().unwrap().serial = u32::MAX;
        state.on_event(InputMethodEvent::Activate);
        state.on_event(InputMethodEvent::Done);
        assert!(!shared.lock().unwrap().context.sensitive());
        assert_eq!(shared.lock().unwrap().serial, 0);
        shared.lock().unwrap().failed = true;
        let current = shared.lock().unwrap().context;
        assert!(
            shared
                .lock()
                .unwrap()
                .validate(current)
                .unwrap_err()
                .outcome_unknown()
        );
        state.on_event(InputMethodEvent::Done);
        assert!(shared.lock().unwrap().failed);
    }
    #[test]
    fn protocol_chunks_use_utf8_byte_boundaries_and_reject_invalid_text() {
        let text = "界".repeat(1365);
        let boundary = chunk_end(&text);
        assert_eq!(boundary, 3999);
        assert_eq!(text[..boundary].len() + text[boundary..].len(), 4095);
        assert!(validate_text(&text).is_ok());
        assert!(validate_text(&"a".repeat(4097)).is_err());
        assert!(validate_text("a\0b").is_err());
    }
    #[test]
    #[ignore = "requires a disposable Sway/Chromium session and stdin JSON commands"]
    fn wayland_browser_delivery() {
        use std::io::{BufRead, Write};
        let manager = InputDetectionManager::new().unwrap();
        let mut keyboard = manager.keyboard();
        println!("NOVAKEYS_READY");
        std::io::stdout().flush().unwrap();
        for line in std::io::stdin().lock().lines() {
            let request: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
            let mut context = manager.output.context();
            if let Some(generation) = request["generation"].as_u64() {
                context.generation = generation;
            }
            keyboard.set_context(context);
            let result = (|| -> Result<(), DeliveryError> {
                if let Some(key) = request["key"].as_str() {
                    keyboard.control(
                        key.chars()
                            .next()
                            .ok_or_else(|| DeliveryError::Rejected("Empty control".into()))?,
                    )?;
                }
                if let Some(text) = request["text"].as_str() {
                    keyboard.insert_text(text)?;
                }
                if let Some(count) = request["delete"].as_u64() {
                    keyboard.delete_text(count as u32, 0)?;
                }
                Ok(())
            })();
            println!(
                "NOVAKEYS_SENT {}",
                serde_json::json!({"error":result.err().map(|error|error.to_string()),"generation": manager.output.context().generation, "active": manager.output.context().active, "sensitive": manager.output.context().sensitive()})
            );
            std::io::stdout().flush().unwrap();
        }
        keyboard.destroy();
    }
}
