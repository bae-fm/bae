use std::path::{Path, PathBuf};

/// bae-owned preferences and caches, separate from coven's `stores` directory.
const LIBRARIES_DIRNAME: &str = "libraries";

/// bae's own directory: `.bae` under the home directory the host passes in.
/// Every registered library and the active-library pointer live under it. Built once at process start and passed to whatever reads or
/// writes under it.
#[derive(Debug, Clone)]
pub struct AppDir {
    path: PathBuf,
}

impl AppDir {
    /// The app directory under `home`: `home/.bae`.
    pub fn under_home(home: &Path) -> Self {
        Self {
            path: home.join(".bae"),
        }
    }

    /// An app directory at exactly `path`, for tests that treat a temp dir as
    /// the app directory itself.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The directory every registered library's own directory is in.
    pub fn libraries(&self) -> PathBuf {
        self.path.join(LIBRARIES_DIRNAME)
    }

    /// The directory the library `library_id` is registered at.
    pub fn registered_library(&self, library_id: &str) -> PathBuf {
        self.libraries().join(library_id)
    }

    /// The file naming the library this device last opened.
    pub fn active_library_pointer(&self) -> PathBuf {
        self.path.join("active-library")
    }

    /// coven owns store directories under the app directory.
    pub(crate) fn store_layout(&self) -> coven::StoreLayout {
        coven::StoreLayout::new(self.path.clone())
    }

    /// A bae-owned installation id shared by this installation's libraries.
    /// Publishing without replacement makes concurrent first launches agree.
    pub(crate) fn install_id(
        &self,
        ids: &dyn coven::IdSource,
    ) -> Result<String, super::ConfigError> {
        use std::io::Write;
        self.create()?;
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
                std::fs::File::open(&self.path)?.sync_all()?;
                Ok(id)
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => read(),
            Err(error) => Err(error.error.into()),
        }
    }

    /// Create the app directory itself if it does not exist yet.
    pub(crate) fn create(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.path)
    }
}
