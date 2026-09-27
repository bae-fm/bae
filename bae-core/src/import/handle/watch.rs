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

    /// Send a command to the folder-watch coordinator; a closed channel is an
    /// `Internal` error naming the action.
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

    /// Watch a folder, in whatever spelling the caller had. A folder already
    /// watched, or inside one, reads that watched folder again, so choosing it
    /// always visibly does something.
    pub async fn add_watched_folder(&self, path: String) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move { this.add_watched_folder_write(path).await })
            .await
    }

    /// Store `path` as watched and ask for it to be read. A folder holding
    /// watched folders takes them over.
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
            info!("{path} takes over the watched folders inside it: {inner:?}");
            return self
                .remove_watched_roots(inner, Some(std::path::PathBuf::from(&path)))
                .await;
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
                .remove_watched_import_folders(vec![path], None)
                .await?;
            return Err(error);
        }
        let folders = self.watched_folders().await?;
        self.event_tx.send(ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        Ok(())
    }

    /// Stop watching `path`. A failure leaves it watched and returns the
    /// error.
    pub async fn remove_watched_folder(
        &self,
        path: String,
    ) -> Result<(), crate::import::ImportError> {
        let path = crate::import::watched_folder::canonical_absolute_root(&path)?;
        self.remove_watched_roots(vec![std::path::PathBuf::from(&path)], None)
            .await
    }

    /// Have the coordinator stop watching `roots`, watching `parent` in their
    /// place when given, and return once that has landed. The coordinator
    /// holds the folder-state lock for the write, so this holds none.
    async fn remove_watched_roots(
        &self,
        roots: Vec<std::path::PathBuf>,
        parent: Option<std::path::PathBuf>,
    ) -> Result<(), crate::import::ImportError> {
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(
            WatcherCommand::Remove {
                roots,
                parent,
                completion,
            },
            "failed to request folder watch removal",
        )?;
        receiver
            .await
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "folder watch removal ended without a result".to_string(),
            })?
            .map_err(|detail| crate::import::ImportError::Watch { detail })
    }

    /// Ask for every watched folder to be read.
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

    /// Read the folder `key` names as `decision`, returning once the decision
    /// and the candidates it gives are stored.
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
