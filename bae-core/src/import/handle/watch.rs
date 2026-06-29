use super::*;

impl ImportServiceHandle {
    /// The current watched-folder list. The UI fetches this when the import
    /// view appears to render the group headers, sidestepping the broadcast
    /// race (the list is durable; events only fire on later changes).
    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.watched_folders.watched_folders()
    }

    /// Add a folder to watch for imports: persist it, broadcast the new list,
    /// and start watching + scanning it so its releases appear as candidates and
    /// later on-disk changes propagate. A folder already watched is left as-is.
    pub fn add_watched_folder(&self, path: String) -> Result<(), String> {
        self.watched_folders.add_watched_folder(path)
    }

    /// Stop watching `path`: persist the removal, broadcast the new list, and
    /// stop the filesystem watcher for it. The reducer drops the folder's
    /// candidates by reconciling against the list, so no per-candidate removal
    /// events are needed here.
    pub fn remove_watched_folder(&self, path: String) -> Result<(), String> {
        self.watched_folders.remove_watched_folder(path)
    }

    /// Start watching + scanning every watched folder, emitting one
    /// `FolderCandidate` per release found and `CandidateRemoved` for any that
    /// have since vanished. The UI calls this when the import view appears.
    pub fn scan_watched_folders(&self) -> Result<(), String> {
        self.watched_folders.scan_watched_folders()
    }
}
