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

    fn create(path: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&path).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("create app directory {}: {error}", path.display()),
            )
        })?;
        Ok(Self { path })
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

    /// The directory of removal records: one file per library whose removal
    /// from this device has started and not finished, named by its id.
    pub(crate) fn removal_records(&self) -> PathBuf {
        self.path.join("removals")
    }

    /// The file a removal, or a lookup of unfinished removals, holds locked
    /// for its whole run, in every process using this directory.
    pub(crate) fn removal_lock(&self) -> PathBuf {
        self.path.join("removals.lock")
    }

    /// coven owns store directories under the app directory.
    pub fn store_layout(&self) -> coven::StoreLayout {
        coven::StoreLayout::new(self.path.clone())
    }

    /// A bae-owned installation id shared by this installation's libraries.
    /// Publishing without replacement makes concurrent first launches agree.
    pub(crate) fn install_id(
        &self,
        ids: &dyn coven::IdSource,
    ) -> Result<String, super::ConfigError> {
        use std::io::Write;
        let path = self.path.join("install-id");
        let read = || -> Result<String, super::ConfigError> {
            let text = std::fs::read_to_string(&path)?;
            let id = uuid::Uuid::parse_str(text.trim()).map_err(|error| {
                super::ConfigError::Config(format!("invalid installation id: {error}"))
            })?;
            Ok(id.to_string())
        };
        match read() {
            Ok(id) => return Ok(id),
            Err(super::ConfigError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
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
}
