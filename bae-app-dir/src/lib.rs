//! bae's own directory and what it holds before any library's store opens:
//! the libraries registered on this device, the one last opened, the
//! installation id, coven's installation of bae's stores under bae's keyring
//! namespace, and the transfer limits a library runs under before its
//! preferences can be read.

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};

/// bae-owned preferences and caches, separate from coven's `stores` directory.
const LIBRARIES_DIRNAME: &str = "libraries";

/// bae's own directory: `.bae` under the home directory the host passes in.
/// Every registered library and the active-library pointer live under it. Built once at process start and passed to whatever reads or
/// writes under it.
///
/// Building one creates the directory, so everything under it — coven's
/// stores, which coven creates only inside an existing app directory, and
/// bae's own files — can rely on it existing.
#[derive(Debug, Clone)]
pub struct AppDir {
    path: PathBuf,
    /// The temporary folder a test's or a preview's directory is, removed
    /// once the last clone of the directory is gone.
    #[cfg(any(test, feature = "test-utils"))]
    _temporary: Option<std::sync::Arc<tempfile::TempDir>>,
}

impl AppDir {
    /// The app directory under `home`, `home/.bae`, created if missing.
    pub fn under_home(home: &Path) -> std::io::Result<Self> {
        Self::create(home.join(".bae"))
    }

    /// An app directory at exactly `path`, created if missing, for tests that
    /// treat a temp dir as the app directory itself.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn at(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        Self::create(path.into())
    }

    /// An app directory that is a new folder in the system's temporary
    /// directory, removed with everything in it once the last clone of this
    /// directory is gone: the home of a test's or a preview's host.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn temporary() -> std::io::Result<Self> {
        let folder = tempfile::Builder::new().prefix("bae-").tempdir()?;
        Ok(Self {
            path: folder.path().to_path_buf(),
            _temporary: Some(std::sync::Arc::new(folder)),
        })
    }

    fn create(path: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&path).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("create app directory {}: {error}", path.display()),
            )
        })?;
        Ok(Self {
            path,
            #[cfg(any(test, feature = "test-utils"))]
            _temporary: None,
        })
    }

    /// Delete everything under the app directory, leaving it empty: a dev
    /// reset's fresh start.
    #[cfg(feature = "dev-reset")]
    pub fn clear(&self) -> std::io::Result<()> {
        std::fs::remove_dir_all(&self.path)?;
        std::fs::create_dir(&self.path)
    }

    /// The directory every registered library's own directory is in.
    pub fn libraries(&self) -> PathBuf {
        self.path.join(LIBRARIES_DIRNAME)
    }

    /// The directory the library `library_id` is registered at.
    pub fn registered_library(&self, library_id: &str) -> PathBuf {
        self.libraries().join(library_id)
    }

    /// Where a test that seeds release files writes the originals it stages
    /// them from.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn test_originals(&self) -> PathBuf {
        self.path.join("test-originals")
    }

    /// The file naming the library this device last opened.
    pub fn active_library_pointer(&self) -> PathBuf {
        self.path.join("active-library")
    }

    /// Name `store` as the library this device last opened.
    pub fn save_active_library(&self, store: coven::StoreId) -> Result<(), AppDirError> {
        coven::write_atomic(&self.active_library_pointer(), store.to_string().as_bytes())?;
        Ok(())
    }

    /// The directory of removal records: one file per library whose removal
    /// from this device has started and not finished, named by its id.
    pub fn removal_records(&self) -> PathBuf {
        self.path.join("removals")
    }

    /// The file a removal, or a lookup of unfinished removals, holds locked
    /// for its whole run, in every process using this directory.
    pub fn removal_lock(&self) -> PathBuf {
        self.path.join("removals.lock")
    }

    /// This device's libraries as coven keeps them: each library's store in
    /// its own directory under the app directory, their keys and secrets in
    /// the OS keychain under bae's service. Built once at process start, so
    /// a keychain coven can't reach fails the start, and held while the
    /// process runs.
    pub fn coven(&self) -> Result<coven::Coven, coven::KeyError> {
        coven::Coven::new(self.path.clone(), KEYRING_SERVICE)
    }

    /// The app directory coven keeps this device's stores under, which a
    /// test builds its own installation over (`coven::Coven::in_memory`, and
    /// the person's other devices from it).
    #[cfg(any(test, feature = "test-utils"))]
    pub fn coven_app_dir(&self) -> PathBuf {
        self.path.clone()
    }

    /// A bae-owned installation id shared by this installation's libraries.
    /// Publishing without replacement makes concurrent first launches agree.
    pub fn install_id(&self, ids: &dyn coven::IdSource) -> Result<String, AppDirError> {
        use std::io::Write;
        let path = self.path.join("install-id");
        let read = || -> Result<String, AppDirError> {
            let text = std::fs::read_to_string(&path)?;
            let id = uuid::Uuid::parse_str(text.trim()).map_err(|error| {
                AppDirError::Invalid(format!("invalid installation id: {error}"))
            })?;
            Ok(id.to_string())
        };
        match read() {
            Ok(id) => return Ok(id),
            Err(AppDirError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let id = ids.new_id().to_string();
        let mut staged = tempfile::NamedTempFile::new_in(&self.path)?;
        staged.write_all(id.as_bytes())?;
        staged.as_file().sync_all()?;
        match staged.persist_noclobber(&path) {
            Ok(_) => {
                bae_util::fs::sync_directory(&self.path)?;
                Ok(id)
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => read(),
            Err(error) => Err(error.error.into()),
        }
    }

    /// The libraries registered on this device, the active one first, then by
    /// name, each with whether it opens: the stores `coven`, this
    /// directory's installation, lists.
    pub async fn discover_libraries(
        &self,
        coven: &coven::Coven,
    ) -> Result<Vec<LibraryInfo>, AppDirError> {
        let active_id = self.active_library_id()?;
        let mut libraries = Vec::new();
        for store in coven.stores().await? {
            let state = match store.unfinished_replacement {
                // Whatever its settings say, only finishing the call opens it.
                Some(by) => LibraryState::ReplacementUnfinished(by),
                None => match coven::Coven::storage_config(&coven.store_dir(&store.id)).await {
                    Ok(storage) => LibraryState::Openable(storage),
                    Err(error) => {
                        tracing::warn!(store = %store.id, %error, "a library's settings can't be read");
                        LibraryState::Unreadable(bae_diagnostics::UiError::diagnostic(
                            bae_diagnostics::cloud_home_category(&error),
                            format_args!("Cloud home error: {error}"),
                        ))
                    }
                },
            };
            let id = store.id.to_string();
            libraries.push(LibraryInfo {
                is_active: active_id.as_deref() == Some(id.as_str()),
                path: self.registered_library(&id),
                id,
                name: store.name,
                state,
            });
        }
        libraries.sort_by(|a, b| b.is_active.cmp(&a.is_active).then_with(|| a.name.cmp(&b.name)));
        Ok(libraries)
    }

    /// The library this device last opened, as its pointer names it.
    pub fn active_library_id(&self) -> Result<Option<String>, AppDirError> {
        let pointer_path = self.active_library_pointer();
        let Some(content) = read_optional_file(&pointer_path)? else {
            return Ok(None);
        };
        let id = content.trim();
        let parsed = uuid::Uuid::parse_str(id).map_err(|error| {
            AppDirError::Invalid(format!("{}: {error}", pointer_path.display()))
        })?;
        Ok(Some(parsed.to_string()))
    }
}

/// A store discovered through coven, with its current committed name, or
/// while a replacement of this device's copy is unfinished, the name the new
/// copy loaded.
#[derive(Clone, Debug)]
pub struct LibraryInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub is_active: bool,
    pub state: LibraryState,
}

/// Whether a library on this device opens, and where it is stored.
#[derive(Clone, Debug)]
pub enum LibraryState {
    /// It opens: stored at its cloud home, or on this device only (`None`).
    Openable(Option<coven::StorageConfig>),
    /// Its storage settings can't be read, which keeps it from opening.
    Unreadable(bae_diagnostics::UiError),
    /// A reset or a restore of this device's copy put the new copy in place
    /// of the old one but stopped before it finished. The library opens once
    /// the call named here runs again and finishes it; coven refuses the
    /// other call.
    ReplacementUnfinished(coven::ReplacedBy),
}

/// The library a registered id names: a UUID, never a path.
pub fn parse_library_id(library_id: &str) -> Result<coven::StoreId, AppDirError> {
    Ok(coven::StoreId(uuid::Uuid::parse_str(library_id).map_err(|error| {
        AppDirError::Invalid(format!("invalid library id {library_id}: {error}"))
    })?))
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<String>, AppDirError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        // Keep the path in the error: "Access is denied" without the file it
        // was denied on is undiagnosable from a user report.
        Err(e) => {
            Err(AppDirError::Io(std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))))
        }
    }
}

/// File transfers bae runs at once, per direction, on a fresh library, the
/// `preferences` row's default, and what adding this device, joining or
/// restoring reads the library from storage under, before the library's row
/// exists. Serial (1) is safe but slow; a small burst keeps a single stalled
/// transfer from holding up the rest.
pub const DEFAULT_CONCURRENT_TRANSFERS: u32 = 3;

pub fn default_transfer_concurrency() -> NonZeroU32 {
    NonZeroU32::new(DEFAULT_CONCURRENT_TRANSFERS).expect("DEFAULT_CONCURRENT_TRANSFERS is non-zero")
}

/// The file transfers a library runs at once before its own preferences can
/// be read: while it is loaded from storage by adding this device, a join or
/// a restore.
pub fn default_transfer_limits() -> coven::TransferLimits {
    coven::TransferLimits {
        uploads: usize_bound(default_transfer_concurrency()),
        downloads: usize_bound(default_transfer_concurrency()),
    }
}

/// Widen a stored concurrency setting to the [`NonZeroUsize`] coven's builder
/// takes. Non-zero is preserved: `usize` is at least 32 bits on every platform
/// bae targets, so a `NonZeroU32` never widens to zero.
pub fn usize_bound(n: NonZeroU32) -> NonZeroUsize {
    NonZeroUsize::new(n.get() as usize).expect("a NonZeroU32 widened to usize stays non-zero")
}

/// bae's keyring namespace: the OS keychain service every library's keys and
/// secrets are kept under.
const KEYRING_SERVICE: &str = "bae";

/// Why bae's directory, or what it holds, could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum AppDirError {
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    File(#[from] coven::FileError),
    #[error(transparent)]
    Stores(#[from] coven::StoreListError),
    #[error(transparent)]
    Storage(#[from] coven::StorageError),
}

impl AppDirError {
    /// The class the person is shown: a file or a store listing that can't
    /// be read or written is a configuration to fix.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        bae_diagnostics::UiErrorCategory::Config
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
