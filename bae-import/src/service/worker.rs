use super::*;
use crate::watching::{start_watcher, FolderWatcher, WatchReport};
use bae_util::worker_thread::WorkerThread;

impl ImportService {
    /// Start the import service worker: one task that drains the import queue
    /// sequentially, never concurrently. The returned handle is cloneable and
    /// is how the rest of the app submits import requests.
    pub async fn start(
        runtime_handle: tokio::runtime::Handle,
        store: bae_store::Store,
        records: bae_records::Records,
        catalogs: crate::CatalogSessions,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        diagnostics: bae_diagnostics::Diagnostics,
        preparations: crate::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        closing: tokio_util::sync::CancellationToken,
    ) -> Result<ImportServiceHandle, crate::ImportError> {
        Self::start_reading_tags_through(
            runtime_handle,
            store,
            records,
            catalogs,
            remote_images,
            diagnostics,
            preparations,
            clock,
            ids,
            closing,
            Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        )
        .await
    }

    /// [`Self::start`] reading folders' tags through `file_tags`: Lofty for
    /// the app, a reader a test decides the answers and timing of for a
    /// test.
    pub(crate) async fn start_reading_tags_through(
        runtime_handle: tokio::runtime::Handle,
        store: bae_store::Store,
        records: bae_records::Records,
        catalogs: crate::CatalogSessions,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        diagnostics: bae_diagnostics::Diagnostics,
        preparations: crate::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        closing: tokio_util::sync::CancellationToken,
        file_tags: Arc<dyn crate::file_tag_reader::FileTagReader>,
    ) -> Result<ImportServiceHandle, crate::ImportError> {
        // No run, import or open list outlives the process that held it, so
        // the library opens with no candidate standing and no list, before
        // anything reads the list.
        let tables = crate::tables::ImportTables::new(store.clone());
        tables.clear_candidate_standings().await?;
        tables.close_import_lists().await?;
        let runtime = CandidateRuntime::new(tables, diagnostics.clone());
        let event_tx = crate::ImportEventBus::new(runtime.clone());
        let services = crate::ImportServices::new(
            event_tx,
            store,
            records,
            catalogs,
            remote_images,
            preparations,
            clock,
            ids,
            file_tags,
            diagnostics.clone(),
            closing,
        );
        Self::start_with(runtime_handle, services, runtime, diagnostics)
    }

    fn start_with(
        runtime_handle: tokio::runtime::Handle,
        services: crate::ImportServices,
        runtime: CandidateRuntime,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Result<ImportServiceHandle, crate::ImportError> {
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<WatchReport>();
        let event_tx_for_worker = services.event_tx.clone();
        let runtime_for_worker = runtime.clone();
        let records = services.records.clone();
        let tables = services.tables.clone();
        let clock = services.clock.clone();
        let ids = services.ids.clone();
        let import_cancels = services.import_cancels.clone();
        let catalogs = services.catalogs.clone();
        let remote_images = services.remote_images.clone();

        // Constructed before the watcher task spawns; the task doesn't need the
        // watcher, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx, diagnostics.clone()));

        let watched = services.clone();
        let (scans_in_memory_tx, scans_in_memory) =
            tokio::sync::watch::channel(crate::folder_scan_status::FolderScansInMemory::new());
        let watcher = WorkerThread::spawn("folder scan coordinator", move |_, watcher_rx| {
            start_watcher(watcher_rx, fs_rx, watched, folder_watcher, scans_in_memory_tx)
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
                        records,
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

        ImportServiceHandle::new(
            worker,
            watcher,
            scans_in_memory,
            services,
            runtime,
            runtime_handle,
        )
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
