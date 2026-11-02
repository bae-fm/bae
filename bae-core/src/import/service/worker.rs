use super::*;
use bae_util::worker_thread::WorkerThread;

impl ImportService {
    /// Start the import service worker: one task that drains the import queue
    /// sequentially, never concurrently. The returned handle is cloneable and
    /// is how the rest of the app submits import requests.
    pub async fn start(
        runtime_handle: tokio::runtime::Handle,
        database: crate::db::Database,
        catalogs: crate::import::CatalogSessions,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        diagnostics: crate::diagnostics::Diagnostics,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        closing: tokio_util::sync::CancellationToken,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        // No run, import or open list outlives the process that held it, so
        // the library opens with no candidate standing and no list, before
        // anything reads the list.
        let tables = crate::import::tables::ImportTables::new(database.clone());
        tables.clear_candidate_standings().await?;
        tables.close_import_lists().await?;
        let runtime = CandidateRuntime::new(tables, diagnostics.clone());
        let event_tx = crate::import::handle::ImportEventBus::new(runtime.clone());
        let services = crate::import::ImportServices::new(
            event_tx,
            database,
            catalogs,
            remote_images,
            preparations,
            clock.clone(),
            ids.clone(),
            diagnostics.clone(),
            closing,
        );
        Self::start_with(runtime_handle, services, runtime, diagnostics)
    }

    /// [`Self::start`] reading folders' tags through `file_tags`, for a test
    /// that decides how long a read takes.
    #[cfg(test)]
    pub(crate) fn start_reading_tags_with(
        runtime_handle: tokio::runtime::Handle,
        database: crate::db::Database,
        catalogs: crate::import::CatalogSessions,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        diagnostics: crate::diagnostics::Diagnostics,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        closing: tokio_util::sync::CancellationToken,
        file_tags: Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> ImportServiceHandle {
        let runtime = CandidateRuntime::new(
            crate::import::tables::ImportTables::new(database.clone()),
            diagnostics.clone(),
        );
        let event_tx = crate::import::handle::ImportEventBus::new(runtime.clone());
        let mut services = crate::import::ImportServices::new(
            event_tx,
            database,
            catalogs,
            remote_images,
            preparations,
            clock,
            ids,
            diagnostics.clone(),
            closing,
        );
        services.file_tags = file_tags;
        Self::start_with(runtime_handle, services, runtime, diagnostics)
            .expect("a test's library is open while its import service starts")
    }

    fn start_with(
        runtime_handle: tokio::runtime::Handle,
        services: crate::import::ImportServices,
        runtime: CandidateRuntime,
        diagnostics: crate::diagnostics::Diagnostics,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<WatchReport>();
        let event_tx_for_worker = services.event_tx.clone();
        let runtime_for_worker = runtime.clone();
        let database = services.database.clone();
        let tables = services.tables.clone();
        let clock = services.clock.clone();
        let ids = services.ids.clone();
        let import_cancels = services.import_cancels.clone();
        let catalogs = services.catalogs.clone();
        let remote_images = services.remote_images.clone();

        // Constructed before the watcher task spawns; the task doesn't need the
        // watcher, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx, diagnostics.clone()));

        let scan = ScanServices::new(services.clone(), folder_watcher);
        let watcher = WorkerThread::spawn("folder scan coordinator", move |_, watcher_rx| {
            ImportService::start_watcher(watcher_rx, fs_rx, scan)
        });

        let worker = WorkerThread::spawn("import worker thread", move |_, commands_rx| {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create import runtime");
                rt.block_on(async move {
                    let mut service = ImportService {
                        commands_rx,
                        event_tx: event_tx_for_worker,
                        runtime: runtime_for_worker,
                        database,
                        tables,
                        catalogs,
                        remote_images,
                        clock,
                        ids,
                        import_cancels,
                        diagnostics,
                    };

                    while let Some(message) = service.commands_rx.recv().await {
                        match message {
                            ImportWorkerMessage::Import { command, expectation } => {
                                service.do_import(command, expectation).await
                            }
                            ImportWorkerMessage::Shutdown => break,
                        }
                    }
                });
            })
        });

        ImportServiceHandle::new(worker, watcher, services, runtime, runtime_handle)
    }
}
