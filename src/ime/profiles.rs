use anyhow::{Result, ensure};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
pub(super) struct Paths {
    pub session: tempfile::TempDir,
    pub cache: PathBuf,
    pub cancellation: Option<std::sync::Arc<std::os::unix::net::UnixStream>>,
}
impl Paths {
    pub fn new(
        cancellation: Option<std::sync::Arc<std::os::unix::net::UnixStream>>,
    ) -> Result<Self> {
        let runtime = crate::ipc::runtime_dir()?;
        let session = tempfile::Builder::new()
            .prefix("ime-")
            .tempdir_in(runtime)?;
        let base = crate::config::file_io::get_xdg_dirs()
            .get_cache_home()
            .ok_or_else(|| anyhow::anyhow!("No cache directory is available"))?;
        private_dir(&base)?;
        let cache = base.join("rime-v1");
        private_dir(&cache)?;
        Ok(Self {
            session,
            cache,
            cancellation,
        })
    }
    #[cfg(test)]
    pub fn temporary() -> Self {
        let session = tempfile::tempdir().unwrap();
        let cache = session.path().join("cache");
        private_dir(&cache).unwrap();
        Self {
            session,
            cache,
            cancellation: None,
        }
    }
    pub fn directory(&self, name: &str) -> Result<PathBuf> {
        let path = self.session.path().join(name);
        private_dir(&path)?;
        Ok(path)
    }
}
pub(super) fn private_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == fs::metadata("/proc/self")?.uid(),
        "Unsafe engine data directory"
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
