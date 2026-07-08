//! Library-lifecycle operations for [`LibraryManager`]: rename a library,
//! lock it (forget the active encryption key), and forget a local library
//! (remove its data directory, clear the active-library pointer, and drop the
//! master key). These mutate the library's on-disk presence and the
//! active-library pointer — distinct from the config-access surface in
//! `config.rs`, which only reads and writes config fields.

use super::*;

impl LibraryManager {
    /// Rename a library by id. If the id matches the active library, the
    /// rename goes through the reactive `ConfigState` so all current
    /// subscribers see it. Otherwise the library's `config.yaml` on disk
    /// is edited in place — the inactive library isn't loaded into
    /// memory.
    pub fn rename_library(&self, library_id: &str, name: &str) -> Result<(), LibraryError> {
        if name.trim().is_empty() {
            return Err(LibraryError::Validation(
                "Library name cannot be empty".to_string(),
            ));
        }
        if library_id == self.config_handle.config().library_id {
            self.config_handle.rename_library(name)?;
            return Ok(());
        }
        let bae_dir = crate::config::bae_dir()?;
        crate::config::rename_inactive_library(&bae_dir, library_id, name)?;
        Ok(())
    }

    /// Forget the active library's encryption key. The running
    /// `sync_manager` still holds the key in memory so this session
    /// keeps working; the next launch lands on `UnlockView` because
    /// the keyring is empty. Used by the sidebar's "Lock Library" action.
    pub fn forget_encryption_key(&self) -> Result<(), LibraryError> {
        self.key_service.forget_encryption_key()?;
        Ok(())
    }

    /// Forget this (local) library on this device: remove its data directory,
    /// clear the active-library pointer, and delete its master encryption key. The
    /// owner's cloud copy (if any) is untouched — this only drops the device's
    /// local presence.
    ///
    /// The caller must drop this handle immediately afterward: the database
    /// lives in the directory being removed, so this must be the handle's last
    /// operation. The next launch re-discovers and opens another library (or
    /// onboards) since the active pointer is gone.
    pub fn forget_library(&self) -> Result<(), LibraryError> {
        let config = self.config_handle.config();
        let library_id = config.library_id.clone();
        let bae_dir = registered_bae_dir(config.library_dir.as_ref(), &library_id)?;
        let library_dir = crate::config::registered_library_path(&bae_dir, &library_id);
        let active_pointer = bae_dir.join("active-library");
        let remove_active_pointer = active_pointer_matches_library(&active_pointer, &library_id)?;

        match std::fs::remove_dir_all(&library_dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(LibraryError::Internal(format!(
                    "Failed to remove library data at {}: {e}",
                    library_dir.display()
                )));
            }
        }

        if remove_active_pointer {
            std::fs::remove_file(&active_pointer).map_err(|e| {
                LibraryError::Internal(format!(
                    "Failed to clear active-library pointer at {}: {e}",
                    active_pointer.display()
                ))
            })?;
        }

        self.key_service.delete_encryption_key()?;

        Ok(())
    }
}

fn active_pointer_matches_library(
    active_pointer: &std::path::Path,
    library_id: &str,
) -> Result<bool, LibraryError> {
    let content = match std::fs::read_to_string(active_pointer) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(LibraryError::Internal(format!(
                "Failed to read active-library pointer at {}: {e}",
                active_pointer.display()
            )));
        }
    };
    let active_library_id = content.trim();
    if active_library_id == library_id {
        return Ok(true);
    }
    Err(LibraryError::Internal(format!(
        "active-library pointer at {} points at {active_library_id}, not {library_id}",
        active_pointer.display()
    )))
}

fn registered_bae_dir(
    library_dir: &std::path::Path,
    library_id: &str,
) -> Result<std::path::PathBuf, LibraryError> {
    if library_dir.file_name() != Some(std::ffi::OsStr::new(library_id)) {
        return Err(LibraryError::Internal(format!(
            "library directory {} does not match library id {library_id}",
            library_dir.display()
        )));
    }

    let libraries_dir = library_dir.parent().ok_or_else(|| {
        LibraryError::Internal(format!(
            "library directory {} has no libraries parent",
            library_dir.display()
        ))
    })?;
    if libraries_dir.file_name() != Some(std::ffi::OsStr::new("libraries")) {
        return Err(LibraryError::Internal(format!(
            "library directory {} is not under a libraries directory",
            library_dir.display()
        )));
    }

    libraries_dir
        .parent()
        .map(|path| path.to_path_buf())
        .ok_or_else(|| {
            LibraryError::Internal(format!(
                "library directory {} has no bae directory parent",
                library_dir.display()
            ))
        })
}
