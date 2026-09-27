//! Files a test makes misbehave.

use std::path::Path;

/// Keeps `path` from opening until dropped, without touching its size or
/// modification time: no permission to read it on Unix, an exclusive handle
/// on Windows.
pub(crate) struct UnopenableFile {
    #[cfg(unix)]
    path: std::path::PathBuf,
    #[cfg(windows)]
    _exclusive: std::fs::File,
}

impl UnopenableFile {
    pub(crate) fn block(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
            Self {
                path: path.to_path_buf(),
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let exclusive = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap();
            Self {
                _exclusive: exclusive,
            }
        }
    }
}

#[cfg(unix)]
impl Drop for UnopenableFile {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
