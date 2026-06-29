use super::*;
use coven::LibraryDir;

/// The watched-folders / scan-driving identity split out of
/// `ImportServiceHandle`: the persistent watched-folder list, the folder-watcher
/// command channel, and the shared handles the scan-driving methods need, held
/// as clones (the broadcast sender, the runtime handle, and the library dir).
/// It never points back at `ImportServiceHandle`.
#[derive(Clone)]
pub(crate) struct WatchedFolderControl {
    /// The persistent watched-folder list. Mutated by `add_watched_folder` /
    /// `remove_watched_folder`, which persist it and broadcast the new list.
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
    event_tx: broadcast::Sender<ImportEvent>,
    runtime_handle: tokio::runtime::Handle,
    /// The library's on-disk directory, where the registry persists its
    /// `import_folders.yaml` sibling file.
    library_dir: LibraryDir,
}

impl WatchedFolderControl {
    pub(crate) fn new(
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        runtime_handle: tokio::runtime::Handle,
        library_dir: LibraryDir,
    ) -> Self {
        Self {
            folder_registry,
            watcher_tx,
            event_tx,
            runtime_handle,
            library_dir,
        }
    }

    pub(crate) fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folder_registry.lock().unwrap().watched_folders()
    }

    pub(crate) fn add_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = &self.library_dir;
        let mut registry = self.folder_registry.lock().unwrap();
        let added = registry.add(library_dir, path.clone())?;
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

    pub(crate) fn remove_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = &self.library_dir;
        let mut registry = self.folder_registry.lock().unwrap();
        let removed = registry.remove(library_dir, &path)?;
        let folders = registry.watched_folders();
        drop(registry);
        if removed {
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

    pub(crate) fn scan_watched_folders(&self) -> Result<(), String> {
        let folders = self.folder_registry.lock().unwrap().watched_folders();
        for folder in folders {
            self.watcher_tx
                .send(WatcherCommand::Watch(std::path::PathBuf::from(folder.path)))
                .map_err(|_| "Failed to start watching folder".to_string())?;
        }
        Ok(())
    }

    pub(crate) fn set_candidate_skipped(&self, path: String, skipped: bool) -> Result<(), String> {
        let library_dir = &self.library_dir;
        let mut registry = self.folder_registry.lock().unwrap();
        let changed = registry.set_skipped(library_dir, path.clone(), skipped)?;
        drop(registry);
        if changed {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key: path,
                    skipped,
                }),
            );
        }
        Ok(())
    }

    pub(crate) fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut rx = self.event_tx.subscribe();
        let (tx, out_rx) = mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ImportEvent::Scan(event)) => {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scan event subscriber lagged by {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out_rx
    }
}
