use crate::config::AppDir;
use crate::library::LibraryError;
use std::ffi::OsStr;
use std::path::{Component, Path};

/// Remove a closed library from this device: every keyring entry coven holds
/// for it (its device identity, master key, and cloud credentials), every host
/// secret bae stores for it, and its directory. Its cloud copy and restore
/// code, if any, are untouched.
///
/// The library must be closed: coven refuses while the store is open
/// anywhere, and nothing is removed. The active-library pointer is cleared only
/// after the removal succeeds, and only when it names this library. This is
/// the welcome screen's removal of a library it never opened, and the end of
/// forgetting the active one once its handle is closed.
pub fn remove_local_library(app_dir: &AppDir, library_id: &str) -> Result<(), LibraryError> {
    validate_library_id(library_id)?;
    let active_pointer = app_dir.active_library_pointer();
    let clears_active_pointer =
        read_active_pointer(&active_pointer)?.as_deref() == Some(library_id);
    coven::Coven::delete_store(
        &coven::StoreDir::new(app_dir.registered_library(library_id)),
        library_id,
        crate::keys::HOST_SECRET_NAMES,
    )
    .map_err(store_deletion_error)?;
    if clears_active_pointer {
        match std::fs::remove_file(&active_pointer) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(LibraryError::Internal(format!(
                    "Failed to clear active-library pointer at {}: {error}",
                    active_pointer.display()
                )));
            }
        }
    }
    Ok(())
}

fn store_deletion_error(error: coven::StoreDeletionError) -> LibraryError {
    match error {
        coven::StoreDeletionError::Keyring(error) => LibraryError::Keyring(error),
        coven::StoreDeletionError::Open(error) => {
            LibraryError::Internal(format!("Failed to remove library data: {error}"))
        }
        coven::StoreDeletionError::Directory(error) => {
            LibraryError::Internal(format!("Failed to remove library data: {error}"))
        }
    }
}

fn validate_library_id(library_id: &str) -> Result<(), LibraryError> {
    let mut components = Path::new(library_id).components();
    let is_single_component = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(component)), None) if component == OsStr::new(library_id)
    );
    if !is_single_component {
        return Err(LibraryError::Validation(format!(
            "invalid library id: {library_id:?}"
        )));
    }
    Ok(())
}

fn read_active_pointer(active_pointer: &Path) -> Result<Option<String>, LibraryError> {
    match std::fs::read_to_string(active_pointer) {
        Ok(content) => {
            let library_id = content.trim();
            if library_id.is_empty() {
                return Err(LibraryError::Internal(format!(
                    "active-library pointer at {} is empty",
                    active_pointer.display()
                )));
            }
            Ok(Some(library_id.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LibraryError::Internal(format!(
            "Failed to read active-library pointer at {}: {error}",
            active_pointer.display()
        ))),
    }
}
