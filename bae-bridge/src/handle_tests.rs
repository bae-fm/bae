use bae_core::album_detail::{
    ComposerDetail, ComposerSummary, ComposerWorkGroup, WorkDetail, WorkReleaseSummary, WorkSummary,
};
use bae_core::db::{DbArtist, DbComposerSummary, DbWork, DbWorkSummary};
#[cfg(not(feature = "desktop"))]
use bae_core::keys::BaeStoreKeysExt;
use std::sync::{Arc, Mutex};

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
        });
        assert_eq!(group.header_key.as_deref(), expected);
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

#[cfg(not(feature = "desktop"))]
fn fresh_bridge_handle(test_name: &str) -> (super::AppHandle, std::path::PathBuf) {
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
    let key_service = bae_core::keys::StoreKeys::bind(library_id.clone());
    key_service
        .set_discogs_key("test-discogs-token")
        .expect("seed test Discogs key");
    // One id source for the whole test app: the library owner mints every
    // database and domain id from this provider.
    let ids: coven::IdRef = Arc::new(coven::SequentialIdProvider::new(test_name));
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let config_handle = Arc::new(bae_core::config::ConfigHandle::new(config));
    let manager = bae_core::library::LibraryManager::open(
        config_handle,
        key_service,
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
    let handle = super::AppHandle {
        runtime,
        services,
        ui_event_bus: bae_core::ui::UiEventBus::new(),
    };

    (handle, root)
}

#[cfg(not(feature = "desktop"))]
#[test]
fn album_browse_subscription_pulls_count_before_any_windows() {
    let (handle, _root) = fresh_bridge_handle("album-browse-empty-windows");
    let subscription = handle.subscribe_album_browse(Vec::new());

    let snapshot = handle
        .runtime
        .block_on(subscription.next())
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
        .block_on(subscription.next())
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
        .block_on(subscription.next())
        .expect("initial album browse snapshot");

    let pending = handle.runtime.spawn({
        let subscription = subscription.clone();
        async move { subscription.next().await }
    });
    handle.runtime.block_on(subscription.cancel());
    let result = handle.runtime.block_on(pending).expect("next task joins");

    assert!(matches!(result, Err(crate::types::BridgeError::Cancelled)));
    assert!(matches!(
        subscription.set_windows(Vec::new()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}

#[cfg(not(feature = "desktop"))]
#[test]
fn enqueue_export_missing_release_does_not_panic() {
    let (handle, root) = fresh_bridge_handle("enqueue-export-missing-release");
    let result = handle.runtime.block_on(async {
        handle
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

/// The per-file converter flattens core's `UploadState` into plain fields:
/// live bytes ride `Uploading`, `Done` reads as fully transferred, and the
/// failure message lands in `last_error` under `Retrying`.
#[test]
fn upload_file_op_flattens_state_into_fields() {
    use crate::types::BridgeUploadFileState;
    use bae_core::library::{UploadFileOp, UploadState};

    let convert = |state: UploadState| {
        crate::types::BridgeUploadFileOp::from_core(UploadFileOp {
            file_id: "file-1".into(),
            display_name: "01 Track Title.flac".into(),
            bytes_total: 1000,
            state,
        })
    };

    let queued = convert(UploadState::Queued);
    assert_eq!(queued.state, BridgeUploadFileState::Queued);
    assert_eq!((queued.bytes_done, queued.last_error), (0, None));

    let active = convert(UploadState::Active { bytes_done: 400 });
    assert_eq!(active.state, BridgeUploadFileState::Uploading);
    assert_eq!(active.bytes_done, 400);

    let failed = convert(UploadState::Failed {
        last_error: "cloud write failed".into(),
    });
    assert_eq!(failed.state, BridgeUploadFileState::Retrying);
    assert_eq!(failed.last_error.as_deref(), Some("cloud write failed"));

    let done = convert(UploadState::Done);
    assert_eq!(done.state, BridgeUploadFileState::Done);
    assert_eq!(done.bytes_done, 1000);
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
