//! Local CJK engines owned by one lazy worker, with bounded traffic to GTK.
mod chinese;
mod deployment;
mod ffi;
mod japanese;
mod korean;
mod profiles;
mod romaji;
use anyhow::{Result, bail};
pub use deployment::run_internal;
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
pub const PAGE_SIZE: usize = 6;
pub const MAX_INPUT: usize = 1024;
#[derive(Debug)]
struct InputLimit;
impl std::fmt::Display for InputLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Commit the current text before typing more")
    }
}
impl std::error::Error for InputLimit {}
fn check_input_limit(bytes: usize) -> Result<()> {
    if bytes > MAX_INPUT {
        return Err(InputLimit.into());
    }
    Ok(())
}
#[derive(Debug)]
pub struct EngineError {
    pub message: String,
    /// When true, rejection did not mutate composition or consume engine output.
    pub recoverable: bool,
}
impl From<anyhow::Error> for EngineError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            recoverable: error.is::<InputLimit>(),
            message: error.to_string(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Native,
    Traditional,
    Katakana,
    Latin,
}
#[derive(Debug, Clone)]
pub enum Command {
    Reset,
    Text(String),
    Backspace,
    Enter,
    Space,
    Escape,
    Left,
    Right,
    Page(i32),
    Select(usize),
    Segment(i32),
    Resize(i32),
    Hanja,
    Finish,
}
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub preedit: String,
    pub cursor: usize,
    pub pending_romaji: bool,
    pub selection: Option<(usize, usize)>,
    pub candidates: Vec<String>,
    pub selected: Option<usize>,
    pub page: usize,
    pub has_previous: bool,
    pub has_next: bool,
    pub segments: usize,
    pub active_segment: usize,
    pub committed: String,
    pub control: Option<char>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Token {
    pub generation: u64,
    pub epoch: u64,
}
#[derive(Debug)]
pub struct Job {
    pub token: Token,
    pub revision: u64,
    pub language: String,
    pub mode: Mode,
    pub command: Command,
}
#[derive(Debug)]
pub struct Response {
    pub token: Token,
    pub revision: u64,
    pub result: Result<Snapshot, EngineError>,
}
pub struct Worker {
    sender: Option<SyncSender<Job>>,
    receiver: Option<Receiver<Response>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    cancellation: Option<std::os::unix::net::UnixStream>,
    thread: Option<std::thread::JoinHandle<()>>,
    current: Arc<Mutex<Token>>,
}
impl Worker {
    pub fn new(notify: impl Fn() + Send + 'static) -> Result<Self> {
        let (cancel, reader) = std::os::unix::net::UnixStream::pair()?;
        let reader = Arc::new(reader);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let (sender, incoming) = mpsc::sync_channel::<Job>(32);
        let (outgoing, receiver) = mpsc::sync_channel(8);
        let current = Arc::new(Mutex::new(Token::default()));
        let active = current.clone();
        let thread = std::thread::Builder::new()
            .name("novakeys-input".into())
            .spawn(move || {
                let mut engines: Option<Engines> = None;
                let mut session: Option<(Token, String, Mode)> = None;
                while let Ok(job) = incoming.recv() {
                    if stopping.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    if job.token != *active.lock().unwrap() || job.language.is_empty() {
                        if let Some(engines) = &mut engines {
                            engines.clear();
                        }
                        session = None;
                        continue;
                    }
                    let result = (|| -> Result<Snapshot> {
                        if engines.is_none() {
                            engines =
                                Some(Engines::new(profiles::Paths::new(Some(reader.clone()))?));
                        }
                        let engines = engines.as_mut().unwrap();
                        let wanted = (job.token, job.language.clone(), job.mode);
                        if session.as_ref() != Some(&wanted) {
                            engines.reset(&job.language, job.mode)?;
                            session = Some(wanted);
                        }
                        engines.command(&job.language, job.mode, &job.command)
                    })()
                    .map_err(EngineError::from);
                    if stopping.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    if job.token != *active.lock().unwrap() {
                        if let Some(engines) = &mut engines {
                            engines.clear();
                        }
                        session = None;
                        continue;
                    }
                    if outgoing
                        .send(Response {
                            token: job.token,
                            revision: job.revision,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    notify();
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver: Some(receiver),
            current,
            stop,
            cancellation: Some(cancel),
            thread: Some(thread),
        })
    }
    pub fn reset_context(&self, token: Token) {
        *self.current.lock().unwrap() = token;
        let _ = self.sender.as_ref().unwrap().try_send(Job {
            token,
            revision: 0,
            language: String::new(),
            mode: Mode::Native,
            command: Command::Reset,
        });
    }
    pub fn send(&self, job: Job) -> Result<(), EngineError> {
        self.sender
            .as_ref()
            .unwrap()
            .try_send(job)
            .map_err(|error| match error {
                TrySendError::Full(_) => EngineError {
                    message: "Input is busy; wait for the current keys to finish".into(),
                    recoverable: true,
                },
                TrySendError::Disconnected(_) => EngineError {
                    message: "Input worker has stopped".into(),
                    recoverable: false,
                },
            })
    }
    #[cfg(test)]
    pub fn fixture() -> (Self, SyncSender<Response>, Receiver<Job>) {
        let (sender, jobs) = mpsc::sync_channel(32);
        let (responses, receiver) = mpsc::sync_channel(8);
        (
            Self {
                sender: Some(sender),
                receiver: Some(receiver),
                stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                cancellation: None,
                thread: None,
                current: Arc::new(Mutex::new(Token::default())),
            },
            responses,
            jobs,
        )
    }
    pub fn receive(&self) -> Option<Response> {
        self.receiver.as_ref()?.try_recv().ok()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        use std::io::Write;
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(cancel) = &mut self.cancellation {
            let _ = cancel.write_all(&[1]);
        }
        self.receiver.take();
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Engines {
    chinese: Option<chinese::Chinese>,
    japanese: Option<japanese::Japanese>,
    korean: Option<korean::Korean>,
    paths: profiles::Paths,
}
impl Engines {
    fn new(paths: profiles::Paths) -> Self {
        Self {
            chinese: None,
            japanese: None,
            korean: None,
            paths,
        }
    }
    fn clear(&mut self) {
        if let Some(engine) = &mut self.chinese {
            engine.clear_session();
        }
        if let Some(engine) = &mut self.japanese {
            engine.reset();
        }
        if let Some(engine) = &mut self.korean {
            engine.reset();
        }
    }
    fn reset(&mut self, language: &str, mode: Mode) -> Result<()> {
        self.clear();
        match language {
            "zh" => {
                if self.chinese.is_none() {
                    self.chinese = Some(chinese::Chinese::new(&self.paths)?);
                }
                self.chinese.as_mut().unwrap().reset(mode)?;
            }
            "ja" => {
                if self.japanese.is_none() {
                    self.japanese = Some(japanese::Japanese::new(&self.paths)?);
                }
            }
            "ko" => {
                if self.korean.is_none() {
                    self.korean = Some(korean::Korean::new()?);
                }
            }
            _ => bail!("No composition engine for this layout"),
        }
        Ok(())
    }
    fn command(&mut self, language: &str, mode: Mode, command: &Command) -> Result<Snapshot> {
        match language {
            "zh" => self.chinese.as_mut().unwrap().command(command),
            "ja" => self.japanese.as_mut().unwrap().command(command, mode),
            "ko" => self.korean.as_mut().unwrap().command(command),
            _ => bail!("No composition engine for this layout"),
        }
    }
}
fn cstring(text: &str) -> Result<std::ffi::CString> {
    Ok(std::ffi::CString::new(text)?)
}
// A non-null pointer must reference a live NUL-terminated buffer throughout this call.
unsafe fn copy_text(pointer: *const std::ffi::c_char) -> Result<String> {
    if pointer.is_null() {
        return Ok(String::new());
    }
    let text = unsafe { std::ffi::CStr::from_ptr(pointer) }.to_str()?;
    anyhow::ensure!(text.len() <= 4096, "Engine text exceeds 4096 bytes");
    Ok(text.to_owned())
}
fn append_commit(target: &mut String, text: &str) -> Result<()> {
    anyhow::ensure!(
        target.len() + text.len() <= 4096,
        "Commit exceeds 4096 bytes"
    );
    target.push_str(text);
    Ok(())
}
#[cfg(test)]
mod tests;

/// Dubeolsik physical key mapping, including Shift and compound-vowel popup keys.
pub fn korean_keys(text: &str, shift: bool) -> String {
    text.chars()
        .map(|ch| {
            match ch {
                'ㅂ' => {
                    if shift {
                        "Q"
                    } else {
                        "q"
                    }
                }
                'ㅈ' => {
                    if shift {
                        "W"
                    } else {
                        "w"
                    }
                }
                'ㄷ' => {
                    if shift {
                        "E"
                    } else {
                        "e"
                    }
                }
                'ㄱ' => {
                    if shift {
                        "R"
                    } else {
                        "r"
                    }
                }
                'ㅅ' => {
                    if shift {
                        "T"
                    } else {
                        "t"
                    }
                }
                'ㅛ' => "y",
                'ㅕ' => "u",
                'ㅑ' => "i",
                'ㅐ' => {
                    if shift {
                        "O"
                    } else {
                        "o"
                    }
                }
                'ㅔ' => {
                    if shift {
                        "P"
                    } else {
                        "p"
                    }
                }
                'ㅁ' => "a",
                'ㄴ' => "s",
                'ㅇ' => "d",
                'ㄹ' => "f",
                'ㅎ' => "g",
                'ㅗ' => "h",
                'ㅓ' => "j",
                'ㅏ' => "k",
                'ㅣ' => "l",
                'ㅋ' => "z",
                'ㅌ' => "x",
                'ㅊ' => "c",
                'ㅍ' => "v",
                'ㅠ' => "b",
                'ㅜ' => "n",
                'ㅡ' => "m",
                'ㅃ' => "Q",
                'ㅉ' => "W",
                'ㄸ' => "E",
                'ㄲ' => "R",
                'ㅆ' => "T",
                'ㅒ' => "O",
                'ㅖ' => "P",
                'ㅘ' => "hk",
                'ㅙ' => "ho",
                'ㅚ' => "hl",
                'ㅝ' => "nj",
                'ㅞ' => "np",
                'ㅟ' => "nl",
                'ㅢ' => "ml",
                _ => return ch.to_string(),
            }
            .to_owned()
        })
        .collect()
}
