use std::path::{Path, PathBuf};

/// The directory registered libraries live under, as its name inside the app
/// directory. coven's store layout is told the same name, so a library it
/// restores or joins lands where discovery looks.
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

    /// coven's layout for bae's stores: a store restored or joined through it
    /// lands at [`Self::registered_library`], as a created one does.
    pub(crate) fn store_layout(&self) -> coven::StoreLayout {
        coven::StoreLayout::new(self.path.clone()).stores_dirname(LIBRARIES_DIRNAME)
    }

    /// Create the app directory itself if it does not exist yet.
    pub(crate) fn create(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.path)
    }
}
