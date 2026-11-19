use super::*;
use crate::watching::{run_watcher, FolderWatcher, WatchReport};
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
        tables: crate::tables::ImportTables,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        files: coven::FileBudget,
        closing: tokio_util::sync::CancellationToken,
    ) -> Result<ImportServiceHandle, crate::ImportError> {
        Self::start_reading_tags_through(
            runtime_handle,
            store,
            records,
            catalogs,
            remote_images,
            diagnostics,
            tables,
            clock,
            ids,
            files,
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
        tables: crate::tables::ImportTables,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        files: coven::FileBudget,
        closing: tokio_util::sync::CancellationToken,
        file_tags: Arc<dyn crate::file_tag_reader::FileTagReader>,
    ) -> Result<ImportServiceHandle, crate::ImportError> {
        // No run, import, catalog release that bae is applying,
        // release edit session, held folder scan or raised scan alert
        // outlives the process, so bae clears them all before anything reads
        // them.
        tables.clear_candidate_standings().await?;
        tables.clear_catalog_apply_steps().await?;
        tables.clear_release_edit_sessions().await?;
        tables.clear_held_folder_scans().await?;
        let runtime = CandidateRuntime::new(tables.clone(), diagnostics.clone());
        let event_tx = crate::ImportEventBus::new(runtime.clone());
        let services = crate::ImportServices::new(
            event_tx,
            store,
            records,
            catalogs,
            remote_images,
            tables.clone(),
            clock,
            ids,
            files,
            file_tags,
            diagnostics.clone(),
            closing,
        );
        Ok(Self::start_with(runtime_handle, services, runtime, diagnostics))
    }

    fn start_with(
        runtime_handle: tokio::runtime::Handle,
        services: crate::ImportServices,
        runtime: CandidateRuntime,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> ImportServiceHandle {
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<WatchReport>();
        let event_tx_for_worker = services.event_tx.clone();
        let runtime_for_worker = runtime.clone();
        let records = services.records.clone();
        let tables = services.tables.clone();
        let clock = services.clock.clone();
        let ids = services.ids.clone();
        let files = services.files.clone();
        let import_cancels = services.import_cancels.clone();
        let catalogs = services.catalogs.clone();
        let remote_images = services.remote_images.clone();

        // Constructed before the watcher task spawns; the task doesn't need the
        // watcher, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx, diagnostics.clone()));

        let watched = services.clone();
        let bugs = diagnostics.bug_sink();
        #[expect(
            clippy::disallowed_methods,
            reason = "the import service is a composition root: it owns this thread and stops it"
        )]
        let watcher =
            WorkerThread::spawn("folder scan coordinator", bugs.clone(), move |_, watcher_rx| {
                run_watcher(watcher_rx, fs_rx, watched, folder_watcher);
            });

        let release_reads =
            crate::ReleaseReads::new(services.tables.clone(), services.catalogs.clone());
        let release_reads_for_worker = release_reads.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "the import service is a composition root: it owns this thread and stops it"
        )]
        let worker = WorkerThread::spawn("import worker thread", bugs, move |_, commands_rx| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create import runtime");
            rt.block_on(async move {
                let service = ImportService {
                    event_tx: event_tx_for_worker,
                    runtime: runtime_for_worker,
                    records,
                    tables,
                    catalogs,
                    release_reads: release_reads_for_worker,
                    remote_images,
                    clock,
                    ids,
                    files,
                    import_cancels,
                    diagnostics,
                };

                while let Some(message) = commands_rx.recv().await {
                    match message {
                        ImportWorkerMessage::Import { command, expectation } => {
                            service.do_import(command, expectation).await
                        }
                        ImportWorkerMessage::Shutdown => break,
                        #[cfg(test)]
                        ImportWorkerMessage::Panic => {
                            bae_diagnostics::test_panic::panics("the import worker broke")
                        }
                    }
                }
            });
        });

        let identify = crate::IdentifyService::new(
            services.store.clone(),
            services.tables.clone(),
            services.catalogs.clone(),
            release_reads.clone(),
            runtime_handle.clone(),
            services.event_tx.clone(),
            runtime.clone(),
            services.diagnostics.clone(),
        );
        let extraction = crate::extraction::ExtractionService::new(
            runtime_handle.clone(),
            services.event_tx.clone(),
            runtime.clone(),
            services.store.clone(),
            services.tables.clone(),
            services.records.clone(),
            services.files.clone(),
            services.diagnostics.clone(),
        );

        ImportServiceHandle::new(
            worker,
            watcher,
            services,
            runtime,
            release_reads,
            identify,
            extraction,
            runtime_handle,
        )
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
