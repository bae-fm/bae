use super::*;

impl ImportServiceHandle {
    /// The watched folders as the store lists them.
    pub async fn watched_folders(&self) -> Result<Vec<WatchedFolder>, crate::import::ImportError> {
        Ok(self.library_manager.load_watched_import_folders().await?)
    }

    async fn is_watched(&self, path: &str) -> Result<bool, crate::import::ImportError> {
        Ok(self
            .watched_folders()
            .await?
            .iter()
            .any(|folder| folder.path == path))
    }

    /// Send a command to the watcher's reconciliation task, turning a closed
    /// channel into a typed `Internal` error naming the action that couldn't
    /// be started.
    fn send_watcher_command(
        &self,
        command: WatcherCommand,
        on_closed: &str,
    ) -> Result<(), crate::import::ImportError> {
        self.watcher
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
    /// row is keyed by before anything here uses it, so the OS watch and the
    /// durable row name the folder the same way.
    ///
    /// Choosing a folder that is already watched, or one inside a watched
    /// folder, re-reads that watched folder. It is not an error and it must not
    /// be nothing: the user pointed at a folder and asked for it to be taken
    /// in, and a call that returned to a list which never moved — no scan, no
    /// status, no log line — is how a folder that could not be read stayed
    /// invisible however many times it was picked.
    pub async fn add_watched_folder(&self, path: String) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move { this.add_watched_folder_write(path).await })
            .await
    }

    /// Store `path` as watched and ask for it to be read.
    ///
    /// A folder holding watched folders is watched in their place rather
    /// than refused as overlapping them: they are the same files under a
    /// wider root, so what was decided about their candidates carries over.
    async fn add_watched_folder_write(
        &self,
        path: String,
    ) -> Result<(), crate::import::ImportError> {
        let path = crate::import::watched_folder::canonical_absolute_root(&path)?;
        let watched = self.watched_folders().await?;
        if let Some(covering) = watched.iter().find(|folder| {
            folder.path != path && std::path::Path::new(&path).starts_with(&folder.path)
        }) {
            info!("{path} is inside the watched folder {}; re-reading it", covering.path);
            return self.send_watcher_command(
                WatcherCommand::Rescan(std::path::PathBuf::from(&covering.path)),
                "Failed to start watching folder",
            );
        }
        let inner: Vec<std::path::PathBuf> = watched
            .into_iter()
            .map(|folder| std::path::PathBuf::from(folder.path))
            .filter(|root| root.as_path() != std::path::Path::new(&path) && root.starts_with(&path))
            .collect();
        if !inner.is_empty() {
            return self.adopt_watched_folders(path, inner).await;
        }
        let _commit = self.folder_state_commit.lock("add a watched folder").await;
        let added = self
            .library_manager
            .add_watched_import_folder(&path)
            .await?;
        let read = WatcherCommand::Rescan(std::path::PathBuf::from(&path));
        if !added {
            info!("{path} is already watched; re-reading it");
            return self.send_watcher_command(read, "Failed to start watching folder");
        }
        if let Err(error) = self.send_watcher_command(read, "Failed to start watching folder") {
            self.library_manager
                .remove_watched_import_folder(&path)
                .await?;
            return Err(error);
        }
        let folders = self.watched_folders().await?;
        self.event_tx.send(ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        Ok(())
    }

    /// Watch `parent` in place of the watched folders `inner` inside it, and
    /// return once that has landed. The coordinator does it — it stops what
    /// is reading them first — and holds the folder-state lock for the write,
    /// so this holds none while it waits. The write checks `inner` is still
    /// exactly what `parent` holds, so a folder watched or removed since this
    /// looked is an error rather than a wrong adoption.
    async fn adopt_watched_folders(
        &self,
        parent: String,
        inner: Vec<std::path::PathBuf>,
    ) -> Result<(), crate::import::ImportError> {
        info!("{parent} takes over the watched folders inside it: {inner:?}");
        let (adopted, landed) = tokio::sync::oneshot::channel();
        self.send_watcher_command(
            WatcherCommand::Adopt {
                parent: std::path::PathBuf::from(&parent),
                inner,
                adopted,
            },
            "failed to request watching a folder in place of the ones inside it",
        )?;
        landed
            .await
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "folder adoption ended without a result".to_string(),
            })?
            .map_err(|detail| crate::import::ImportError::Watch { detail })
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
        let path = crate::import::watched_folder::canonical_absolute_root(&path)?;
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

    /// Enqueue a scan for every watched folder. The coordinator reads the
    /// list from the store when it takes the command, so this needs no copy
    /// of it. Each blocking scan installs its optional OS watch before
    /// reading the directory. An unavailable root reports a failed scan and
    /// preserves its previous candidates.
    pub fn scan_watched_folders(&self) -> Result<(), crate::import::ImportError> {
        self.send_watcher_command(WatcherCommand::RescanAll, "Failed to start watching folder")
    }

    pub async fn refresh_watched_folder(
        &self,
        path: String,
    ) -> Result<(), crate::import::ImportError> {
        let path = crate::import::watched_folder::canonical_absolute_root(&path)?;
        if !self.is_watched(&path).await? {
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

    /// Read the folder `key` names as `decision`. Returns once the decision
    /// and the candidates it gives are stored — in one write, which also
    /// removes the candidates of the reading it replaces.
    pub(crate) async fn set_folder_release_decision(
        &self,
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    ) -> Result<(), crate::import::ImportError> {
        if !self.is_watched(&key.watched_folder_path).await? {
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
