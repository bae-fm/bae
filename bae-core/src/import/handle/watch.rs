use super::*;

impl ImportServiceHandle {
    /// The current watched-folder list. The UI fetches this when the import
    /// view appears to render the group headers, sidestepping the broadcast
    /// race (the list is durable; events only fire on later changes).
    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folder_registry.lock().unwrap().watched_folders()
    }

    /// Send a command to the watcher's reconciliation task, turning a closed
    /// channel into a typed `Internal` error naming the action that couldn't
    /// be started.
    fn send_watcher_command(
        &self,
        command: WatcherCommand,
        on_closed: &str,
    ) -> Result<(), crate::import::ImportError> {
        self.watcher_tx
            .send(command)
            .map_err(|_| crate::import::ImportError::Internal {
                detail: on_closed.to_string(),
            })
    }

    /// Add a folder to watch for imports, atomically with installing its OS
    /// watch: persist it, install the watch, and only on success broadcast the
    /// new list and trigger the initial scan. An already-watched folder is left
    /// as-is.
    ///
    /// If the watch can't be installed (missing path, permissions), the persisted
    /// add is rolled back and the failure returns to the caller. The UI must not
    /// believe a folder is watched when it isn't, and a registry entry for an
    /// unwatchable folder would silently no-op every later install attempt, since
    /// `FolderWatcher::install` is keyed on success.
    pub fn add_watched_folder(&self, path: String) -> Result<(), crate::import::ImportError> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let added = registry.add(&library_dir, path.clone())?;
        let folders = registry.watched_folders();
        drop(registry);
        if !added {
            return Ok(());
        }

        let path_buf = std::path::PathBuf::from(&path);
        if let Err(watch_err) = self.folder_watcher.install(&path_buf) {
            let mut registry = self.folder_registry.lock().unwrap();
            if let Err(rollback_err) = registry.remove(&library_dir, &path) {
                return Err(crate::import::ImportError::Watch {
                    detail: format!(
                        "{watch_err}; rolling back the registry add also failed: {rollback_err}"
                    ),
                });
            }
            return Err(watch_err);
        }

        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        self.send_watcher_command(
            WatcherCommand::Rescan(path_buf),
            "Failed to start watching folder",
        )
    }

    /// Stop watching `path`: persist the removal, broadcast the new list, and
    /// uninstall its OS watch. Each of the folder's candidates gets a
    /// `CandidateRemoved` so the extraction service cancels their in-flight work
    /// and bus consumers drop the keys.
    ///
    /// Unlike `add_watched_folder`, an uninstall failure never rolls the registry
    /// removal back: the user asked for the folder to stop being watched, and the
    /// registry (the durable authority) ending up correct matters more than a
    /// leaked OS watch, whose events the reconciliation task ignores once the
    /// folder is gone from the registry.
    pub fn remove_watched_folder(&self, path: String) -> Result<(), crate::import::ImportError> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let removed = registry.remove(&library_dir, &path)?;
        let folders = registry.watched_folders();
        drop(registry);
        if removed {
            let removed_keys = self
                .candidate_state
                .lock()
                .unwrap()
                .remove_root(std::path::Path::new(&path));
            for candidate_key in removed_keys {
                send_event(
                    &self.event_tx,
                    ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }),
                );
            }
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
            );
            self.folder_watcher.uninstall(std::path::Path::new(&path))?;
        }
        Ok(())
    }

    /// Install the OS watch (if not already installed) for every watched folder
    /// and re-scan each, emitting one `FolderCandidate` per release found and a
    /// `CandidateRemoved` for any that has since vanished. Called when the import
    /// view appears, and once at app launch.
    ///
    /// Unlike `add_watched_folder`, a folder whose watch fails to install here
    /// still gets its re-scan — the folder is durable user intent, not a fresh
    /// request to refuse, and `rescan_and_reconcile`'s missing-root handling
    /// reconciles an unreachable one (an unplugged drive) to no candidates. Every
    /// install failure is collected into one `Err` naming the failed folders,
    /// while the rest proceed; retrying later is safe, since
    /// `FolderWatcher::install` is idempotent.
    pub fn scan_watched_folders(&self) -> Result<(), crate::import::ImportError> {
        let folders = self.folder_registry.lock().unwrap().watched_folders();
        let mut failed = Vec::new();
        for folder in folders {
            let path_buf = std::path::PathBuf::from(&folder.path);
            if let Err(e) = self.folder_watcher.install(&path_buf) {
                warn!("failed to watch {}: {e}", folder.path);
                failed.push(folder.path);
            }
            self.send_watcher_command(
                WatcherCommand::Rescan(path_buf),
                "Failed to start watching folder",
            )?;
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(crate::import::ImportError::Watch {
                detail: format!("failed to watch: {}", failed.join(", ")),
            })
        }
    }
}
