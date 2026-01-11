use super::manager::LibraryManager;
use std::ops::Deref;
use std::sync::Arc;

/// Shared library manager that can be accessed from both UI and Subsonic server
#[derive(Clone, Debug)]
pub struct SharedLibraryManager {
    inner: Arc<LibraryManager>,
}

impl PartialEq for SharedLibraryManager {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Deref for SharedLibraryManager {
    type Target = LibraryManager;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl SharedLibraryManager {
    /// Create a new shared library manager
    pub fn new(library_manager: LibraryManager) -> Self {
        SharedLibraryManager {
            inner: Arc::new(library_manager),
        }
    }

    /// Get a reference to the library manager
    pub fn get(&self) -> &LibraryManager {
        &self.inner
    }

    /// Get a reference to the database
    pub fn database(&self) -> &crate::db::Database {
        self.inner.database()
    }
}
