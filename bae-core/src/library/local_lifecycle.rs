use crate::library::LibraryError;
use std::ffi::OsStr;
use std::path::{Component, Path};

#[derive(Clone, Copy)]
pub(crate) enum ActiveLibraryExpectation {
    MayBeInactive,
    MustNotNameAnotherLibrary,
}

pub(crate) struct PreparedLocalLibraryRemoval {
    library_dir: std::path::PathBuf,
    active_pointer: std::path::PathBuf,
    clears_active_pointer: bool,
}

/// Remove a registered library without opening its database first. This is the
/// welcome screen's path for a library whose database cannot be opened.
pub fn remove_local_library(library_id: &str) -> Result<(), LibraryError> {
    let bae_dir = crate::config::bae_dir()?;
    remove_local_library_from_bae_dir(
        &bae_dir,
        library_id,
        ActiveLibraryExpectation::MayBeInactive,
    )?;
    coven::Coven::forget_keyring_master_key(library_id)?;
    Ok(())
}

pub(crate) fn remove_local_library_from_bae_dir(
    bae_dir: &Path,
    library_id: &str,
    active_expectation: ActiveLibraryExpectation,
) -> Result<(), LibraryError> {
    prepare_local_library_removal(bae_dir, library_id, active_expectation)?.remove()
}

pub(crate) fn prepare_local_library_removal(
    bae_dir: &Path,
    library_id: &str,
    active_expectation: ActiveLibraryExpectation,
) -> Result<PreparedLocalLibraryRemoval, LibraryError> {
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
    if library_dir.exists() && !library_dir.is_dir() {
        return Err(LibraryError::Internal(format!(
            "Failed to remove library data at {}: path is not a directory",
            library_dir.display()
        )));
    }

    Ok(PreparedLocalLibraryRemoval {
        library_dir,
        active_pointer,
        clears_active_pointer: active_library_id.as_deref() == Some(library_id),
    })
}

impl PreparedLocalLibraryRemoval {
    pub(crate) fn remove(self) -> Result<(), LibraryError> {
        match std::fs::remove_dir_all(&self.library_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(LibraryError::Internal(format!(
                    "Failed to remove library data at {}: {error}",
                    self.library_dir.display()
                )));
            }
        }

        if self.clears_active_pointer {
            match std::fs::remove_file(&self.active_pointer) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(LibraryError::Internal(format!(
                        "Failed to clear active-library pointer at {}: {error}",
                        self.active_pointer.display()
                    )));
                }
            }
        }

        Ok(())
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
