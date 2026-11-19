//! What a test reads and damages in bae's own directory: the registered
//! libraries, the active-library pointer and the removal records. Only
//! AppDir touches these files, so tests go through these calls.

use crate::AppDir;

impl AppDir {
    /// The names inside the directory `store` is registered at, or `None`
    /// when bae has no directory for it.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn registered_library_entries(&self, store: coven::StoreId) -> Option<Vec<String>> {
        let directory = self.registered_library(&store.to_string());
        match std::fs::read_dir(&directory) {
            Ok(entries) => Some(
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect(),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("{}: {error}", directory.display()),
        }
    }

    /// Whether the directory `store` is registered at is on disk.
    pub fn has_registered_library(&self, store: coven::StoreId) -> bool {
        self.registered_library_entries(store).is_some()
    }

    /// Whether a removal of `store` is recorded and not finished.
    pub fn has_removal_record(&self, store: coven::StoreId) -> bool {
        self.recorded_removals().unwrap().contains(&store)
    }

    /// Give `store` the directory bae keeps for a library this device has
    /// opened: its cache folder, created if missing.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn seed_registered_library(&self, store: coven::StoreId) {
        let cache = self.registered_library(&store.to_string()).join("cache");
        std::fs::create_dir_all(&cache).unwrap();
    }

    /// Put a folder where the active-library pointer file goes, so reading
    /// or clearing the pointer fails.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn block_active_library_pointer(&self) {
        std::fs::create_dir(self.active_library_pointer()).unwrap();
    }

    /// Remove the folder `block_active_library_pointer` put in the pointer's
    /// place.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn unblock_active_library_pointer(&self) {
        std::fs::remove_dir(self.active_library_pointer()).unwrap();
    }

    /// Put a file where the directory of removal records goes, so recording
    /// a removal fails.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn block_removal_records(&self) {
        std::fs::write(self.removal_records(), b"").unwrap();
    }

    /// Allow or refuse writes in the directory that holds every registered
    /// library, so deleting a library's directory fails or succeeds.
    #[cfg(unix)]
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn set_libraries_writable(&self, writable: bool) {
        use std::os::unix::fs::PermissionsExt;
        let mode = if writable { 0o755 } else { 0o555 };
        std::fs::set_permissions(self.libraries(), std::fs::Permissions::from_mode(mode)).unwrap();
    }
}
