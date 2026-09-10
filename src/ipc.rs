//! Bounded, acknowledged local commands in a private per-user runtime directory.
use crate::service::{IPCMessage, IPCResponse};
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};
const LIMIT: u64 = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(3);

pub fn runtime_dir() -> Result<PathBuf> {
    let uid = fs::metadata("/proc/self")?.uid();
    let base = if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(path);
        validate_dir(&path, uid)?;
        path
    } else {
        PathBuf::from("/tmp")
    };
    let path = base.join(format!("novakeys-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    validate_dir(&path, uid)?;
    Ok(path)
}
fn validate_dir(path: &Path, uid: u32) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute() && meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0,
        "Runtime directory must be an absolute, owned, private directory: {}",
        path.display()
    );
    Ok(())
}
pub struct Ipc {
    socket: Option<UnixListener>,
    path: PathBuf,
    _lock: Option<File>,
}
impl Ipc {
    pub fn init() -> Result<Self> {
        Self::at(&runtime_dir()?)
    }
    fn at(dir: &Path) -> Result<Self> {
        let path = dir.join("control.sock");
        let lock_path = dir.join("instance.lock");
        if let Ok(meta) = fs::symlink_metadata(&lock_path) {
            ensure!(meta.is_file(), "Invalid instance lock");
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Ok(Self {
                    socket: None,
                    path,
                    _lock: None,
                });
            }
            Err(e) => return Err(e.into()),
        }
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(
                meta.file_type().is_socket(),
                "Refusing to remove non-socket runtime entry"
            );
            fs::remove_file(&path)?;
        }
        let socket = UnixListener::bind(&path).context("Bind control socket")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            socket: Some(socket),
            path,
            _lock: Some(lock),
        })
    }
    pub fn client_only() -> Result<Self> {
        Ok(Self {
            socket: None,
            path: runtime_dir()?.join("control.sock"),
            _lock: None,
        })
    }
    pub fn is_single_instance(&self) -> bool {
        self.socket.is_some()
    }
    pub fn send(&self, message: &IPCMessage) -> Result<IPCResponse> {
        message.validate()?;
        let mut stream = match connect(&self.path) {
            Ok(stream) => stream,
            Err(error)
                if matches!(message.kind, crate::service::CommandKind::Close)
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
            {
                return Ok(IPCResponse::default());
            }
            Err(error) => return Err(error).context("No running NOVA Keys instance"),
        };
        configure(&stream)?;
        stream.write_all(&serde_json::to_vec(message)?)?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let response: IPCResponse = serde_json::from_slice(&read_frame(&mut stream)?)?;
        if let Some(error) = &response.error {
            bail!("{error}");
        }
        Ok(response)
    }
    pub fn accept(&self) -> Result<Option<(IPCMessage, UnixStream)>> {
        let (mut stream, _) = loop {
            match self.socket.as_ref().context("Not a server")?.accept() {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => break result?,
            }
        };
        configure(&stream)?;
        match read_frame(&mut stream).and_then(|data| {
            let message: IPCMessage = serde_json::from_slice(&data)?;
            message.validate()?;
            Ok(message)
        }) {
            Ok(message) => Ok(Some((message, stream))),
            Err(e) => {
                let _ = respond(&mut stream, &IPCResponse::error(e.to_string()));
                Ok(None)
            }
        }
    }
}
fn configure(stream: &UnixStream) -> Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    Ok(())
}
fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    read_frame_with_timeout(stream, TIMEOUT)
}
fn read_frame_with_timeout(stream: &mut UnixStream, timeout: Duration) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .context("IPC frame deadline exceeded")?;
        stream.set_read_timeout(Some(remaining))?;
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
        ensure!(data.len() as u64 <= LIMIT, "IPC frame exceeds 16 KiB");
    }
    ensure!(data.len() as u64 <= LIMIT, "IPC frame exceeds 16 KiB");
    ensure!(!data.is_empty(), "Empty IPC frame");
    Ok(data)
}
// A full local listen backlog must fail promptly, before framing deadlines apply.
fn connect(path: &std::path::Path) -> std::io::Result<UnixStream> {
    use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType, socket_with};
    let socket = socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::connect(&socket, &SocketAddrUnix::new(path)?)?;
    let stream = UnixStream::from(socket);
    stream.set_nonblocking(false)?;
    Ok(stream)
}
pub fn respond(stream: &mut UnixStream, response: &IPCResponse) -> Result<()> {
    stream.write_all(&serde_json::to_vec(response)?)?;
    Ok(())
}
impl Drop for Ipc {
    fn drop(&mut self) {
        if self.socket.is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_listen_backlog_fails_without_waiting_for_accept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        rustix::net::listen(&listener, 1).unwrap();
        let first = connect(&path).unwrap();
        let second = connect(&path).unwrap();
        let start = std::time::Instant::now();
        let error = connect(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        assert!(start.elapsed() < Duration::from_millis(200));
        drop((first, second));
    }
    #[test]
    fn instance_lock_and_stale_socket_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let first = Ipc::at(dir.path()).unwrap();
        assert!(first.is_single_instance());
        assert!(!Ipc::at(dir.path()).unwrap().is_single_instance());
        drop(first);
        drop(UnixListener::bind(dir.path().join("control.sock")).unwrap());
        assert!(Ipc::at(dir.path()).unwrap().is_single_instance());
    }
    #[test]
    fn refuses_non_socket_and_shared_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("control.sock"), "keep").unwrap();
        assert!(Ipc::at(dir.path()).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("control.sock")).unwrap(),
            "keep"
        );
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(validate_dir(dir.path(), fs::metadata("/proc/self").unwrap().uid()).is_err());
    }
    #[test]
    fn frame_size_and_read_timeout_are_enforced() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        b.write_all(&vec![b'x'; LIMIT as usize + 1]).unwrap();
        b.shutdown(std::net::Shutdown::Write).unwrap();
        assert!(read_frame(&mut a).is_err());
        let (mut a, _b) = UnixStream::pair().unwrap();
        a.set_read_timeout(Some(Duration::from_millis(10))).unwrap();
        assert!(read_frame_with_timeout(&mut a, Duration::from_millis(10)).is_err());
    }
    #[test]
    fn response_is_fresh_and_remote_error_propagates() {
        use crate::service::CommandKind;
        let dir = tempfile::tempdir().unwrap();
        let server = Ipc::at(dir.path()).unwrap();
        let client = Ipc {
            socket: None,
            path: dir.path().join("control.sock"),
            _lock: None,
        };
        let worker = std::thread::spawn(move || {
            let (_, mut stream) = server.accept().unwrap().unwrap();
            respond(
                &mut stream,
                &IPCResponse {
                    error: None,
                    status: Some(serde_json::json!({"current_language": "no"})),
                },
            )
            .unwrap();
            drop(stream);
            let (_, mut stream) = server.accept().unwrap().unwrap();
            respond(
                &mut stream,
                &IPCResponse::error("invalid configuration".into()),
            )
            .unwrap();
        });
        let message = IPCMessage {
            version: 1,
            kind: CommandKind::GetStatus,
            value: None,
        };
        assert_eq!(
            client.send(&message).unwrap().status.unwrap()["current_language"],
            "no"
        );
        assert!(
            client
                .send(&message)
                .unwrap_err()
                .to_string()
                .contains("invalid configuration")
        );
        worker.join().unwrap();
        assert!(client.send(&message).is_err());
        assert!(
            client
                .send(&IPCMessage {
                    version: 1,
                    kind: CommandKind::Close,
                    value: None
                })
                .is_ok()
        );
    }
    #[test]
    fn trickling_peer_cannot_extend_absolute_deadline() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            for _ in 0..50 {
                if writer.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let start = std::time::Instant::now();
        assert!(read_frame_with_timeout(&mut reader, Duration::from_millis(30)).is_err());
        assert!(start.elapsed() < Duration::from_millis(200));
        drop(reader);
        thread.join().unwrap();
    }
}
