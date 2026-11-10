use super::*;

impl ImportServiceHandle {
    /// The watched folders as the store lists them.
    pub async fn watched_folders(&self) -> Result<Vec<WatchedFolder>, crate::ImportError> {
        Ok(self.services.tables.load_watched_import_folders().await?)
    }

    async fn is_watched(&self, path: &Path) -> Result<bool, crate::ImportError> {
        Ok(self.services.tables.is_watched_import_folder(path).await?)
    }

    /// Send a command to the folder-watch coordinator, or fail with how the
    /// coordinator ended.
    fn send_watcher_command(&self, command: WatcherCommand) -> Result<(), crate::ImportError> {
        self.watcher.send(command).map_err(|ended| ended.end.into())
    }

    /// Watch a folder, in whatever spelling the caller had. A folder already
    /// watched, or inside one, reads that watched folder again, so choosing it
    /// always visibly does something.
    pub async fn add_watched_folder(
        &self,
        path: std::path::PathBuf,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        self.committed(async move { this.add_watched_folder_write(path).await }).await
    }

    /// Store `path` as watched and ask for it to be read. A folder holding
    /// watched folders takes them over.
    async fn add_watched_folder_write(
        &self,
        path: std::path::PathBuf,
    ) -> Result<(), crate::ImportError> {
        let path = crate::tables::watched_path::canonical_absolute_root(&path)?;
        let shown = coven::path_text(&path);
        if let Some(covering) = self.services.tables.watched_import_folder_above(&path).await? {
            info!(
                "{shown} is inside the watched folder {}; re-reading it",
                coven::path_text(&covering)
            );
            return self.send_watcher_command(WatcherCommand::Rescan(covering));
        }
        let inner: Vec<std::path::PathBuf> =
            self.services.tables.watched_import_folders_below(&path).await?;
        if !inner.is_empty() {
            info!(
                "{shown} takes over the watched folders inside it: {:?}",
                inner.iter().map(|folder| coven::path_text(folder)).collect::<Vec<_>>()
            );
            return self.remove_watched_roots(inner, Some(path)).await;
        }
        let _commit = self.services.folder_state_commit.lock("add a watched folder").await;
        let added = self.services.tables.add_watched_import_folder(&path).await?;
        let read = WatcherCommand::Rescan(path.clone());
        let Some(folders) = added else {
            info!("{shown} is already watched; re-reading it");
            return self.send_watcher_command(read);
        };
        if let Err(error) = self.send_watcher_command(read) {
            self.services.tables.remove_watched_import_folders(vec![path], None).await?;
            return Err(error);
        }
        self.services
            .event_tx
            .send(ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }));
        Ok(())
    }

    /// Stop watching `path`. A failure leaves it watched and returns the
    /// error.
    pub async fn remove_watched_folder(
        &self,
        path: std::path::PathBuf,
    ) -> Result<(), crate::ImportError> {
        let path = crate::tables::watched_path::canonical_absolute_root(&path)?;
        self.remove_watched_roots(vec![path], None).await
    }

    /// Have the coordinator stop watching `roots`, watching `parent` in their
    /// place when given, and return once that has landed. The coordinator
    /// holds the folder-state lock for the write, so this holds none.
    async fn remove_watched_roots(
        &self,
        roots: Vec<std::path::PathBuf>,
        parent: Option<std::path::PathBuf>,
    ) -> Result<(), crate::ImportError> {
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(WatcherCommand::Remove { roots, parent, completion })?;
        receiver
            .await
            .map_err(|error| crate::ImportError::Internal {
                detail: format!("folder watch removal ended without a result: {error}"),
            })?
            .map_err(|detail| crate::ImportError::Watch { detail })
    }

    /// Ask for every watched folder to be read, returning once each read is
    /// asked for, or why the watched folders could not be listed.
    pub async fn scan_watched_folders(&self) -> Result<(), crate::ImportError> {
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(WatcherCommand::RescanAll { completion })?;
        receiver
            .await
            .map_err(|error| crate::ImportError::Internal {
                detail: format!("the watched folders' rescan ended without a result: {error}"),
            })?
            .map_err(crate::ImportError::from)
    }

    /// Read every watched folder and return, once each read has ended, where
    /// each one's scan stands.
    pub async fn read_watched_folders(
        &self,
    ) -> Result<Vec<crate::WatchedFolderScanStatus>, crate::ImportError> {
        let mut reads = tokio::task::JoinSet::new();
        for folder in self.watched_folders().await? {
            let this = self.clone();
            #[expect(
                clippy::disallowed_methods,
                reason = "joined below: each read's panic comes back at its join"
            )]
            reads.spawn(bae_util::task_failure::caught("read a watched folder", async move {
                this.refresh_watched_folder(folder.path).await
            }));
        }
        while let Some(read) = reads.join_next().await {
            // Each read catches its own panic, so its join fails only as its
            // runtime ends it.
            let read = read.map_err(|error| {
                self.services.diagnostics.bug(bae_util::task_failure::TaskFailed::of_join(
                    "read a watched folder",
                    error,
                ))
            })?;
            read.map_err(|bug| {
                self.services.diagnostics.bug(bae_util::task_failure::TaskFailed::Panicked(bug))
            })??;
        }
        Ok(self.folder_scan_progress().await?.statuses)
    }

    /// Where each watched folder's scan stands now: the stored scans read
    /// after the coordinator's memory, as [`crate::folder_scan_status`]'s live
    /// read puts them together.
    async fn folder_scan_progress(&self) -> Result<crate::FolderScanProgress, crate::ImportError> {
        let in_memory = self.scans_in_memory.borrow().clone();
        let stored = self.services.tables.load_stored_folder_scans().await?;
        Ok(crate::FolderScanProgress::of(stored, &in_memory))
    }

    pub async fn refresh_watched_folder(
        &self,
        path: std::path::PathBuf,
    ) -> Result<(), crate::ImportError> {
        let path = crate::tables::watched_path::canonical_absolute_root(&path)?;
        if !self.is_watched(&path).await? {
            return Err(crate::ImportError::Watch {
                detail: format!("{} is not a watched folder", coven::path_text(&path)),
            });
        }
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(WatcherCommand::Refresh { path, completion })?;
        receiver
            .await
            .map_err(|error| crate::ImportError::Internal {
                detail: format!("folder refresh task ended without a result: {error}"),
            })?
            .map_err(|detail| crate::ImportError::Watch { detail })
    }

    /// Read the folder `key` names as `decision`, returning once the decision
    /// and the candidates it gives are stored, with the keys of the releases
    /// the folder now reads as, in path order.
    pub(crate) async fn set_folder_release_decision(
        &self,
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    ) -> Result<Vec<crate::CandidateKey>, crate::ImportError> {
        if !self.is_watched(&key.watched_folder_path).await? {
            return Err(crate::ImportError::Watch {
                detail: format!(
                    "{} is not a watched folder",
                    coven::path_text(&key.watched_folder_path)
                ),
            });
        }
        let (completion, receiver) = tokio::sync::oneshot::channel();
        self.send_watcher_command(WatcherCommand::SetFolderReleaseDecision {
            target: (key, decision),
            completion,
        })?;
        receiver
            .await
            .map_err(|error| crate::ImportError::Internal {
                detail: format!("folder decision task ended without a result: {error}"),
            })?
            .map_err(|detail| crate::ImportError::Watch { detail })
    }

    /// The first folder scans `accept` admits, waiting through the query's
    /// values until one does.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_folder_scans(
        &self,
        mut accept: impl FnMut(&crate::FolderScanProgress) -> bool,
    ) -> crate::FolderScanProgress {
        let mut scans = crate::folder_scan_status::FolderScansLive::new(
            self.services.tables.clone(),
            self.scans_in_memory.clone(),
        );
        loop {
            let progress = scans
                .next()
                .await
                .expect("the folder scan query stays open")
                .expect("the folder scan query reads");
            if accept(&progress) {
                return progress;
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
