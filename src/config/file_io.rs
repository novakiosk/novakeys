//! Bounded UTF-8 reads for discovered configuration files.
use anyhow::{Context, Result, ensure};
use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path, sync::OnceLock};
use xdg::BaseDirectories;
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
pub fn get_xdg_dirs() -> &'static BaseDirectories {
    static DIRS: OnceLock<BaseDirectories> = OnceLock::new();
    DIRS.get_or_init(|| BaseDirectories::with_prefix("novakeys"))
}
pub fn read_text(path: &Path) -> Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
        .with_context(|| format!("Read {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "Configuration entry is not a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG_BYTES,
        "Configuration file exceeds 1 MiB"
    );
    String::from_utf8(bytes).context("Configuration is not UTF-8")
}
pub fn read_optional(name: &str) -> Result<Option<String>> {
    get_xdg_dirs()
        .find_config_file(name)
        .map(|path| read_text(&path))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regular_symlinks_work_and_fifos_cannot_block_reads() {
        let directory = tempfile::tempdir().unwrap();
        let regular = directory.path().join("regular");
        let link = directory.path().join("link");
        std::fs::write(&regular, "configuration").unwrap();
        std::os::unix::fs::symlink(&regular, &link).unwrap();
        assert_eq!(read_text(&link).unwrap(), "configuration");
        let fifo = directory.path().join("fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "config::file_io::tests::nonblocking_read_helper",
            ])
            .env("NOVAKEYS_FIFO_TEST", &fifo)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Configuration read blocked opening a FIFO");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "private subprocess entry point"]
    fn nonblocking_read_helper() {
        let Some(path) = std::env::var_os("NOVAKEYS_FIFO_TEST") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        assert!(
            read_text(&path)
                .unwrap_err()
                .to_string()
                .contains("not a regular file")
        );
        let link = path.with_extension("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(
            read_text(&link)
                .unwrap_err()
                .to_string()
                .contains("not a regular file")
        );
    }
}
