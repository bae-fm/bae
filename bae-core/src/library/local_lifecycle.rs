use crate::keys::StoreKeys;
use crate::library::LibraryError;
use std::ffi::OsStr;
use std::path::{Component, Path};

#[derive(Clone, Copy)]
pub(crate) enum ActiveLibraryExpectation {
    MayBeInactive,
    MustNotNameAnotherLibrary,
}

/// Remove a registered library without opening its database first. This is the
/// welcome screen's path for a library whose database cannot be opened.
pub fn remove_local_library(library_id: &str) -> Result<(), LibraryError> {
    let bae_dir = crate::config::bae_dir()?;
    let keys = StoreKeys::bind(library_id.to_string());
    remove_local_library_from_bae_dir(
        &bae_dir,
        library_id,
        &keys,
        ActiveLibraryExpectation::MayBeInactive,
    )
}

pub(crate) fn remove_local_library_from_bae_dir(
    bae_dir: &Path,
    library_id: &str,
    keys: &StoreKeys,
    active_expectation: ActiveLibraryExpectation,
) -> Result<(), LibraryError> {
    validate_library_id(library_id)?;
    let active_pointer = bae_dir.join("active-library");
    let active_library_id = read_active_pointer(&active_pointer)?;
    if let Some(active_library_id) = &active_library_id {
        if matches!(
            active_expectation,
            ActiveLibraryExpectation::MustNotNameAnotherLibrary
        ) && active_library_id != library_id
        {
            return Err(LibraryError::Internal(format!(
                "active-library pointer at {} points at {active_library_id}, not {library_id}",
                active_pointer.display()
            )));
        }
    }

    let library_dir = crate::config::registered_library_path(bae_dir, library_id);
    match std::fs::remove_dir_all(&library_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(LibraryError::Internal(format!(
                "Failed to remove library data at {}: {error}",
                library_dir.display()
            )));
        }
    }

    if active_library_id.as_deref() == Some(library_id) {
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

    keys.delete_encryption_key()?;
    Ok(())
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
