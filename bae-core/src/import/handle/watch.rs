use super::*;

impl ImportServiceHandle {
    /// The current watched-folder list. The UI fetches this when the import
    /// view appears to render the group headers, sidestepping the broadcast
    /// race (the list is durable; events only fire on later changes).
    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folder_registry.lock().unwrap().watched_folders()
    }

    /// Add a folder to watch for imports: persist it, broadcast the new list,
    /// and start watching + scanning it so its releases appear as candidates and
    /// later on-disk changes propagate. A folder already watched is left as-is.
    pub fn add_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let added = registry.add(&library_dir, path.clone())?;
        let folders = registry.watched_folders();
        drop(registry);
        if !added {
            return Ok(());
        }
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        self.watcher_tx
            .send(WatcherCommand::Watch(std::path::PathBuf::from(path)))
            .map_err(|_| "Failed to start watching folder".to_string())
    }

    /// Stop watching `path`: persist the removal, broadcast the new list, and
    /// stop the filesystem watcher for it. The reducer drops the folder's
    /// candidates by reconciling against the list, so no per-candidate removal
    /// events are needed here.
    pub fn remove_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let removed = registry.remove(&library_dir, &path)?;
        let folders = registry.watched_folders();
        drop(registry);
        if removed {
            self.candidate_state
                .lock()
                .unwrap()
                .remove_root(std::path::Path::new(&path));
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
            );
            self.watcher_tx
                .send(WatcherCommand::Unwatch(std::path::PathBuf::from(path)))
                .map_err(|_| "Failed to stop watching folder".to_string())?;
        }
        Ok(())
    }

    /// Start watching + scanning every watched folder, emitting one
    /// `FolderCandidate` per release found and `CandidateRemoved` for any that
    /// have since vanished. The UI calls this when the import view appears.
    pub fn scan_watched_folders(&self) -> Result<(), String> {
        let folders = self.folder_registry.lock().unwrap().watched_folders();
        for folder in folders {
            self.watcher_tx
                .send(WatcherCommand::Watch(std::path::PathBuf::from(folder.path)))
                .map_err(|_| "Failed to start watching folder".to_string())?;
        }
        Ok(())
    }
}
