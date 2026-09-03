use bae_core::album_detail::{
    ComposerDetail, ComposerSummary, ComposerWorkGroup, WorkDetail, WorkReleaseSummary, WorkSummary,
};
use bae_core::db::{DbArtist, DbComposerSummary, DbWork, DbWorkSummary};
use std::sync::{Arc, Mutex};

#[test]
#[cfg(feature = "desktop")]
fn first_unidentified_position_crosses_the_bridge() {
    let bridge = crate::types::BridgeFirstUnidentifiedRowRef::from_core(
        bae_core::import::FirstUnidentifiedRowRef {
            candidate_key: "/library/release".to_string(),
            stable_key: "candidate:/library/release".to_string(),
            group_key: Some(bae_core::import::FolderReleaseDecisionKey {
                watched_folder_path: "/library".to_string(),
                relative_folder_path: "group".to_string(),
            }),
            visible_position: Some(61),
        },
    );

    assert_eq!(bridge.candidate_key, "/library/release");
    assert_eq!(bridge.stable_key, "candidate:/library/release");
    assert_eq!(bridge.visible_position, Some(61));
    let group = bridge.group_key.expect("the target belongs to its group");
    assert_eq!(group.watched_folder_path, "/library");
    assert_eq!(group.relative_folder_path, "group");
}

/// Records every delivered event so tests can assert on the stream.
struct CollectingCallback {
    events: Arc<Mutex<Vec<crate::types::BridgeUiEvent>>>,
}

impl crate::types::UiEventCallback for CollectingCallback {
    fn on_event(&self, event: crate::types::BridgeUiEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn track_detail(
    position: bae_core::album_detail::TrackPosition,
    duration_ms: Option<i64>,
) -> bae_core::album_detail::TrackDetail {
    bae_core::album_detail::TrackDetail {
        id: "track-1".to_string(),
        title: "Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms,
        artist_names: "Artist".to_string(),
        display_artist: None,
        position_text: "1".to_string(),
        position,
    }
}

#[test]
fn track_group_precomputes_header_key_per_side() {
    use bae_core::album_detail::{TrackPosition, TrackSide};

    let cases = [
        (
            TrackSide::Sided {
                side_letter: "A".to_string(),
            },
            TrackPosition::Sided {
                side_letter: "A".to_string(),
                number: 1,
            },
            Some("core.track.side"),
        ),
        (
            TrackSide::Disc { disc: 2 },
            TrackPosition::Disc { disc: 2, number: 1 },
            Some("core.track.disc"),
        ),
        (TrackSide::Flat, TrackPosition::Flat { number: 1 }, None),
    ];

    for (side, position, expected) in cases {
        let group = crate::types::BridgeTrackGroup::from_core(bae_core::album_detail::TrackGroup {
            side,
            tracks: vec![track_detail(position, Some(187_000))],
            total_duration_ms: 187_000,
        });
        assert_eq!(group.header_key.as_deref(), expected);
        assert_eq!(
            group.total_duration,
            Some(crate::types::BridgeDurationUnits::MinutesOnly { minutes: 3 })
        );
    }
}

#[test]
fn track_precomputes_duration_clock() {
    use bae_core::album_detail::TrackPosition;

    // A present duration renders a clock while the raw number is retained.
    let present = crate::types::BridgeTrack::from_core(track_detail(
        TrackPosition::Flat { number: 1 },
        Some(187_000),
    ));
    assert_eq!(present.duration_ms, Some(187_000));
    let clock = present
        .duration_clock
        .expect("clock for a present duration");
    assert_eq!((clock.minutes, clock.seconds), (3, 7));

    // An absent duration has nothing to label.
    let absent =
        crate::types::BridgeTrack::from_core(track_detail(TrackPosition::Flat { number: 1 }, None));
    assert_eq!(absent.duration_ms, None);
    assert!(absent.duration_clock.is_none());
}

fn track_search_result(duration_ms: Option<i64>) -> bae_core::album_detail::TrackSearchResult {
    bae_core::album_detail::TrackSearchResult {
        id: "track-1".to_string(),
        title: "Track".to_string(),
        duration_ms,
        album_id: "album-1".to_string(),
        album_title: "Album".to_string(),
        artist_name: "Artist".to_string(),
        cover: None,
    }
}

#[test]
fn track_search_result_precomputes_duration_clock() {
    // A present duration renders a clock; the raw number does not cross.
    let present =
        crate::types::BridgeTrackSearchResult::from_core(track_search_result(Some(187_000)));
    let clock = present
        .duration_clock
        .expect("clock for a present duration");
    assert_eq!((clock.minutes, clock.seconds), (3, 7));

    // An absent duration has nothing to label.
    let absent = crate::types::BridgeTrackSearchResult::from_core(track_search_result(None));
    assert!(absent.duration_clock.is_none());
}

fn queue_item(duration_ms: Option<i64>) -> bae_core::queue::QueueItem {
    bae_core::queue::QueueItem {
        entry_id: "entry-1".to_string(),
        track_id: "track-1".to_string(),
        title: "Track".to_string(),
        artist_names: "Artist".to_string(),
        duration_ms,
        album_title: "Album".to_string(),
        cover_image: None,
    }
}

#[test]
fn queue_entry_precomputes_duration_clock() {
    // A present duration renders a clock; the raw number does not cross.
    let present = crate::types::BridgeQueueEntry::from_core(queue_item(Some(187_000)));
    let clock = present
        .duration_clock
        .expect("clock for a present duration");
    assert_eq!((clock.minutes, clock.seconds), (3, 7));

    // An absent duration has nothing to label.
    let absent = crate::types::BridgeQueueEntry::from_core(queue_item(None));
    assert!(absent.duration_clock.is_none());
}

/// A handle over a fresh, empty library, built the way the app builds one so
/// whatever this bridge build carries — the desktop services, the cast
/// controller — is behind it.
fn fresh_bridge_handle(test_name: &str) -> (Arc<super::AppHandle>, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("bae-bridge-{test_name}"));
    match std::fs::remove_dir_all(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove stale test library dir: {error}"),
    }
    let library_dir = coven::StoreDir::new(root.join("library"));
    std::fs::create_dir_all(&*library_dir).expect("create test library dir");
    bae_core::config::install_test_keyring();

    let library_id = format!("test-{test_name}");
    let config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        format!("device-{test_name}"),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    // One id source for the whole test app: the library owner mints every
    // database and domain id from this provider.
    let ids: coven::IdRef = Arc::new(coven::SequentialIdProvider::new(test_name));
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let config_handle = Arc::new(bae_core::config::ConfigHandle::new(config));
    let manager = bae_core::library::LibraryManager::open(
        config_handle,
        Arc::new(coven::SystemClock),
        ids,
        bae_core::diagnostics::Diagnostics::noop(),
        runtime.handle().clone(),
        None,
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("open test library");
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let services = runtime
        .block_on(bae_core::library::AppServices::for_test(manager))
        .expect("start test app services");
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let services = {
        let playback = manager.start_playback_service(runtime.handle().clone(), 50, true);
        bae_core::library::AppServices::new(manager, playback)
    };
    let handle = Arc::new(
        super::AppHandle::start(services, bae_core::ui::UiEventBus::new(), runtime)
            .expect("the bridge handle starts"),
    );

    (handle, root)
}

#[cfg(not(feature = "desktop"))]
#[test]
fn album_browse_subscription_pulls_count_before_any_windows() {
    let (handle, _root) = fresh_bridge_handle("album-browse-empty-windows");
    let subscription = handle.subscribe_album_browse(Vec::new());

    let snapshot = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("album browse snapshot");

    assert_eq!(snapshot.total_count, 0);
    assert!(snapshot.windows.is_empty());
    assert_eq!(snapshot.cause, crate::types::BridgeLiveQueryCause::Initial);
    assert_eq!(snapshot.request_revision, 0);

    subscription
        .set_windows(vec![crate::types::BridgeLibraryPageWindow {
            offset: 0,
            limit: 50,
        }])
        .expect("request first album window");
    let requested = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("requested album browse snapshot");
    assert_eq!(
        requested.cause,
        crate::types::BridgeLiveQueryCause::RequestChanged
    );
    assert_eq!(requested.request_revision, 1);
    assert_eq!(requested.windows.len(), 1);
    assert_eq!(requested.windows[0].window.offset, 0);
    assert_eq!(requested.windows[0].window.limit, 50);
    assert!(requested.windows[0].rows.is_empty());
}

#[cfg(not(feature = "desktop"))]
#[test]
fn cancelling_album_browse_finishes_pending_next() {
    let (handle, _root) = fresh_bridge_handle("album-browse-cancel");
    let subscription = handle.subscribe_album_browse(Vec::new());
    handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("initial album browse snapshot");

    let pending = handle.runtime.spawn({
        let subscription = subscription.clone();
        async move { subscription.next().await }
    });
    handle
        .runtime
        .block_on(subscription.clone().cancel())
        .expect("cancel album browse subscription");
    let result = handle.runtime.block_on(pending).expect("next task joins");

    assert!(matches!(result, Err(crate::types::BridgeError::Cancelled)));
    assert!(matches!(
        subscription.set_windows(Vec::new()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}

#[cfg(not(feature = "desktop"))]
#[test]
fn save_sync_config_survives_the_uniffi_worker_stack() {
    const CHILD: &str = "BAE_BRIDGE_SAVE_SYNC_CONFIG_STACK_CHILD";

    if std::env::var_os(CHILD).is_some() {
        let runtime_drop_panicked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let panic_flag = runtime_drop_panicked.clone();
        std::panic::set_hook(Box::new(move |_| {
            panic_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        let (handle, _root) = fresh_bridge_handle("save-sync-config-stack");
        let caller_runtime = tokio::runtime::Runtime::new().expect("create caller runtime");
        std::thread::Builder::new()
            .name("uniffi-worker".to_string())
            .stack_size(544 * 1024)
            .spawn(move || {
                caller_runtime
                    .block_on(handle.save_sync_config(crate::types::BridgeSaveSyncConfig {
                        bucket: "bucket".to_string(),
                        region: "us-east-1".to_string(),
                        endpoint: Some("http://127.0.0.1:1".to_string()),
                        key_prefix: None,
                        access_key: "access-key".to_string(),
                        secret_key: "secret-key".to_string(),
                        storage: crate::types::BridgeHomeStorage::Opaque,
                    }))
                    .expect_err("an unreachable endpoint must fail the probe");
            })
            .expect("start UniFFI-sized worker")
            .join()
            .expect("UniFFI-sized worker survives the sync configuration call");
        assert!(
            !runtime_drop_panicked.load(std::sync::atomic::Ordering::SeqCst),
            "releasing the last app handle on its runtime worker must not panic"
        );
        return;
    }

    let status = std::process::Command::new(
        std::env::current_exe().expect("locate bae-bridge test executable"),
    )
    .arg("save_sync_config_survives_the_uniffi_worker_stack")
    .arg("--nocapture")
    .env(CHILD, "1")
    .status()
    .expect("run sync configuration stack subprocess");
    assert!(
        status.success(),
        "sync configuration exhausted the UniFFI-sized worker stack: {status}"
    );
}

#[cfg(not(feature = "desktop"))]
#[test]
fn enqueue_export_missing_release_does_not_panic() {
    let (handle, root) = fresh_bridge_handle("enqueue-export-missing-release");
    let result = handle.runtime.block_on(async {
        Arc::clone(&handle)
            .enqueue_export(
                "missing-release".to_string(),
                root.join("exports").to_string_lossy().into_owned(),
            )
            .await
    });

    assert!(matches!(
        result,
        Err(crate::types::BridgeError::Diagnostic {
            category: crate::types::BridgeErrorCategory::Export,
            ..
        })
    ));
}

/// A consumer that falls behind the transient-event bus keeps receiving the
/// events behind the gap. Persistent values have independent subscriptions.
#[tokio::test]
async fn pump_ui_events_keeps_delivering_after_broadcast_lag() {
    let (tx, rx) = tokio::sync::broadcast::channel(1);

    // Two sends into a capacity-1 channel before the pump runs: the first
    // is overwritten, so the pump's first recv returns Lagged with the
    // second event still queued behind it.
    tx.send(bae_core::ui::UiBusEvent::QueueItemsAdded { count: 1 })
        .unwrap();
    tx.send(bae_core::ui::UiBusEvent::QueueItemsAdded { count: 2 })
        .unwrap();
    drop(tx);

    let events = Arc::new(Mutex::new(Vec::new()));
    let pump = tokio::spawn(super::pump_ui_events(
        rx,
        Box::new(CollectingCallback {
            events: events.clone(),
        }),
    ));
    pump.await.unwrap();

    let events = events.lock().unwrap();
    assert!(matches!(
        events.as_slice(),
        [crate::types::BridgeUiEvent::QueueItemsAdded { count: 2 }]
    ));
}

/// Extraction's snapshots cross whole, keyed by candidate: the
/// form on the other side reads the text pools off the same value the run
/// produced.
#[cfg(feature = "desktop")]
#[test]
fn extracted_signals_cross_the_bus_with_their_key() {
    let signals = bae_core::signals::Signals {
        disc_id: bae_core::signals::DiscIdSignal::Absent { track_count: 9 },
        barcode: bae_core::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: bae_core::signals::TextSignal::Settled {
            catalogs: vec![bae_core::signals::SourcedValue::new(
                "CAT-1".to_string(),
                bae_core::signals::SignalOrigin::Artwork,
            )],
            free_text: vec!["Album Title".to_string()],
        },
        durations: bae_core::import::probe::SourceDurations::totalling(1_000),
    };

    let crossed =
        super::ui_events::convert_ui_event(bae_core::ui::UiBusEvent::CandidateSignalsUpdated {
            key: "reidentify:release-1".to_string(),
            signals,
        })
        .expect("a desktop bridge carries the import stream");

    let crate::types::BridgeUiEvent::CandidateSignalsUpdated { key, signals } = crossed else {
        panic!("expected the signals event, got {crossed:?}");
    };
    assert_eq!(key, "reidentify:release-1");
    let crate::types::BridgeTextSignal::Settled {
        catalogs,
        free_text,
    } = signals.text
    else {
        panic!(
            "a settled text signal crosses as settled, got {:?}",
            signals.text
        );
    };
    assert_eq!(free_text, vec!["Album Title".to_string()]);
    assert_eq!(
        catalogs
            .iter()
            .map(|catalog| catalog.value.clone())
            .collect::<Vec<_>>(),
        vec!["CAT-1".to_string()]
    );
}

/// The per-file converter keeps source size separate from the active phase's
/// own bar and preserves every durable phase.
#[test]
fn upload_file_op_flattens_state_into_fields() {
    use crate::types::{BridgeUploadBar, BridgeUploadFileState, BridgeUploadPhase};
    use bae_core::library::{UploadFileOp, UploadState};

    let convert = |state: UploadState| {
        crate::types::BridgeUploadFileOp::from_core(UploadFileOp {
            file_id: "file-1".into(),
            label: bae_core::library::UploadFileLabel::Filename("01 Track Title.flac".into()),
            source_bytes_total: 1000,
            state,
        })
    };

    let queued = convert(UploadState::Queued);
    assert_eq!(
        queued.label,
        crate::types::BridgeUploadFileLabel::Filename {
            name: "01 Track Title.flac".into()
        }
    );
    assert_eq!(queued.state, BridgeUploadFileState::Queued);
    assert_eq!((queued.bar, queued.last_error), (None, None));

    let preparing = convert(UploadState::Preparing {
        bytes_done: 400,
        bytes_total: 1000,
    });
    assert_eq!(preparing.state, BridgeUploadFileState::Preparing);
    assert_eq!(
        preparing.bar,
        Some(BridgeUploadBar {
            phase: BridgeUploadPhase::Preparing,
            bytes_done: 400,
            bytes_total: 1000,
        })
    );

    let prepared = convert(UploadState::Prepared { bytes_total: 1016 });
    assert_eq!(prepared.state, BridgeUploadFileState::Prepared);
    assert_eq!(prepared.bar, None);

    let uploading = convert(UploadState::Uploading {
        bytes_done: 420,
        bytes_total: 1016,
    });
    assert_eq!(uploading.state, BridgeUploadFileState::Uploading);
    assert_eq!(
        uploading.bar,
        Some(BridgeUploadBar {
            phase: BridgeUploadPhase::Uploading,
            bytes_done: 420,
            bytes_total: 1016,
        })
    );

    let failed = convert(UploadState::RetryingUpload {
        last_error: "cloud write failed".into(),
        bytes_total: 1016,
    });
    assert_eq!(failed.state, BridgeUploadFileState::Retrying);
    assert_eq!(failed.last_error.as_deref(), Some("cloud write failed"));
    assert_eq!(failed.bar, None);

    let uploaded = convert(UploadState::Uploaded { bytes_total: 1016 });
    assert_eq!(uploaded.state, BridgeUploadFileState::Uploaded);
    assert_eq!(uploaded.bar, None);
    assert_eq!(uploaded.source_bytes_total, 1000);
}

/// The aggregate converter carries core's phase-scoped bar across unchanged,
/// so the UI cannot fill a bar with one phase's bytes and label it with
/// another's.
#[test]
fn upload_progress_carries_the_phase_scoped_bar() {
    use crate::types::{BridgeUploadBar, BridgeUploadPhase};

    let preparing =
        crate::types::BridgeUploadProgress::from_core(bae_core::library::UploadProgress {
            queued: 1,
            uploading: 1,
            preparation_bytes_done: 1000,
            preparation_bytes_total: 1100,
            upload_bytes_done: 250,
            upload_bytes_total: 1016,
            upload_bytes_total_complete: false,
            ..Default::default()
        });
    assert_eq!(
        preparing.bar,
        Some(BridgeUploadBar {
            phase: BridgeUploadPhase::Preparing,
            bytes_done: 1000,
            bytes_total: 1100,
        })
    );

    let uploading =
        crate::types::BridgeUploadProgress::from_core(bae_core::library::UploadProgress {
            uploading: 1,
            prepared: 1,
            preparation_bytes_done: 1100,
            preparation_bytes_total: 1100,
            upload_bytes_done: 250,
            upload_bytes_total: 1132,
            upload_bytes_total_complete: true,
            ..Default::default()
        });
    assert_eq!(
        uploading.bar,
        Some(BridgeUploadBar {
            phase: BridgeUploadPhase::Uploading,
            bytes_done: 250,
            bytes_total: 1132,
        })
    );
}

#[test]
fn upload_progress_preserves_core_cancellation_availability() {
    let queued = crate::types::BridgeUploadProgress::from_core(bae_core::library::UploadProgress {
        queued: 1,
        retrying: 2,
        ..Default::default()
    });
    assert!(queued.can_cancel);
    assert_eq!(queued.retrying, 2);

    let publishing =
        crate::types::BridgeUploadProgress::from_core(bae_core::library::UploadProgress {
            publishing: 1,
            ..Default::default()
        });
    assert!(!publishing.can_cancel);
}

#[test]
fn composer_detail_conversion_preserves_work_groups() {
    let created_at = "2026-01-01T00:00:00Z".parse().unwrap();
    let detail = ComposerDetail {
        composer: ComposerSummary {
            raw: DbComposerSummary {
                artist: DbArtist {
                    id: "artist-composer-a".to_string(),
                    name: "Composer Name A".to_string(),
                    sort_name: Some("Composer Name Sort A".to_string()),
                    discogs_artist_id: None,
                    musicbrainz_artist_id: None,
                    created_at,
                },
                work_count: 2,
                linked_release_count: 1,
                unlinked_credit_count: 0,
            },
            image: None,
        },
        work_groups: vec![ComposerWorkGroup {
            id: "work-parent-a".to_string(),
            parent: Some(WorkSummary {
                raw: DbWorkSummary {
                    work: DbWork {
                        id: "work-parent-a".to_string(),
                        title: "Parent Work A".to_string(),
                        disambiguation: None,
                        work_type: Some("work".to_string()),
                        musicbrainz_work_id: "mb-work-parent-a".to_string(),
                        created_at,
                    },
                    parent_work_id: None,
                    composer_names: Some("Composer Name A".to_string()),
                    linked_release_count: 1,
                    representative_release_id: Some("release-a".to_string()),
                },
                representative_cover: None,
            }),
            works: vec![WorkSummary {
                raw: DbWorkSummary {
                    work: DbWork {
                        id: "work-child-a".to_string(),
                        title: "Child Work A".to_string(),
                        disambiguation: None,
                        work_type: Some("part".to_string()),
                        musicbrainz_work_id: "mb-work-child-a".to_string(),
                        created_at,
                    },
                    parent_work_id: Some("work-parent-a".to_string()),
                    composer_names: Some("Composer Name A".to_string()),
                    linked_release_count: 1,
                    representative_release_id: Some("release-a".to_string()),
                },
                representative_cover: None,
            }],
        }],
        unlinked_release_roles: Vec::new(),
        unlinked_track_roles: Vec::new(),
        default_work_id: Some("work-parent-a".to_string()),
    };

    let bridge = super::BridgeComposerDetail::from_core(detail);

    assert_eq!(bridge.work_groups.len(), 1);
    assert_eq!(bridge.default_work_id.as_deref(), Some("work-parent-a"));
    let group = &bridge.work_groups[0];
    assert_eq!(group.id, "work-parent-a");
    assert_eq!(
        group.parent.as_ref().map(|work| work.work_id.as_str()),
        Some("work-parent-a")
    );
    assert_eq!(group.works.len(), 1);
    assert_eq!(group.works[0].work_id, "work-child-a");
    assert_eq!(
        group.works[0].parent_work_id.as_deref(),
        Some("work-parent-a")
    );
    assert_eq!(
        group.works[0].representative_release_id.as_deref(),
        Some("release-a")
    );
}

#[test]
fn work_detail_conversion_preserves_work_release_rows() {
    let created_at = "2026-01-01T00:00:00Z".parse().unwrap();
    let work = WorkSummary {
        raw: DbWorkSummary {
            work: DbWork {
                id: "work-a".to_string(),
                title: "Work Title A".to_string(),
                disambiguation: None,
                work_type: Some("work".to_string()),
                musicbrainz_work_id: "mb-work-a".to_string(),
                created_at,
            },
            parent_work_id: None,
            composer_names: Some("Composer Name A".to_string()),
            linked_release_count: 1,
            representative_release_id: Some("release-a".to_string()),
        },
        representative_cover: None,
    };
    let detail = WorkDetail {
        work,
        child_works: Vec::new(),
        releases: vec![WorkReleaseSummary {
            release_id: "release-a".to_string(),
            album_id: "album-a".to_string(),
            album_title: "Album Title A".to_string(),
            display_name: "2026 CD".to_string(),
            format: Some("CD".to_string()),
            cover: None,
        }],
        tracks: Vec::new(),
    };

    let bridge = super::BridgeWorkDetail::from_core(detail);

    assert_eq!(bridge.releases.len(), 1);
    let release = &bridge.releases[0];
    assert_eq!(release.release_id, "release-a");
    assert_eq!(release.album_id, "album-a");
    assert_eq!(release.album_title, "Album Title A");
    assert_eq!(release.display_name, "2026 CD");
    assert_eq!(release.format.as_deref(), Some("CD"));
}

/// A failing sync cycle's fault has to reach the front-ends, not just the log.
/// Core records the whole chain as the sync-status error's diagnostic detail;
/// the crossing keeps it beside the category the UI localizes, so a surface has
/// something to render besides "Something went wrong."
#[test]
fn a_failed_sync_status_crosses_with_its_fault() {
    let fault = "sync cycle: pull Store commits: database: retained Merge replay \
                 has an unresolved foreign-key dependency";
    let snapshot = bae_core::library::SyncStatusSnapshot {
        error: Some(bae_core::ui::UiError::internal(fault)),
        blocked: Vec::new(),
        last_sync_time: None,
        syncing: false,
        sync_ready: false,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    let Some(crate::types::BridgeError::Diagnostic { category, detail }) = bridge.error else {
        panic!("a recorded sync failure crosses as a diagnostic");
    };
    assert_eq!(category, crate::types::BridgeErrorCategory::Internal);
    assert_eq!(
        detail, fault,
        "the fault crosses whole, never summarized away"
    );
}

/// A healthy cycle carries no error, so no surface renders a failure for it.
#[test]
fn a_healthy_sync_status_crosses_without_an_error() {
    let snapshot = bae_core::library::SyncStatusSnapshot {
        error: None,
        blocked: Vec::new(),
        last_sync_time: Some(1_700_000_000_000),
        syncing: false,
        sync_ready: true,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert!(bridge.error.is_none());
    assert!(bridge.sync_ready);
}

/// The runtime stream, end to end through the bridge: a claim crosses as one
/// key with an import in flight, the import ending takes the key out of the
/// stream, and a subscriber that joins mid-import is told about it rather than
/// left waiting for the next tick.
#[cfg(feature = "desktop")]
mod candidate_runtime {
    use super::*;
    use crate::types::{BridgeCandidateRuntimeChange, CandidateRuntimeCallback};
    use std::time::Duration;

    struct ForwardingCallback {
        changes: tokio::sync::mpsc::UnboundedSender<BridgeCandidateRuntimeChange>,
    }

    impl CandidateRuntimeCallback for ForwardingCallback {
        fn on_change(&self, change: BridgeCandidateRuntimeChange) {
            let _ = self.changes.send(change);
        }
    }

    fn subscribe(
        handle: &Arc<super::super::AppHandle>,
    ) -> (
        Arc<crate::LiveSubscription>,
        tokio::sync::mpsc::UnboundedReceiver<BridgeCandidateRuntimeChange>,
    ) {
        let (changes, receiver) = tokio::sync::mpsc::unbounded_channel();
        let subscription =
            handle.subscribe_candidate_runtime(Box::new(ForwardingCallback { changes }));
        (subscription, receiver)
    }

    /// What one change says about `key`.
    ///
    /// Three shapes say something, and which one arrives is timing rather than
    /// meaning. `Updated` and `Removed` name the key. A `Reset` — the
    /// subscription re-stating every key in flight after the broadcast dropped
    /// deliveries under load — speaks about every key at once, including the
    /// ones it leaves out: "a consumer holding a key this does not list treats
    /// it as removed" is the type's own contract, and on a loaded runner it is
    /// the *only* thing that ever says a key is gone, because the `Removed`
    /// that would have said so is exactly what was dropped.
    enum SaysAbout {
        InFlight(Box<crate::types::BridgeCandidateRuntimeSnapshot>),
        Gone,
        /// A change about some other key. An empty library has none, but a
        /// shared test root should not be able to make this flaky.
        NothingOfTheSort,
    }

    fn says_about(change: &BridgeCandidateRuntimeChange, key: &str) -> SaysAbout {
        match change {
            BridgeCandidateRuntimeChange::Updated {
                key: changed,
                runtime,
            } if changed == key => SaysAbout::InFlight(Box::new(runtime.clone())),
            BridgeCandidateRuntimeChange::Removed { key: changed } if changed == key => {
                SaysAbout::Gone
            }
            BridgeCandidateRuntimeChange::Updated { .. }
            | BridgeCandidateRuntimeChange::Removed { .. } => SaysAbout::NothingOfTheSort,
            BridgeCandidateRuntimeChange::Reset { runtimes } => runtimes
                .iter()
                .find(|entry| entry.key == key)
                .map_or(SaysAbout::Gone, |entry| {
                    SaysAbout::InFlight(Box::new(entry.runtime.clone()))
                }),
        }
    }

    /// Drain changes until the stream says `key` is in flight, and answer with
    /// what it is running.
    ///
    /// A change saying the opposite on the way there is not a failure — it is
    /// the answer not having arrived yet, so this keeps draining. Only the
    /// timeout is a failure.
    fn wait_in_flight(
        handle: &Arc<super::super::AppHandle>,
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<BridgeCandidateRuntimeChange>,
        key: &str,
    ) -> crate::types::BridgeCandidateRuntimeSnapshot {
        drain_until(handle, receiver, key, |said| match said {
            SaysAbout::InFlight(runtime) => Some(*runtime),
            SaysAbout::Gone | SaysAbout::NothingOfTheSort => None,
        })
        .expect("the stream says the key is in flight")
    }

    /// Drain changes until the stream says `key` is not in flight, by either of
    /// the two shapes that say it.
    fn wait_gone(
        handle: &Arc<super::super::AppHandle>,
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<BridgeCandidateRuntimeChange>,
        key: &str,
    ) {
        drain_until(handle, receiver, key, |said| match said {
            SaysAbout::Gone => Some(()),
            SaysAbout::InFlight(_) | SaysAbout::NothingOfTheSort => None,
        })
        .expect("the stream says the key is gone");
    }

    fn drain_until<T>(
        handle: &Arc<super::super::AppHandle>,
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<BridgeCandidateRuntimeChange>,
        key: &str,
        settled: impl Fn(SaysAbout) -> Option<T>,
    ) -> Option<T> {
        handle
            .runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let change = receiver.recv().await.expect("the subscription stays open");
                        if let Some(value) = settled(says_about(&change, key)) {
                            return value;
                        }
                    }
                })
                .await
            })
            .ok()
    }

    /// A lagged subscription never sends the `Removed` — it sends one `Reset`
    /// standing for the whole map, and a key it leaves out is gone. A consumer
    /// that waits for `Removed` alone waits for a message that is not coming,
    /// which is what the loaded runner produced and the quiet machine did not.
    #[test]
    fn a_reset_that_omits_the_key_says_the_key_is_gone() {
        let (handle, _root) = fresh_bridge_handle("candidate-runtime-reset");
        let key = "/watch/Album Title".to_string();
        let running = crate::types::BridgeCandidateRuntimeSnapshot::from_core(
            bae_core::import::CandidateRuntimeSnapshot {
                identify: None,
                import: Some(bae_core::import::ImportInFlight {
                    progress_percent: None,
                    step: None,
                }),
                search: None,
            },
        );

        let (changes, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        // Another key's reset entry is not this key's answer, so the map's
        // omission has to be read rather than its emptiness.
        changes
            .send(BridgeCandidateRuntimeChange::Reset {
                runtimes: vec![crate::types::BridgeKeyedCandidateRuntime {
                    key: "/watch/Someone Else".to_string(),
                    runtime: running.clone(),
                }],
            })
            .unwrap();
        wait_gone(&handle, &mut receiver, &key);

        // And the same reset listing the key says it is still in flight.
        let (changes, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        changes
            .send(BridgeCandidateRuntimeChange::Reset {
                runtimes: vec![crate::types::BridgeKeyedCandidateRuntime {
                    key: key.clone(),
                    runtime: running,
                }],
            })
            .unwrap();
        wait_in_flight(&handle, &mut receiver, &key);
    }

    #[test]
    fn a_claimed_import_crosses_as_in_flight_and_leaves_when_it_fails() {
        let (handle, _root) = fresh_bridge_handle("candidate-runtime-claim");
        let key = "/watch/Album Title".to_string();
        let (_watching, mut watching_changes) = subscribe(&handle);

        handle
            .runtime
            .block_on(handle.services.claim_candidate_for_import_for_test(&key));

        let claimed = wait_in_flight(&handle, &mut watching_changes, &key);
        let import = claimed
            .import
            .clone()
            .expect("a claimed candidate has an import in flight");
        assert_eq!(import.progress_percent, None);
        assert!(
            matches!(
                import.step,
                Some(crate::types::BridgeImportStep::Preparing { .. })
            ),
            "a claim is the queued step, got {:?}",
            import.step
        );

        // A subscriber that opens while the import is running is told what is
        // running rather than left waiting for the next tick.
        let (_joining, mut joining_changes) = subscribe(&handle);
        assert!(
            wait_in_flight(&handle, &mut joining_changes, &key)
                .import
                .is_some(),
            "a late subscriber is replayed the running import"
        );

        handle
            .services
            .import_emit_event_for_test(bae_core::import::ImportEvent::ImportProgress {
                candidate_key: key.clone(),
                progress: bae_core::import::ImportProgress::Failed {
                    error: "no space left".to_string(),
                    import_id: "import-1".to_string(),
                },
            });

        // The failure is a row by the time it is announced, so the key stops
        // being in flight rather than crossing as a status of its own.
        wait_gone(&handle, &mut watching_changes, &key);
        assert!(
            handle.candidate_runtime(key).is_none(),
            "and nothing is left to read for the key"
        );
    }
}
