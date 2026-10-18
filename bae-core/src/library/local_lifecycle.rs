use crate::config::AppDir;
use crate::library::LibraryError;

/// A store already removed by coven; only bae's preferences and pointer remain.
#[derive(Debug)]
pub struct RemovedLibrary {
    store_id: coven::StoreId,
}

impl RemovedLibrary {
    pub(super) fn new(store_id: coven::StoreId) -> Self {
        Self { store_id }
    }

    /// Retry only the local cleanup; never try to leave an already deleted store.
    pub async fn finish(self, app_dir: &AppDir) -> Result<(), LibraryRemovalError> {
        remove_preferences(app_dir, self.store_id)
            .await
            .map_err(|source| LibraryRemovalError::Preferences {
                library: self,
                source,
            })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryRemovalError {
    /// This device holds writes and files it hasn't uploaded, so nothing was
    /// removed. Removing with `IfUnsent::Discard` loses them.
    #[error("{} writes and {} files wait to upload", .0.writes, .0.files)]
    Unsent(coven::UnsentWork),
    #[error(transparent)]
    Store(coven::StoreDeletionError),
    #[error("the store was removed, but deleting its preferences failed: {source}")]
    Preferences {
        library: RemovedLibrary,
        #[source]
        source: LibraryError,
    },
}

impl From<coven::StoreDeletionError> for LibraryRemovalError {
    fn from(error: coven::StoreDeletionError) -> Self {
        match error {
            coven::StoreDeletionError::Unsent(work) => Self::Unsent(work),
            error => Self::Store(error),
        }
    }
}

impl LibraryRemovalError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        match self {
            Self::Unsent(_) => crate::ui::UiErrorCategory::Config,
            Self::Store(error) => super::manager::deletion_category(error),
            Self::Preferences { source, .. } => source.category(),
        }
    }
}

/// Leave the store before removing this installation. Offline failures leave
/// the store and bae's preferences intact; the caller retries the same request.
/// The caller must stop its playback and import services first.
pub async fn remove_local_library(
    app_dir: &AppDir,
    handle: &coven::CovenHandle,
    unsent: coven::IfUnsent,
) -> Result<(), LibraryRemovalError> {
    let store_id = handle.store_id();
    handle.delete_store(unsent).await?;
    RemovedLibrary::new(store_id).finish(app_dir).await
}

async fn remove_preferences(
    app_dir: &AppDir,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    tokio::task::spawn_blocking(move || {
        let id = store_id.to_string();
        let directory = app_dir.registered_library(&id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LibraryError::Io(error)),
        }
        let pointer = app_dir.active_library_pointer();
        let current = match std::fs::read_to_string(&pointer) {
            Ok(current) => current,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(LibraryError::Io(error)),
        };
        if current.trim() == id {
            std::fs::remove_file(pointer)?;
        }
        Ok(())
    })
    .await?
}
