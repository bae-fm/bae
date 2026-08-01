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

    /// Add a folder to the durable scan set. The OS watch is an accelerator;
    /// launch, manual, and periodic scans remain authoritative when it is not
    /// available for a network filesystem.
    ///
    /// `path` is whatever spelling the caller had — a picker's, a `file://`
    /// drop's, a `bae://import` link's. It is settled to the one spelling the
    /// row is keyed by before anything here uses it, so the in-memory registry,
    /// the OS watch, and the durable row all name the folder the same way.
    pub async fn add_watched_folder(&self, path: String) -> Result<(), crate::import::ImportError> {
        let path = crate::import::folder_registry::canonical_absolute_root(&path)?;
        let _commit = self.folder_state_commit.lock().await;
        let added = self
            .library_manager
            .add_watched_import_folder(&path)
            .await?;
        if !added {
            return Ok(());
        }
        let folders = {
            let mut registry = self.folder_registry.lock().unwrap();
            registry.apply_added(path.clone());
            registry.watched_folders()
        };
        if let Err(error) = self.send_watcher_command(
            WatcherCommand::Rescan(std::path::PathBuf::from(&path)),
            "Failed to start watching folder",
        ) {
            self.library_manager
                .remove_watched_import_folder(&path)
                .await?;
            self.folder_registry.lock().unwrap().apply_removed(&path);
            return Err(error);
        }
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        Ok(())
    }

    /// Stop watching `path`. The coordinator first cancels its scan and
    /// uninstalls its OS watch. Only after that succeeds does this remove the
    /// durable row and in-memory candidates, then broadcast their removal.
    /// An uninstall failure leaves the watched folder and its stored scan intact
    /// and returns the error to the caller.
    pub async fn remove_watched_folder(
        &self,
        path: String,
    ) -> Result<(), crate::import::ImportError> {
        let path = crate::import::folder_registry::canonical_absolute_root(&path)?;
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(
            WatcherCommand::Remove {
                path: std::path::PathBuf::from(&path),
                completion,
            },
            "failed to request folder watch removal",
        )?;
        receiver
            .await
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "folder watch removal ended without a result".to_string(),
            })?
            .map_err(|detail| crate::import::ImportError::Watch { detail })?;
        Ok(())
    }

    /// Enqueue a scan for every watched folder. Each blocking scan installs its
    /// optional OS watch before reading the directory. An unavailable root
    /// reports a failed scan and preserves its previous candidates.
    pub fn scan_watched_folders(&self) -> Result<(), crate::import::ImportError> {
        let folders = self.folder_registry.lock().unwrap().watched_folders();
        for folder in folders {
            let path_buf = std::path::PathBuf::from(&folder.path);
            self.send_watcher_command(
                WatcherCommand::Rescan(path_buf),
                "Failed to start watching folder",
            )?;
        }
        Ok(())
    }

    pub async fn refresh_watched_folder(
        &self,
        path: String,
    ) -> Result<(), crate::import::ImportError> {
        let path = crate::import::folder_registry::canonical_absolute_root(&path)?;
        let registered = self
            .folder_registry
            .lock()
            .unwrap()
            .watched_folders()
            .iter()
            .any(|folder| folder.path == path);
        if !registered {
            return Err(crate::import::ImportError::Watch {
                detail: format!("{path} is not a watched folder"),
            });
        }
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(
            WatcherCommand::Refresh {
                path: std::path::PathBuf::from(path),
                completion,
            },
            "failed to request folder refresh",
        )?;
        receiver
            .await
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "folder refresh task ended without a result".to_string(),
            })?
            .map_err(|detail| crate::import::ImportError::Watch { detail })
    }

    pub async fn set_folder_release_decision(
        &self,
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    ) -> Result<(), crate::import::ImportError> {
        let registered = self
            .folder_registry
            .lock()
            .unwrap()
            .watched_folders()
            .iter()
            .any(|folder| folder.path == key.watched_folder_path);
        if !registered {
            return Err(crate::import::ImportError::Watch {
                detail: format!("{} is not a watched folder", key.watched_folder_path),
            });
        }
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(
            WatcherCommand::SetFolderReleaseDecision {
                target: (key, decision),
                completion,
            },
            "failed to set folder release decision",
        )?;
        receiver
            .await
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "folder decision task ended without a result".to_string(),
            })?
            .map_err(|detail| crate::import::ImportError::Watch { detail })
    }
}
