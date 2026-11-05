use bae_db::{DbArtist, DbWork};
use bae_views::album_detail::{
    ComposerDetail, ComposerDetailItem, ComposerDetailRow, ComposerHeading, ComposerSummary,
    WorkDetail, WorkReleaseSummary, WorkSummary,
};
use bae_views::{DbComposerSummary, DbWorkSummary};
use std::sync::Arc;

fn track_detail(
    position: bae_types::mediums::TrackPosition,
    duration_ms: u64,
) -> bae_views::album_detail::TrackDetail {
    bae_views::album_detail::TrackDetail {
        id: "track-1".to_string(),
        title: "Track".to_string(),
        duration_ms,
        artist_names: "Artist".to_string(),
        display_artist: None,
        position_text: "1".to_string(),
        position,
    }
}

#[test]
fn track_group_precomputes_header_key_per_side() {
    use bae_types::mediums::{TrackPosition, TrackSide};

    let cases = [
        (
            TrackSide::Sided { side_letter: "A".to_string() },
            TrackPosition::Sided { side_letter: "A".to_string(), number: 1 },
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
        let group =
            crate::types::BridgeTrackGroup::from_core(bae_views::album_detail::TrackGroup {
                side,
                tracks: vec![track_detail(position, 187_000)],
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
    use bae_types::mediums::TrackPosition;

    // A track's length renders a clock while the raw number is retained.
    let present = crate::types::BridgeTrack::from_core(track_detail(
        TrackPosition::Flat { number: 1 },
        187_000,
    ));
    assert_eq!(present.duration_ms, 187_000);
    let clock = present.duration_clock;
    assert_eq!((clock.minutes, clock.seconds), (3, 7));
}

fn track_search_result(duration_ms: u64) -> bae_views::album_detail::TrackSearchResult {
    bae_views::album_detail::TrackSearchResult {
        id: "track-1".to_string(),
        title: "Track".to_string(),
        duration_ms,
        release_id: "release-1".to_string(),
        album_id: "album-1".to_string(),
        album_title: "Album".to_string(),
        artist_name: "Artist".to_string(),
        cover: None,
    }
}

#[test]
fn track_search_result_precomputes_duration_clock() {
    // A track's length renders a clock; the raw number does not cross.
    let present = crate::types::BridgeTrackSearchResult::from_core(track_search_result(187_000));
    let clock = present.duration_clock;
    assert_eq!((clock.minutes, clock.seconds), (3, 7));
}

fn queue_item(duration_ms: u64) -> bae_playback::QueueItem {
    bae_playback::QueueItem {
        entry_id: "entry-1".to_string(),
        track_id: "track-1".to_string(),
        display: bae_playback::TrackDisplay {
            title: "Track".to_string(),
            artist_names: "Artist".to_string(),
            album_id: "album-1".to_string(),
            release_id: "release-1".to_string(),
            album_title: "Album".to_string(),
            cover_image: None,
        },
        duration_ms,
    }
}

#[test]
fn queue_entry_precomputes_duration_clock() {
    // A track's length renders a clock; the raw number does not cross.
    let present = crate::types::BridgeQueueEntry::from_core(queue_item(187_000));
    let clock = present.duration_clock;
    assert_eq!((clock.minutes, clock.seconds), (3, 7));
}

/// A handle over a fresh, empty library, built the way the app builds one.
pub(super) fn fresh_bridge_handle(test_name: &str) -> (Arc<super::AppHandle>, std::path::PathBuf) {
    let (runtime, library, root) = fresh_test_library(test_name);
    (start_test_handle(runtime, library, &root), root)
}

/// A fresh library opened in a directory named for `test_name`, on the
/// runtime a handle over it runs on.
pub(super) fn fresh_test_library(
    test_name: &str,
) -> (tokio::runtime::Runtime, bae_library::test_libraries::TestLibrary, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("bae-bridge-{test_name}"));
    match std::fs::remove_dir_all(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove stale test library dir: {error}"),
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("test runtime");
    let app_dir = bae_app_dir::AppDir::under_home(&root).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let handle = runtime
        .block_on(
            bae_db::configure_store(installation.builder(), bae_library::SCHEMA)
                .create_store("Test Library"),
        )
        .expect("test store");
    let library = runtime.block_on(bae_library::test_libraries::test_library_from_handle(
        handle,
        &root,
        bae_catalogs::providers::Providers::offline(),
        bae_util::http::Http::for_test(),
    ));
    (runtime, library, root)
}

/// A host over the bae directory under `home`, which a test starts a handle
/// for.
pub(crate) fn test_host(home: &std::path::Path) -> Arc<crate::BridgeHost> {
    crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(home.to_str().unwrap().into()).unwrap(),
    )
    .unwrap()
}

/// Start a handle over `library` on `runtime`, for a host over the bae
/// directory under `home`, as the app starts one.
pub(super) fn start_test_handle(
    runtime: tokio::runtime::Runtime,
    library: bae_library::test_libraries::TestLibrary,
    home: &std::path::Path,
) -> Arc<super::AppHandle> {
    let services = runtime.block_on(async move {
        bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start")
    });
    Arc::new(
        super::AppHandle::start(
            services,
            runtime,
            bae_diagnostics::Diagnostics::noop(),
            test_host(home),
        )
        .expect("the bridge handle starts"),
    )
}

#[cfg(not(feature = "desktop"))]
#[test]
fn album_browse_subscription_pulls_count_before_any_windows() {
    let (handle, _root) = fresh_bridge_handle("album-browse-empty-windows");
    let subscription = handle.subscribe_album_browse(Vec::new(), false);

    let snapshot =
        handle.runtime.block_on(subscription.clone().next()).expect("album browse snapshot");

    assert_eq!(snapshot.total_count, 0);
    assert!(snapshot.windows.is_empty());
    assert_eq!(snapshot.cause, crate::types::BridgeLiveQueryCause::Request);
    assert_eq!(snapshot.request_revision, 0);

    subscription
        .set_windows(vec![crate::types::BridgeLibraryPageWindow { offset: 0, limit: 50 }])
        .expect("request first album window");
    let requested = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("requested album browse snapshot");
    assert_eq!(requested.cause, crate::types::BridgeLiveQueryCause::Request);
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
    let subscription = handle.subscribe_album_browse(Vec::new(), false);
    handle.runtime.block_on(subscription.clone().next()).expect("initial album browse snapshot");

    let result = handle.runtime.block_on(async {
        let pending = tokio::spawn({
            let subscription = subscription.clone();
            async move { subscription.next().await }
        });
        subscription.clone().cancel().await.expect("cancel album browse subscription");
        pending.await.expect("next task joins")
    });

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
                    .block_on(handle.save_sync_config(
                        crate::types::BridgeSaveSyncConfig {
                            bucket: "bucket".to_string(),
                            region: "us-east-1".to_string(),
                            endpoint: Some("http://127.0.0.1:1".to_string()),
                            key_prefix: String::new(),
                            access_key: "access-key".to_string(),
                            secret_key: "secret-key".to_string(),
                            profile: None,
                        },
                        "Test device".into(),
                        None,
                    ))
                    .err()
                    .expect("an unreachable endpoint must fail the probe");
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
            .enqueue_export("missing-release".to_string(), coven::path_bytes(&root.join("exports")))
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

/// The text pools the run in flight last read cross with its runtime, for
/// the search form's autocomplete; a queued key carries none.
#[cfg(feature = "desktop")]
#[test]
fn a_run_s_signals_cross_with_its_runtime() {
    let signals = bae_identify::signals::Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: bae_identify::signals::DiscIdSignal::Absent,
        barcode: bae_identify::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: bae_identify::signals::TextSignal::Settled {
            catalogs: vec!["CAT-1".to_string()],
            free_text: vec!["Album Title".to_string()],
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    };
    let running = |signals: Option<bae_identify::signals::Signals>| {
        crate::types::BridgeCandidateRuntimeSnapshot::from_core(
            bae_import::CandidateRuntimeSnapshot {
                identification: Some(bae_import::IdentificationRuntime::Running(
                    bae_identify::PublishedRun {
                        outcome: bae_identify::RunOutcome::Answered(
                            bae_identify::TerminalVerdict::ManualOnly {
                                track_count: 9,
                                ledger: None,
                            },
                        ),
                        signals,
                    },
                )),
                import: None,
                search: None,
            },
        )
    };

    let crossed = running(Some(signals)).signals.expect("the run's signals cross with its runtime");
    let crate::types::BridgeTextSignal::Settled { catalogs, free_text } = crossed.text else {
        panic!("a settled text signal crosses as settled, got {:?}", crossed.text);
    };
    assert_eq!(free_text, vec!["Album Title".to_string()]);
    assert_eq!(catalogs, vec!["CAT-1".to_string()]);
    assert!(running(None).signals.is_none(), "a run extraction has not reported to carries none");

    let queued = crate::types::BridgeCandidateRuntimeSnapshot::from_core(
        bae_import::CandidateRuntimeSnapshot {
            identification: Some(bae_import::IdentificationRuntime::Queued(
                bae_import::Admission::Automatic,
            )),
            import: None,
            search: None,
        },
    );
    assert!(queued.signals.is_none(), "a queued key has no run to have read any");
}

#[test]
fn upload_progress_keeps_the_counts_and_their_phase() {
    let value = crate::types::BridgeUploadProgress::from_core(bae_db::UploadProgress {
        queued: 1,
        uploading: 2,
    });
    assert_eq!(value.queued, 1);
    assert_eq!(value.uploading, 2);
    assert_eq!(value.activity, crate::types::BridgeUploadActivity::Uploading);
}

#[test]
fn composer_detail_conversion_carries_the_composer_and_its_first_work() {
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
                image: None,
                work_count: 2,
                linked_release_count: 1,
                unlinked_credit_count: 0,
            },
        },
        default_work_id: Some("work-parent-a".to_string()),
    };

    let bridge = super::BridgeComposerDetail::from_core(detail);

    assert_eq!(
        bridge.composer.count_line,
        crate::types::BridgeCountLabel { key: "core.library.work_count".to_string(), count: 2 },
        "the composer's line counts its works"
    );
    assert_eq!(bridge.default_work_id.as_deref(), Some("work-parent-a"));
}

#[test]
fn composer_detail_row_conversion_carries_its_heading_and_part() {
    let created_at = "2026-01-01T00:00:00Z".parse().unwrap();
    let row = ComposerDetailRow {
        id: 7,
        heading: Some(ComposerHeading::Works),
        item: ComposerDetailItem::Work {
            work: WorkSummary {
                raw: DbWorkSummary {
                    work: DbWork {
                        id: "work-child-a".to_string(),
                        title: "Child Work A".to_string(),
                        disambiguation: None,
                        work_type: Some("part".to_string()),
                        musicbrainz_work_id: "mb-work-child-a".to_string(),
                        created_at,
                    },
                    composer_names: Some("Composer Name A".to_string()),
                    linked_release_count: 1,
                    representative_release_id: Some("release-a".to_string()),
                    representative_cover: None,
                },
            },
            part: true,
        },
    };

    let bridge = crate::types::BridgeComposerDetailRow::from_core(row);

    assert_eq!(bridge.id, 7);
    assert!(matches!(bridge.heading, Some(crate::types::BridgeComposerHeading::Works)));
    let crate::types::BridgeComposerDetailItem::Work { work, part } = bridge.item else {
        panic!("a work row stays a work row");
    };
    assert!(part);
    assert_eq!(work.work_id, "work-child-a");
    assert_eq!(work.representative_release_id.as_deref(), Some("release-a"));
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
            composer_names: Some("Composer Name A".to_string()),
            linked_release_count: 1,
            representative_release_id: Some("release-a".to_string()),
            representative_cover: None,
        },
    };
    let detail = WorkDetail {
        work,
        child_works: Vec::new(),
        releases: vec![WorkReleaseSummary {
            release_id: "release-a".to_string(),
            album_id: "album-a".to_string(),
            album_title: "Album Title A".to_string(),
            name: bae_views::album_detail::ReleaseName::Described {
                year: Some(2026),
                media: vec![bae_types::pressing::MediaCount {
                    medium: bae_types::pressing::Medium::Cd,
                    count: 2,
                }],
            },
            media: vec![bae_types::pressing::MediaCount {
                medium: bae_types::pressing::Medium::Cd,
                count: 2,
            }],
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
    // Two CDs, worded: the count and the medium's printed name.
    let two_cds = vec![crate::types::BridgeFactTerm::Counted {
        count: 2,
        label: crate::types::BridgeTermLabel::Verbatim { text: "CD".to_string() },
    }];
    assert_eq!(
        release.name,
        crate::types::BridgeReleaseName::Described { year: Some(2026), media: two_cds.clone() }
    );
    assert_eq!(release.media, two_cds);
}

/// A failing sync cycle's diagnostic detail crosses beside the category the UI
/// localizes.
#[test]
fn a_failed_sync_status_crosses_with_its_fault() {
    let fault = "sync cycle: pull Store commits: database: retained Merge replay \
                 has an unresolved foreign-key dependency";
    let snapshot = bae_db::SyncStatusSnapshot {
        error: Some(bae_diagnostics::UiError::internal(fault)),
        coven: coven::SyncStatus { state: coven::SyncState::NotSynced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.indicator, None);
    let Some(crate::types::BridgeError::Diagnostic { category, detail }) = bridge.error else {
        panic!("a recorded sync failure crosses as a diagnostic");
    };
    assert_eq!(category, crate::types::BridgeErrorCategory::Internal);
    assert_eq!(detail, fault, "the fault crosses whole, never summarized away");
}

/// A healthy cycle carries no error, so no surface renders a failure for it.
#[test]
fn a_healthy_sync_status_crosses_without_an_error() {
    let snapshot = bae_db::SyncStatusSnapshot {
        error: None,
        coven: coven::SyncStatus { state: coven::SyncState::Synced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert!(bridge.error.is_none());
    assert_eq!(bridge.indicator, Some(crate::types::BridgeSyncIndicator::Synced));
    assert!(!bridge.reconnect_offered);
}

/// A start that failed with storage set up crosses with why and with
/// Reconnect offered beside it.
#[test]
fn a_failed_start_crosses_offering_reconnect() {
    let snapshot = bae_db::SyncStatusSnapshot {
        error: Some(bae_diagnostics::UiError::internal("the store key is missing")),
        coven: coven::SyncStatus { state: coven::SyncState::Synced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::NotSyncing,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.indicator, Some(crate::types::BridgeSyncIndicator::Failed));
    assert!(bridge.error.is_some());
    assert!(bridge.reconnect_offered);
}

/// The status line shows a failure the connection's pending record shows
/// already only once, as that record.
#[test]
fn a_failure_its_record_shows_crosses_only_as_the_record() {
    let failure = coven::StorageFailure::Protocol;
    let snapshot = bae_db::SyncStatusSnapshot {
        error: None,
        coven: coven::SyncStatus {
            state: coven::SyncState::Failed(coven::SyncCause::Pass(coven::LastPass::Failed {
                failure: coven::StatusFailure::Storage(failure),
                error: "storage refused the pass".into(),
            })),
            last_synced: None,
        },
        pending: vec![bae_db::PendingItem::before_open(coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            coven::PendingReason::Storage {
                provider: coven::CloudProvider::S3,
                failure,
                answered: true,
            },
        ))],
        retry_effect: coven::RetryEffect::Pass,
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert!(bridge.error.is_none());
    assert_eq!(bridge.indicator, Some(crate::types::BridgeSyncIndicator::Waiting { items: 1 }));
}

/// One candidate's runtime pulled end to end: a claim crosses as its import
/// in flight, a subscriber opening mid-import reads it, and the import
/// failing leaves nothing in flight.
#[cfg(feature = "desktop")]
mod candidate_runtime {
    use super::*;
    use bae_util::test_channels::HANG_GUARD;

    /// Pull from `subscription` until a value satisfies `wanted`; only a
    /// value that never comes fails.
    fn pull_until(
        handle: &Arc<super::super::AppHandle>,
        subscription: &Arc<crate::CandidateRuntimeSubscription>,
        wanted: impl Fn(&Option<crate::types::BridgeCandidateRuntimeSnapshot>) -> bool,
    ) -> Option<crate::types::BridgeCandidateRuntimeSnapshot> {
        handle.runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, async {
                loop {
                    let value = subscription.clone().next().await.expect("the pull reads");
                    if wanted(&value) {
                        return value;
                    }
                }
            })
            .await
            .expect("the subscription gets there")
        })
    }

    #[test]
    fn a_claimed_import_crosses_as_in_flight_and_leaves_when_it_fails() {
        let (handle, _root) = fresh_bridge_handle("candidate-runtime-claim");
        let key = b"/watch/Album Title".to_vec();
        let watching = handle.subscribe_candidate_runtime(key.clone());
        assert!(pull_until(&handle, &watching, |_| true).is_none(), "nothing runs yet");

        handle.runtime.block_on(
            handle.services.claim_candidate_for_import_for_test(&key.clone().into(), "import-1"),
        );

        let claimed = pull_until(&handle, &watching, Option::is_some)
            .expect("a claimed candidate is in flight");
        let import = claimed.import.clone().expect("a claimed candidate has an import in flight");
        assert!(
            matches!(import.step, crate::types::BridgeImportStep::Preparing { .. }),
            "a claim is the queued step, got {:?}",
            import.step
        );

        // A subscriber opening mid-import reads what is running.
        let joining = handle.subscribe_candidate_runtime(key.clone());
        assert!(
            pull_until(&handle, &joining, |_| true).and_then(|runtime| runtime.import).is_some(),
            "a late subscriber reads the running import"
        );

        // The import fails as the worker fails it: its failure ends the
        // claim, then announces it.
        handle.runtime.block_on(
            handle.services.end_candidate_import_for_test(&key.clone().into(), "import-1"),
        );

        // A failed import is a row by the time it is announced, so the key
        // simply stops being in flight.
        pull_until(&handle, &watching, Option::is_none);

        // Cancelling ends the pulls: a pull after it answers that it was
        // cancelled.
        handle.runtime.block_on(watching.clone().cancel()).unwrap();
        assert!(matches!(
            handle.runtime.block_on(watching.clone().next()),
            Err(crate::types::BridgeError::Cancelled)
        ));
    }
}

/// Dropping a handle with its library syncing closes the library before the
/// runtime ends: sync stops, the manager's tasks end and the store closes, so
/// nothing of the library is left running for the runtime's shutdown to cancel
/// under it.
#[cfg(desktop)]
#[test]
fn dropping_a_handle_closes_its_library_before_its_runtime_ends() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("test runtime");
    let directory = tempfile::TempDir::new().expect("library directory");
    let (library, storage) = runtime.block_on(bae_library::test_libraries::storage_library(
        directory.path(),
        bae_catalogs::providers::Providers::offline(),
        bae_util::http::Http::for_test(),
    ));
    runtime.block_on(async {
        bae_library::test_libraries::connect_storage(&library, &storage).await;
        library.set_sync_paused(false).await.expect("resume sync");
        library.attach_and_start_sync_at_startup().await;
        let mut status = library.subscribe_sync_status();
        bae_library::followed::value_matching(&mut status, |value| {
            value.running() && value.coven.state == coven::SyncState::Synced
        })
        .await;
    });
    let database = library.database().clone();
    let services = runtime.block_on(async move {
        bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start")
    });
    let app = super::AppHandle::start(
        services,
        runtime,
        bae_diagnostics::Diagnostics::noop(),
        test_host(directory.path()),
    )
    .expect("the bridge handle starts");

    drop(app);

    let read = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime to read the library from")
        .block_on(database.unsent_writes_for_test());
    assert!(
        matches!(
            read,
            Err(bae_db::DbError::Coven(coven::CovenError::Database(coven::DbError::StoreClosed)))
        ),
        "the library's store is closed: {read:?}"
    );
}

/// A handle whose last call ends on the library's own runtime is released
/// there, and its library closes on a thread of its own. Opening the
/// library again waits for that close, so it never finds the store still
/// held, and the store the handle had open ends closed.
#[test]
fn reopening_a_library_released_on_its_runtime_waits_for_its_close() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = caller
        .block_on(host.clone().create_library(
            Some("Released Library".into()),
            crate::BridgeLibrarySetup::new(host.clone()),
        ))
        .unwrap();
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    drop(opened);
    let services = app.services.clone();

    let runtime = app.runtime.clone();
    runtime
        .block_on(async move { tokio::spawn(async move { drop(app) }).await })
        .expect("the handle is released on its runtime");
    let opening = crate::BridgeLibraryOpening::new(host.clone());
    let reopened = crate::init_app(id, 50, false, host, opening).expect("the library opens again");

    let read = caller.block_on(services.library_name());
    assert!(
        matches!(
            read,
            Err(bae_library::LibraryError::Database(bae_db::DbError::Coven(
                coven::CovenError::Database(coven::DbError::StoreClosed)
            )))
        ),
        "the released handle's store is closed: {read:?}"
    );
    reopened.close_library(None).unwrap();
}

/// A library refusal crosses in the class core gave it: a blank credit name
/// is refused as the edit it is, not shown as a database failure.
#[cfg(feature = "desktop")]
#[test]
fn a_library_refusal_crosses_in_the_class_core_gave_it() {
    let (handle, _root) = fresh_bridge_handle("library-refusal-class");
    let result = handle.runtime.block_on(async {
        Arc::clone(&handle)
            .add_artist_credit_name("missing-artist".to_string(), "   ".to_string())
            .await
    });
    assert!(
        matches!(
            result,
            Err(crate::types::BridgeError::Diagnostic {
                category: crate::types::BridgeErrorCategory::Import,
                ..
            })
        ),
        "{result:?}"
    );
}
