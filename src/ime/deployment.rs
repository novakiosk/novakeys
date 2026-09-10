//! Compile dictionaries in a short-lived process; runtime never keeps compiler allocations.
use anyhow::{Result, ensure};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{
    ffi::CString,
    fs,
    path::Path,
    process::{Command, Stdio},
};
struct Helper(std::process::Child);
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
const ARG: &str = "--internal-prepare-chinese";
const READY: &str = "novakeys-cache-version";
fn fingerprint() -> Result<String> {
    let mut value = format!("private-luna-v1:{}\n", unsafe {
        super::copy_text(super::ffi::nk_rime_version())
    }?);
    for entry in fs::read_dir("/usr/share/rime-data")? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            value.push_str(&format!(
                "{}:{}:{}\n",
                entry.file_name().to_string_lossy(),
                meta.len(),
                meta.modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
        }
    }
    let mut lines = value.lines().collect::<Vec<_>>();
    lines.sort_unstable();
    Ok(lines.join("\n"))
}
pub fn run_internal() -> Option<Result<()>> {
    let mut args = std::env::args_os();
    args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new(ARG)) {
        return None;
    }
    Some((|| {
        let user = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing private profile"))?;
        let cache = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing private cache"))?;
        ensure!(args.next().is_none(), "Unexpected preparation arguments");
        compile(Path::new(&user), Path::new(&cache))
    })())
}
fn compile(user: &Path, cache: &Path) -> Result<()> {
    super::profiles::private_dir(user)?;
    super::profiles::private_dir(cache)?;
    let user = CString::new(user.as_os_str().as_encoded_bytes())?;
    let cache = CString::new(cache.as_os_str().as_encoded_bytes())?;
    ensure!(
        unsafe { super::ffi::nk_rime_init(user.as_ptr(), cache.as_ptr(), 1) } != 0,
        "Dictionary privacy validation failed"
    );
    unsafe {
        super::ffi::nk_rime_finalize();
    }
    Ok(())
}
pub(super) fn prepare(
    user: &Path,
    cache: &Path,
    cancellation: Option<&std::os::unix::net::UnixStream>,
) -> Result<std::path::PathBuf> {
    let wanted = fingerprint()?;
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    wanted.hash(&mut hasher);
    let selected = cache.join(format!("compiled-{:016x}", hasher.finish()));
    if fs::read_to_string(selected.join(READY)).ok().as_deref() == Some(&wanted) {
        return Ok(selected);
    }
    let parent = cache;
    let staging = tempfile::Builder::new()
        .prefix("rime-build-")
        .tempdir_in(parent)?;
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    #[cfg(test)]
    command
        .args([
            "--ignored",
            "--exact",
            "ime::tests::dictionary_deployment_helper",
            "--nocapture",
        ])
        .env("NOVAKEYS_TEST_DEPLOY_USER", user)
        .env("NOVAKEYS_TEST_DEPLOY_CACHE", staging.path());
    #[cfg(not(test))]
    command.arg(ARG).arg(user).arg(staging.path());
    let mut child = Helper(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut output = child.0.stdout.take().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    // Drain and discard stdout (including test harness output) while watching
    // for completion, cancellation and the deadline.
    use std::io::Read;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            let _ = child.0.kill();
            let _ = child.0.wait();
            anyhow::bail!("Chinese dictionary preparation timed out");
        }
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: remaining.subsec_nanos() as i64,
        };
        let mut fds = vec![PollFd::new(&output, PollFlags::IN)];
        if let Some(cancellation) = cancellation {
            fds.push(PollFd::new(cancellation, PollFlags::IN));
        }
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        ensure!(
            fds.get(1).is_none_or(|fd| fd.revents().is_empty()),
            "Chinese dictionary preparation cancelled"
        );
        let mut bytes = [0; 1024];
        match output.read(&mut bytes) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        child.0.wait()?.success(),
        "Chinese dictionaries could not be compiled"
    );
    fs::write(staging.path().join(READY), wanted)?;
    // Publish the completed cache before pruning other compiled directories.
    if selected.exists() {
        fs::remove_dir_all(&selected)?;
    }
    fs::rename(staging.path(), &selected)?;
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("compiled-")
            && entry.path() != selected
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(selected)
}
#[cfg(test)]
pub(super) fn test_compile() -> Result<()> {
    compile(
        Path::new(&std::env::var_os("NOVAKEYS_TEST_DEPLOY_USER").unwrap()),
        Path::new(&std::env::var_os("NOVAKEYS_TEST_DEPLOY_CACHE").unwrap()),
    )
}
