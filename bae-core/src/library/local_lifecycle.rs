//! Removing a library from this device: coven deletes the store, then bae
//! deletes its own files for it (the library's directory of preferences and
//! caches, and the active-library pointer when it names the library). The two
//! can't commit together, so a removal that stops between them leaves bae's
//! files for a store coven no longer holds. Those are read back from disk as an
//! [`UnfinishedRemoval`], never remembered by the process that started the
//! removal, and the person finishes it with [`finish_removals`].

use crate::config::{AppDir, Config, ConfigError};
use crate::library::LibraryError;
use std::collections::BTreeSet;
use tracing::debug;

#[derive(Debug, thiserror::Error)]
pub enum LibraryRemovalError {
    /// This device holds writes and files it hasn't uploaded, so nothing was
    /// removed. Removing with `IfUnsent::Discard` loses them.
    #[error("{} writes and {} files wait to upload", .0.writes, .0.files)]
    Unsent(coven::UnsentWork),
    /// coven didn't delete the store; bae's files for it are untouched.
    #[error(transparent)]
    Store(coven::StoreDeletionError),
    /// coven deleted the store, but deleting bae's files for it failed. The
    /// removal stays an [`UnfinishedRemoval`] until [`finish_removals`]
    /// deletes them.
    #[error("the store was removed, but deleting this device's files for it failed: {0}")]
    Unfinished(#[source] LibraryError),
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
            Self::Unfinished(source) => source.category(),
        }
    }
}

/// Libraries coven no longer holds whose files bae still keeps on this device:
/// a library directory, or the active-library pointer naming one. Only a
/// removal that stopped after coven deleted the store leaves them.
#[derive(Debug, thiserror::Error)]
#[error(
    "this device still has files for removed libraries: {}",
    .libraries.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
)]
pub struct UnfinishedRemoval {
    libraries: Vec<coven::StoreId>,
}

impl UnfinishedRemoval {
    /// The unfinished removal on this device, if any.
    pub async fn find(app_dir: &AppDir) -> Result<Option<Self>, LibraryError> {
        let libraries = unfinished_libraries(app_dir).await?;
        Ok((!libraries.is_empty()).then_some(Self { libraries }))
    }

    pub fn category(&self) -> crate::ui::UiErrorCategory {
        crate::ui::UiErrorCategory::Config
    }
}

/// Delete bae's files for every library coven no longer holds. Each step
/// succeeds on a file already gone, so retrying after any failure is safe.
pub async fn finish_removals(app_dir: &AppDir) -> Result<(), LibraryError> {
    for library in unfinished_libraries(app_dir).await? {
        delete_library_files(app_dir, library).await?;
    }
    Ok(())
}

/// Leave the store before removing this installation. Offline failures leave
/// the store and bae's files intact; the caller retries the same request.
/// The caller must stop its playback and import services first.
pub async fn remove_local_library(
    app_dir: &AppDir,
    handle: &coven::CovenHandle,
    unsent: coven::IfUnsent,
) -> Result<(), LibraryRemovalError> {
    let store_id = handle.store_id();
    handle.delete_store(unsent).await?;
    after_store_deleted(app_dir, store_id).await
}

/// The second half of a removal, once coven deleted `store_id`'s store.
pub(crate) async fn after_store_deleted(
    app_dir: &AppDir,
    store_id: coven::StoreId,
) -> Result<(), LibraryRemovalError> {
    delete_library_files(app_dir, store_id)
        .await
        .map_err(LibraryRemovalError::Unfinished)
}

/// Every library bae keeps files for, less the stores coven holds.
async fn unfinished_libraries(app_dir: &AppDir) -> Result<Vec<coven::StoreId>, LibraryError> {
    let stores = app_dir
        .store_layout()
        .stores()
        .await
        .map_err(ConfigError::from)?;
    let mut libraries = library_directories(app_dir).await?;
    if let Some(active) = Config::active_library_id(app_dir)? {
        libraries.insert(parse_library_id(&active).expect("the pointer read parses its id"));
    }
    for store in stores {
        libraries.remove(&store.id);
    }
    Ok(libraries.into_iter().collect())
}

/// The ids naming directories under the libraries directory. Anything else
/// there (a file, a name that isn't a library id) isn't bae's per-library
/// state and is passed over.
async fn library_directories(app_dir: &AppDir) -> Result<BTreeSet<coven::StoreId>, LibraryError> {
    let root = app_dir.libraries();
    tokio::task::spawn_blocking(move || {
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeSet::new())
            }
            Err(error) => return Err(io_at(&root, error)),
        };
        let mut libraries = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_at(&root, error))?;
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map_err(|error| io_at(&path, error))?
                .is_dir();
            match entry.file_name().to_str().and_then(parse_library_id) {
                Some(id) if is_dir => {
                    libraries.insert(id);
                }
                _ => debug!(path = %path.display(), "not a library directory"),
            }
        }
        Ok(libraries)
    })
    .await
    .map_err(unfinished_work)?
}

/// A library id exactly as bae names its directory: a canonical UUID.
fn parse_library_id(name: &str) -> Option<coven::StoreId> {
    let id = uuid::Uuid::parse_str(name).ok()?;
    (id.to_string() == name).then_some(coven::StoreId(id))
}

/// Clear the pointer if it names `store_id`, then delete the library's
/// directory. A failure partway leaves the directory, so the removal stays
/// listed until a retry finishes it.
async fn delete_library_files(
    app_dir: &AppDir,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    tokio::task::spawn_blocking(move || {
        let id = store_id.to_string();
        let pointer = app_dir.active_library_pointer();
        match std::fs::read_to_string(&pointer) {
            Ok(current) if current.trim() == id => {
                std::fs::remove_file(&pointer).map_err(|error| io_at(&pointer, error))?
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_at(&pointer, error)),
        }
        let directory = app_dir.registered_library(&id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_at(&directory, error)),
        }
    })
    .await
    .map_err(unfinished_work)?
}

fn unfinished_work(error: tokio::task::JoinError) -> LibraryError {
    LibraryError::Internal(format!("removal file work did not complete: {error}"))
}

/// Keep the path in the error: a bare "Permission denied" names no file.
fn io_at(path: &std::path::Path, error: std::io::Error) -> LibraryError {
    LibraryError::Io(std::io::Error::new(
        error.kind(),
        format!("{}: {error}", path.display()),
    ))
}
