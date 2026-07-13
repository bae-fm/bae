use super::*;
use tokio::sync::broadcast;

/// Central event bus for UI events: subscribes to the service channels and
/// translates their domain events into `UiBusEvent`s, which the bridge forwards
/// to the native reducer. Clone-cheap.
#[derive(Clone)]
pub struct UiEventBus {
    tx: broadcast::Sender<UiBusEvent>,
}

impl UiEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { tx }
    }

    pub fn emit(&self, event: UiBusEvent) {
        let _ = self.tx.send(event);
    }

    fn invalidate(&self, invalidation: Invalidation) {
        self.emit(UiBusEvent::Invalidated(invalidation));
    }

    fn invalidate_library_lag(&self) {
        for invalidation in [
            Invalidation::AlbumList,
            Invalidation::ComposerList,
            Invalidation::ArtistList,
            Invalidation::SyncStatus,
            Invalidation::Outbox,
            Invalidation::DownloadQueue,
            Invalidation::ExportQueue,
        ] {
            self.invalidate(invalidation);
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn invalidate_import_lag(&self) {
        self.invalidate(Invalidation::WatchedFolders);
        self.invalidate(Invalidation::ImportCandidateList);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UiBusEvent> {
        self.tx.subscribe()
    }

    /// Wire the bus to every service channel, spawning one forwarding task per
    /// channel. Call once at startup.
    pub fn wire(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        self.wire_playback(app_services, runtime_handle);
        self.wire_library(app_services, runtime_handle);
        // Import/scan/identify events come from the desktop-only import service.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.wire_import(app_services, runtime_handle);
        self.wire_config_changes(
            app_services.library_manager().subscribe_config_changes(),
            runtime_handle,
        );
    }

    /// Forward config changes to the bus as a scoped invalidation — the UI reads
    /// the new value back through the config query, not off the event.
    fn wire_config_changes(
        &self,
        mut config_rx: tokio::sync::watch::Receiver<crate::config::Config>,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let bus = self.clone();

        runtime_handle.spawn(async move {
            loop {
                if config_rx.changed().await.is_err() {
                    tracing::debug!("config watch closed; stopping config→UI forwarder");
                    break;
                }
                config_rx.borrow_and_update();
                bus.invalidate(Invalidation::Config);
                bus.invalidate(Invalidation::SyncStatus);
            }
        });
    }

    fn wire_playback(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let mut rx = app_services.playback().subscribe_progress();
        let bus = self.clone();
        let lm = app_services.library_manager().clone();

        runtime_handle.spawn(async move {
            use crate::playback::PlaybackProgress;

            while let Some(event) = rx.recv().await {
                match event {
                    PlaybackProgress::StateChanged { state } => {
                        let bus_event = match &state {
                            crate::playback::PlaybackState::Stopped => UiBusEvent::PlaybackStopped,
                            crate::playback::PlaybackState::Loading { track_id, resolved } => {
                                UiBusEvent::PlaybackLoading {
                                    track_id: track_id.clone(),
                                    track: resolved.clone(),
                                }
                            }
                            crate::playback::PlaybackState::Playing {
                                track_info,
                                duration_ms,
                            } => UiBusEvent::PlaybackPlaying {
                                track_id: track_info.track_id.clone(),
                                track_title: track_info.track_title.clone(),
                                artist_names: track_info.artist_names.clone(),
                                artist_id: track_info.artist_id.clone(),
                                album_id: track_info.album_id.clone(),
                                album_title: track_info.album_title.clone(),
                                cover_image_id: track_info.cover_image_id.clone(),
                                duration_ms: *duration_ms,
                            },
                            crate::playback::PlaybackState::Paused {
                                track_info,
                                duration_ms,
                                reason,
                            } => UiBusEvent::PlaybackPaused {
                                track_id: track_info.track_id.clone(),
                                track_title: track_info.track_title.clone(),
                                artist_names: track_info.artist_names.clone(),
                                artist_id: track_info.artist_id.clone(),
                                album_id: track_info.album_id.clone(),
                                album_title: track_info.album_title.clone(),
                                cover_image_id: track_info.cover_image_id.clone(),
                                duration_ms: *duration_ms,
                                reason: reason.clone(),
                            },
                        };
                        bus.emit(bus_event);
                    }
                    PlaybackProgress::PositionUpdate {
                        track_id,
                        position_ms,
                        duration_ms,
                        progress,
                    } => {
                        bus.emit(UiBusEvent::PlaybackProgress {
                            track_id,
                            position_ms,
                            duration_ms,
                            progress,
                        });
                    }
                    PlaybackProgress::Seeked {
                        track_id,
                        position_ms,
                        duration_ms,
                        progress,
                    } => {
                        bus.emit(UiBusEvent::PlaybackSeeked {
                            track_id,
                            position_ms,
                            duration_ms,
                            progress,
                        });
                    }
                    PlaybackProgress::QueueUpdated(projection) => {
                        let entry_ids: Vec<_> = projection
                            .manual
                            .iter()
                            .chain(
                                projection
                                    .context
                                    .iter()
                                    .flat_map(|context| context.upcoming.iter()),
                            )
                            .map(|entry| entry.id.0.clone())
                            .collect();
                        match lm.resolve_queue_projection(projection).await {
                            Ok(snapshot) => {
                                bus.emit(UiBusEvent::QueueUpdated(snapshot));
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "skipping QueueUpdated for entries {:?}: failed to resolve queue projection: {e}",
                                    entry_ids
                                );
                            }
                        }
                    }
                    PlaybackProgress::QueueItemsAdded { count } => {
                        bus.emit(UiBusEvent::QueueItemsAdded { count });
                    }
                    PlaybackProgress::VolumeChanged { volume } => {
                        bus.emit(UiBusEvent::VolumeChanged { volume });
                    }
                    PlaybackProgress::MuteChanged { is_muted } => {
                        bus.emit(UiBusEvent::MuteChanged { is_muted });
                    }
                    PlaybackProgress::RepeatModeChanged { mode } => {
                        bus.emit(UiBusEvent::RepeatModeChanged { mode });
                    }
                    PlaybackProgress::PreviewStateChanged(state) => {
                        let bus_event = match &state {
                            crate::playback::PreviewState::Idle => UiBusEvent::PreviewIdle,
                            crate::playback::PreviewState::Playing { path, duration_ms } => {
                                UiBusEvent::PreviewPlaying {
                                    path: path.clone(),
                                    duration_ms: *duration_ms,
                                }
                            }
                            crate::playback::PreviewState::Paused { path, duration_ms } => {
                                UiBusEvent::PreviewPaused {
                                    path: path.clone(),
                                    duration_ms: *duration_ms,
                                }
                            }
                        };
                        bus.emit(bus_event);
                    }
                    PlaybackProgress::PreviewPositionUpdate {
                        position_ms,
                        progress,
                    } => {
                        bus.emit(UiBusEvent::PreviewProgress {
                            position_ms,
                            progress,
                        });
                    }
                    PlaybackProgress::PlaybackError { reason } => {
                        bus.emit(UiBusEvent::PlaybackError { reason });
                    }
                    _ => {}
                }
            }
        });
    }

    /// Wire to the import event channel: scan events, import and loudness
    /// progress, identify-state changes, and extracted signals.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let rx = app_services.import().subscribe_events();
        let diagnostics = app_services.library_manager().diagnostics().clone();
        self.wire_import_events(
            rx,
            app_services.import().clone(),
            diagnostics,
            runtime_handle,
        );
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import_events(
        &self,
        mut rx: broadcast::Receiver<crate::import::ImportEvent>,
        import: crate::import::ImportServiceHandle,
        diagnostics: crate::diagnostics::Diagnostics,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let bus = self.clone();

        runtime_handle.spawn(async move {
            use crate::import::{ImportEvent, ImportProgress, ScanEvent};

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        import.record_candidate_event(&event);
                        match event {
                            ImportEvent::Scan(scan_event) => match scan_event {
                                ScanEvent::WatchedFoldersChanged { .. } => {
                                    bus.invalidate(Invalidation::WatchedFolders);
                                }
                                ScanEvent::FolderCandidate(c) => {
                                    bus.invalidate(Invalidation::ImportCandidate {
                                        key: c.path.to_string_lossy().to_string(),
                                    });
                                    bus.invalidate(Invalidation::ImportCandidateList);
                                }
                                ScanEvent::InvalidCandidate(c) => {
                                    bus.invalidate(Invalidation::ImportCandidate {
                                        key: c.path.to_string_lossy().to_string(),
                                    });
                                    bus.invalidate(Invalidation::ImportCandidateList);
                                }
                                ScanEvent::CandidateRemoved { candidate_key }
                                | ScanEvent::CandidateSkipChanged { candidate_key, .. } => {
                                    bus.invalidate(Invalidation::ImportCandidate {
                                        key: candidate_key.clone(),
                                    });
                                    bus.invalidate(Invalidation::ImportCandidateList);
                                }
                                ScanEvent::Failed { error } => {
                                    bus.emit(UiBusEvent::Error {
                                        error: crate::ui::UiError::import(error),
                                    });
                                }
                                ScanEvent::Finished => {
                                    bus.invalidate(Invalidation::ImportCandidateList);
                                }
                            },
                            ImportEvent::ImportProgress {
                                candidate_key,
                                progress,
                            } => {
                                match progress {
                                    ImportProgress::Preparing { .. }
                                    | ImportProgress::Started { .. }
                                    | ImportProgress::Progress { .. }
                                    | ImportProgress::Failed { .. } => {
                                        bus.invalidate(Invalidation::ImportCandidate {
                                            key: candidate_key.clone(),
                                        });
                                    }
                                    ImportProgress::Complete { .. }
                                    | ImportProgress::RemoteUploadQueued { .. } => {
                                        bus.invalidate(Invalidation::ImportCandidate {
                                            key: candidate_key.clone(),
                                        });
                                        bus.invalidate(Invalidation::ImportCandidateList);
                                    }
                                };
                            }
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            ImportEvent::ImportLoudnessProgress {
                                candidate_key,
                                tracks_done,
                                tracks_total,
                                fraction,
                            } => {
                                bus.emit(UiBusEvent::CandidateImportLoudnessProgress {
                                    key: candidate_key,
                                    tracks_done,
                                    tracks_total,
                                    fraction,
                                });
                            }
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            ImportEvent::IdentifyStateChanged { candidate_key, .. } => {
                                bus.invalidate(Invalidation::ImportCandidate {
                                    key: candidate_key.clone(),
                                });
                            }
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            ImportEvent::SignalsUpdated { candidate_key, .. } => {
                                bus.invalidate(Invalidation::ImportCandidate {
                                    key: candidate_key.clone(),
                                });
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Import event bus lagged by {n} events");
                        diagnostics.event(crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        });
                        bus.invalidate_import_lag();
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    fn wire_library(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let rx = app_services.library_manager().subscribe_events();
        let diagnostics = app_services.library_manager().diagnostics().clone();
        self.wire_library_events(rx, diagnostics, runtime_handle);
    }

    fn wire_library_events(
        &self,
        mut rx: broadcast::Receiver<crate::library::LibraryEvent>,
        diagnostics: crate::diagnostics::Diagnostics,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let bus = self.clone();

        runtime_handle.spawn(async move {
            use crate::library::LibraryEvent;
            loop {
                match rx.recv().await {
                    Ok(LibraryEvent::AlbumAdded { album }) => {
                        let album_id = album.album.id.clone();
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album { album_id });
                    }
                    Ok(LibraryEvent::AlbumUpdated { album }) => {
                        let album_id = album.album.id.clone();
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album { album_id });
                    }
                    Ok(LibraryEvent::AlbumRemoved {
                        album_id,
                        release_ids,
                    }) => {
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album {
                            album_id: album_id.clone(),
                        });
                        for release_id in &release_ids {
                            bus.invalidate(Invalidation::Release {
                                release_id: release_id.clone(),
                            });
                        }
                    }
                    Ok(LibraryEvent::ReleaseAdded { album, release }) => {
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album {
                            album_id: album.id.clone(),
                        });
                        bus.invalidate(Invalidation::Release {
                            release_id: release.summary.id.clone(),
                        });
                    }
                    Ok(LibraryEvent::ReleaseUpdated { album_id, release }) => {
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album {
                            album_id: album_id.clone(),
                        });
                        bus.invalidate(Invalidation::Release {
                            release_id: release.summary.id.clone(),
                        });
                    }
                    Ok(LibraryEvent::ReleaseRemoved {
                        album_id,
                        release_id,
                        ..
                    }) => {
                        bus.invalidate(Invalidation::AlbumList);
                        bus.invalidate(Invalidation::ComposerList);
                        bus.invalidate(Invalidation::ArtistList);
                        bus.invalidate(Invalidation::Album {
                            album_id: album_id.clone(),
                        });
                        bus.invalidate(Invalidation::Release {
                            release_id: release_id.clone(),
                        });
                    }
                    Ok(LibraryEvent::TracksDeleted { .. }) => {
                        // Handled by playback service directly, not the UI bus
                    }
                    Ok(LibraryEvent::Error { error }) => {
                        bus.emit(UiBusEvent::Error { error });
                    }
                    Ok(LibraryEvent::SyncError { .. })
                    | Ok(LibraryEvent::SyncTimeChanged { .. })
                    | Ok(LibraryEvent::SyncingChanged { .. }) => {
                        bus.invalidate(Invalidation::SyncStatus);
                    }
                    Ok(LibraryEvent::OutboxChanged { .. }) => {
                        bus.invalidate(Invalidation::Outbox);
                    }
                    Ok(LibraryEvent::ReleaseTransferProgress { release_id, action }) => {
                        bus.invalidate(Invalidation::Release {
                            release_id: release_id.clone(),
                        });
                        bus.emit(UiBusEvent::ReleaseTransferProgress { release_id, action });
                    }
                    Ok(LibraryEvent::ReleaseTransferEnded { release_id }) => {
                        bus.invalidate(Invalidation::Release {
                            release_id: release_id.clone(),
                        });
                        bus.emit(UiBusEvent::ReleaseTransferEnded { release_id });
                    }
                    Ok(LibraryEvent::DownloadQueueChanged { .. }) => {
                        bus.invalidate(Invalidation::DownloadQueue);
                    }
                    Ok(LibraryEvent::ExportQueueChanged { .. }) => {
                        bus.invalidate(Invalidation::ExportQueue);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Library event bus lagged by {n} events");
                        diagnostics.event(crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        });
                        bus.invalidate_library_lag();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigHandle};
    use crate::db::Database;
    use crate::library::LibraryEvent;
    use coven::StoreDir;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    fn recv_ui_event(
        runtime: &tokio::runtime::Runtime,
        rx: &mut broadcast::Receiver<UiBusEvent>,
    ) -> UiBusEvent {
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out waiting for UI event")
                .expect("event bus closed")
        })
    }

    fn make_test_library_manager(
        runtime: &tokio::runtime::Runtime,
    ) -> (crate::library::LibraryManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let library_dir = StoreDir::new(tmp.path().join("lib"));
        let database = runtime
            .block_on(Database::new_test(
                tmp.path().join("test.db").to_str().unwrap(),
                Arc::new(coven::SystemClock),
                std::sync::Arc::new(coven::UuidProvider),
            ))
            .unwrap();
        let library_id = format!("test-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        let manager = crate::library::LibraryManager::new(
            database,
            Arc::new(ConfigHandle::new(config)),
            crate::keys::StoreKeys::new(library_id),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            runtime.handle().clone(),
        );
        (manager, tmp)
    }

    /// A config change published by the handle is forwarded to the bus as a
    /// scoped invalidation so projections can re-read the authoritative value.
    #[test]
    fn config_change_invalidates_and_forwards_to_the_ui_bus() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let config = Config::with_defaults(
            "lib-1".to_string(),
            "device-1".to_string(),
            StoreDir::new(tmp.path().join("lib")),
            "Test Library".to_string(),
        );
        config.save_to_config_yaml().expect("save config.yaml");
        let handle = ConfigHandle::new(config);

        let bus = UiEventBus::new();
        bus.wire_config_changes(handle.subscribe(), runtime.handle());
        let mut rx = bus.subscribe();

        handle
            .update(|c| c.discogs = Some(crate::config::DiscogsValidation::Valid))
            .unwrap();

        match recv_ui_event(&runtime, &mut rx) {
            UiBusEvent::Invalidated(Invalidation::Config) => {}
            other => panic!("expected config invalidation, got {other:?}"),
        }

        match recv_ui_event(&runtime, &mut rx) {
            UiBusEvent::Invalidated(Invalidation::SyncStatus) => {}
            other => panic!("expected sync-status invalidation, got {other:?}"),
        }
    }

    #[test]
    fn library_lag_emits_coarse_invalidations() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bus = UiEventBus::new();
        let mut ui_rx = bus.subscribe();
        let (library_tx, library_rx) = broadcast::channel(1);

        // Overflow the capacity-1 channel before wiring the consumer, so the
        // receiver is already lagged when the forward task takes its first
        // recv. Wiring first would race: a promptly-scheduled task can drain
        // the first send before the second lands, and no lag ever happens.
        library_tx
            .send(LibraryEvent::TracksDeleted {
                track_ids: vec!["track-1".to_string()],
            })
            .unwrap();
        library_tx
            .send(LibraryEvent::TracksDeleted {
                track_ids: vec!["track-2".to_string()],
            })
            .unwrap();
        drop(library_tx);

        bus.wire_library_events(
            library_rx,
            crate::diagnostics::Diagnostics::noop(),
            runtime.handle(),
        );

        let expected = [
            Invalidation::AlbumList,
            Invalidation::ComposerList,
            Invalidation::ArtistList,
            Invalidation::SyncStatus,
            Invalidation::Outbox,
            Invalidation::DownloadQueue,
            Invalidation::ExportQueue,
        ];

        for invalidation in expected {
            match recv_ui_event(&runtime, &mut ui_rx) {
                UiBusEvent::Invalidated(actual) => assert_eq!(actual, invalidation),
                other => panic!("expected invalidation, got {other:?}"),
            }
        }
    }

    #[test]
    fn library_event_emits_scoped_invalidation() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bus = UiEventBus::new();
        let mut ui_rx = bus.subscribe();
        let (library_tx, library_rx) = broadcast::channel(16);
        bus.wire_library_events(
            library_rx,
            crate::diagnostics::Diagnostics::noop(),
            runtime.handle(),
        );

        library_tx
            .send(LibraryEvent::SyncingChanged { syncing: true })
            .unwrap();

        match recv_ui_event(&runtime, &mut ui_rx) {
            UiBusEvent::Invalidated(Invalidation::SyncStatus) => {}
            other => panic!("expected sync-status invalidation, got {other:?}"),
        }
    }

    #[test]
    fn release_removal_invalidates_scoped_keys() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bus = UiEventBus::new();
        let mut ui_rx = bus.subscribe();
        let (library_tx, library_rx) = broadcast::channel(16);
        bus.wire_library_events(
            library_rx,
            crate::diagnostics::Diagnostics::noop(),
            runtime.handle(),
        );

        library_tx
            .send(LibraryEvent::ReleaseRemoved {
                album_id: "album-1".to_string(),
                release_id: "release-1".to_string(),
                album: None,
            })
            .unwrap();

        let expected = [
            Invalidation::AlbumList,
            Invalidation::ComposerList,
            Invalidation::ArtistList,
            Invalidation::Album {
                album_id: "album-1".to_string(),
            },
            Invalidation::Release {
                release_id: "release-1".to_string(),
            },
        ];

        for invalidation in expected {
            match recv_ui_event(&runtime, &mut ui_rx) {
                UiBusEvent::Invalidated(actual) => assert_eq!(actual, invalidation),
                other => panic!("expected invalidation, got {other:?}"),
            }
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[test]
    fn import_lag_emits_coarse_invalidations() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bus = UiEventBus::new();
        let mut ui_rx = bus.subscribe();
        let (library_manager, _tmp) = make_test_library_manager(&runtime);
        let import = crate::import::ImportService::start(
            runtime.handle().clone(),
            library_manager,
            crate::import::cover_art::CoverArtArchiveClient::hermetic(),
        );
        let (import_tx, import_rx) = broadcast::channel(1);

        import_tx
            .send(crate::import::ImportEvent::Scan(
                crate::import::ScanEvent::Finished,
            ))
            .unwrap();
        import_tx
            .send(crate::import::ImportEvent::Scan(
                crate::import::ScanEvent::Finished,
            ))
            .unwrap();
        drop(import_tx);
        bus.wire_import_events(
            import_rx,
            import,
            crate::diagnostics::Diagnostics::noop(),
            runtime.handle(),
        );

        match recv_ui_event(&runtime, &mut ui_rx) {
            UiBusEvent::Invalidated(Invalidation::WatchedFolders) => {}
            other => panic!("expected watched-folders invalidation, got {other:?}"),
        }
        match recv_ui_event(&runtime, &mut ui_rx) {
            UiBusEvent::Invalidated(Invalidation::ImportCandidateList) => {}
            other => panic!("expected import-candidate-list invalidation, got {other:?}"),
        }
    }
}
