use super::*;
use tokio::sync::broadcast;

/// Central event bus for UI events. Clone-cheap (Arc internally via broadcast).
/// Subscribes to existing service channels and translates domain events into
/// UiBusEvents. The bridge subscribes to receive events for the native reducer.
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

    pub fn subscribe(&self) -> broadcast::Receiver<UiBusEvent> {
        self.tx.subscribe()
    }

    /// Wire the bus to all service channels. Spawns async tasks that
    /// translate domain events into UiBusEvents. Call once at startup.
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
        let lm = app_services.library_manager().clone();
        self.wire_config_changes(
            app_services.library_manager().subscribe_config_changes(),
            move || lm.is_sync_ready(),
            runtime_handle,
        );
    }

    /// Forward the reactive config-state stream to the bus. `ConfigHandle`
    /// publishes the whole latest `Config` on every change (Discogs key
    /// stored/validated, cloud provider, library rename, …);
    /// each becomes a `ConfigChanged` the native reducer applies wholesale. The
    /// `watch` channel coalesces to the most recent value, so the UI always sees
    /// the current config without polling or a restart.
    fn wire_config_changes(
        &self,
        mut config_rx: tokio::sync::watch::Receiver<crate::config::Config>,
        sync_ready: impl Fn() -> bool + Send + Sync + 'static,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let bus = self.clone();

        runtime_handle.spawn(async move {
            loop {
                if config_rx.changed().await.is_err() {
                    tracing::debug!("config watch closed; stopping config→UI forwarder");
                    break;
                }
                let config = config_rx.borrow_and_update().clone();
                bus.emit(UiBusEvent::ConfigChanged {
                    config,
                    sync_ready: sync_ready(),
                });
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
                            } => UiBusEvent::PlaybackPaused {
                                track_id: track_info.track_id.clone(),
                                track_title: track_info.track_title.clone(),
                                artist_names: track_info.artist_names.clone(),
                                artist_id: track_info.artist_id.clone(),
                                album_id: track_info.album_id.clone(),
                                album_title: track_info.album_title.clone(),
                                cover_image_id: track_info.cover_image_id.clone(),
                                duration_ms: *duration_ms,
                            },
                        };
                        bus.emit(bus_event);
                    }
                    PlaybackProgress::PositionUpdate {
                        position_ms,
                        duration_ms,
                        progress,
                        ..
                    } => {
                        bus.emit(UiBusEvent::PlaybackProgress {
                            position_ms,
                            duration_ms,
                            progress,
                        });
                    }
                    PlaybackProgress::Seeked {
                        position_ms,
                        duration_ms,
                        progress,
                        ..
                    } => {
                        bus.emit(UiBusEvent::PlaybackProgress {
                            position_ms,
                            duration_ms,
                            progress,
                        });
                    }
                    PlaybackProgress::QueueUpdated {
                        tracks,
                        has_next,
                        has_previous,
                    } => {
                        // Resolve the queue's track ids to display-ready items in
                        // core so the event payload is fully populated. Consumers
                        // then map it directly instead of each re-querying the DB
                        // (the macOS reducer used to; Windows didn't, so its queue
                        // rows rendered blank).
                        match lm.get_queue_items(&tracks).await {
                            Ok(items) => {
                                bus.emit(UiBusEvent::QueueUpdated {
                                    items,
                                    has_next,
                                    has_previous,
                                });
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "skipping QueueUpdated: failed to resolve {} queue item(s): {e}",
                                    tracks.len()
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
                            crate::playback::PreviewState::Playing {
                                path,
                                duration_ms,
                            } => UiBusEvent::PreviewPlaying {
                                path: path.clone(),
                                duration_ms: *duration_ms,
                            },
                            crate::playback::PreviewState::Paused {
                                path,
                                duration_ms,
                            } => UiBusEvent::PreviewPaused {
                                path: path.clone(),
                                duration_ms: *duration_ms,
                            },
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

    /// Wire to the unified import event channel. Handles all ImportEvent variants:
    /// scan events, import progress, identify, search, prefetch, and errors.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let mut rx = app_services.import().subscribe_events();
        let bus = self.clone();

        runtime_handle.spawn(async move {
            use crate::import::{ImportEvent, ImportProgress, ScanEvent};

            loop {
                match rx.recv().await {
                    Ok(ImportEvent::Scan(scan_event)) => match scan_event {
                        ScanEvent::WatchedFoldersChanged { folders } => {
                            bus.emit(UiBusEvent::WatchedFoldersChanged { folders });
                        }
                        ScanEvent::FolderCandidate(c) => {
                            bus.emit(UiBusEvent::FolderCandidateAdded { candidate: c });
                        }
                        ScanEvent::InvalidCandidate(c) => {
                            bus.emit(UiBusEvent::InvalidCandidate { candidate: c });
                        }
                        ScanEvent::CandidateRemoved { candidate_key } => {
                            bus.emit(UiBusEvent::ScanCandidateRemoved { key: candidate_key });
                        }
                        ScanEvent::CandidateSkipChanged {
                            candidate_key,
                            skipped,
                        } => {
                            bus.emit(UiBusEvent::CandidateSkipChanged {
                                key: candidate_key,
                                skipped,
                            });
                        }
                        ScanEvent::Finished => {
                            bus.emit(UiBusEvent::ScanFinished);
                        }
                    },
                    Ok(ImportEvent::ImportProgress {
                        candidate_key,
                        progress,
                    }) => {
                        let bus_event = match progress {
                            ImportProgress::Preparing { step, .. } => {
                                Some(UiBusEvent::CandidateImportImporting {
                                    key: candidate_key.clone(),
                                    progress_percent: 0,
                                    step: Some(crate::import::ImportStep::Preparing(step)),
                                })
                            }
                            ImportProgress::Started { .. } => {
                                Some(UiBusEvent::CandidateImportImporting {
                                    key: candidate_key.clone(),
                                    progress_percent: 0,
                                    step: None,
                                })
                            }
                            ImportProgress::Progress { percent, phase, .. } => {
                                Some(UiBusEvent::CandidateImportImporting {
                                    key: candidate_key.clone(),
                                    progress_percent: percent as u32,
                                    step: phase.map(crate::import::ImportStep::Running),
                                })
                            }
                            ImportProgress::Complete { id, album_id, .. } => {
                                Some(UiBusEvent::CandidateImportComplete {
                                    key: candidate_key.clone(),
                                    release_id: id,
                                    album_id,
                                })
                            }
                            ImportProgress::Failed { error, .. } => {
                                // The import pipeline flattens every failure
                                // (scan, metadata, encryption, DB) to a string;
                                // the UI shows a generic Import line plus this as
                                // copyable, log-only detail.
                                Some(UiBusEvent::CandidateImportError {
                                    key: candidate_key.clone(),
                                    error: crate::ui::UiError::import(error),
                                })
                            }
                        };

                        if let Some(event) = bus_event {
                            bus.emit(event);
                        }
                    }
                    #[cfg(not(any(target_os = "ios", target_os = "android")))]
                    Ok(ImportEvent::IdentifyStateChanged {
                        candidate_key,
                        state,
                        toolbar,
                    }) => {
                        bus.emit(UiBusEvent::CandidateIdentifyStateChanged {
                            key: candidate_key,
                            state,
                            toolbar,
                        });
                    }
                    #[cfg(not(any(target_os = "ios", target_os = "android")))]
                    Ok(ImportEvent::SignalsUpdated {
                        candidate_key,
                        signals,
                    }) => {
                        bus.emit(UiBusEvent::CandidateSignalsUpdated {
                            key: candidate_key,
                            signals,
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Import event bus lagged by {n} events");
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
        let mut rx = app_services.library_manager().subscribe_events();
        let bus = self.clone();

        runtime_handle.spawn(async move {
            use crate::library::LibraryEvent;
            loop {
                match rx.recv().await {
                    Ok(LibraryEvent::AlbumAdded { album }) => {
                        tracing::info!("wire_library: AlbumAdded for album {}", album.album.id);
                        bus.emit(UiBusEvent::AlbumAdded { album });
                    }
                    Ok(LibraryEvent::AlbumUpdated { album }) => {
                        bus.emit(UiBusEvent::AlbumUpdated { album });
                    }
                    Ok(LibraryEvent::AlbumRemoved {
                        album_id,
                        release_ids,
                    }) => {
                        bus.emit(UiBusEvent::AlbumRemoved {
                            album_id,
                            release_ids,
                        });
                    }
                    Ok(LibraryEvent::ReleaseAdded { album, release }) => {
                        bus.emit(UiBusEvent::ReleaseAdded { album, release });
                    }
                    Ok(LibraryEvent::ReleaseUpdated { album_id, release }) => {
                        bus.emit(UiBusEvent::ReleaseUpdated { album_id, release });
                    }
                    Ok(LibraryEvent::ReleaseRemoved {
                        album_id,
                        release_id,
                        album,
                    }) => {
                        bus.emit(UiBusEvent::ReleaseRemoved {
                            album_id,
                            release_id,
                            album,
                        });
                    }
                    Ok(LibraryEvent::TracksDeleted { .. }) => {
                        // Handled by playback service directly, not the UI bus
                    }
                    Ok(LibraryEvent::Error { error }) => {
                        bus.emit(UiBusEvent::Error { error });
                    }
                    Ok(LibraryEvent::SyncError { error }) => {
                        bus.emit(UiBusEvent::SyncError { error });
                    }
                    Ok(LibraryEvent::SyncTimeChanged { time }) => {
                        bus.emit(UiBusEvent::SyncTimeChanged { time });
                    }
                    Ok(LibraryEvent::SyncingChanged { syncing }) => {
                        bus.emit(UiBusEvent::SyncingChanged { syncing });
                    }
                    Ok(LibraryEvent::OutboxChanged { snapshot }) => {
                        bus.emit(UiBusEvent::OutboxChanged { snapshot });
                    }
                    Ok(LibraryEvent::ReleaseTransferProgress {
                        release_id,
                        action,
                        file_no,
                        total,
                        percent,
                    }) => {
                        bus.emit(UiBusEvent::ReleaseTransferProgress {
                            release_id,
                            action,
                            file_no,
                            total,
                            percent,
                        });
                    }
                    Ok(LibraryEvent::ReleaseTransferEnded { release_id }) => {
                        bus.emit(UiBusEvent::ReleaseTransferEnded { release_id });
                    }
                    Ok(LibraryEvent::DownloadQueueChanged { snapshot }) => {
                        bus.emit(UiBusEvent::DownloadQueueChanged { snapshot });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Library event bus lagged by {n} events");
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
    use crate::library_dir::LibraryDir;
    use std::time::Duration;
    use tempfile::TempDir;

    /// A config change published by the handle is forwarded to the bus as a
    /// `ConfigChanged` event carrying the whole new config, so the UI reacts
    /// without polling or a restart.
    #[test]
    fn config_change_is_forwarded_to_the_ui_bus() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let config = Config::with_defaults(
            "lib-1".to_string(),
            "device-1".to_string(),
            LibraryDir::new(tmp.path().join("lib")),
            "Test Library".to_string(),
        );
        config.save_to_config_yaml().expect("save config.yaml");
        let handle = ConfigHandle::new(config);

        let bus = UiEventBus::new();
        bus.wire_config_changes(handle.subscribe(), || false, runtime.handle());
        let mut rx = bus.subscribe();

        handle
            .update(|c| c.discogs = Some(crate::config::DiscogsValidation::Valid))
            .unwrap();

        let config = runtime.block_on(async {
            let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out waiting for ConfigChanged")
                .expect("event bus closed");
            match event {
                UiBusEvent::ConfigChanged { config, .. } => config,
                other => panic!("expected ConfigChanged, got {other:?}"),
            }
        });

        assert_eq!(
            config.discogs,
            Some(crate::config::DiscogsValidation::Valid)
        );
    }
}
