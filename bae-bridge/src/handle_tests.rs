#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use bae_records::{DbArtist, DbWork};
use bae_views::album_detail::{
    ComposerDetail, ComposerDetailItem, ComposerDetailRow, ComposerHeading, WorkDetail,
    WorkReleaseSummary,
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
        artist_line: bae_types::credits::CreditLine::of_name("Artist"),
        display_artist: None,
        position_text: "1".to_string(),
        position,
        works: Vec::new(),
        my_stars: None,
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
                shows_time: true,
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
        artist_line: bae_types::credits::CreditLine::of_name("Artist"),
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
            artist_line: bae_types::credits::CreditLine::of_name("Artist"),
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

/// Pull `next` until `wanted` takes a value, failing the test if none ever
/// does.
pub(super) fn pull_until<T, F>(
    handle: &super::AppHandle,
    next: impl Fn() -> F,
    wanted: impl Fn(&T) -> bool,
) -> T
where
    F: std::future::Future<Output = Result<T, crate::types::BridgeError>>,
{
    handle.runtime.block_on(async {
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let value = next().await.expect("the pull reads");
                if wanted(&value) {
                    return value;
                }
            }
        })
        .await
        .expect("the values get there")
    })
}

/// A handle over a fresh, empty library, built the way the app builds one.
pub(super) fn fresh_bridge_handle(test_name: &str) -> TestHandle {
    let (runtime, library, root) = fresh_test_library(test_name);
    start_test_handle(runtime, library, root)
}

/// A started handle together with the directory its library lives in. The
/// handle field comes first so it drops, and shuts down, before the directory
/// is removed.
pub(super) struct TestHandle {
    handle: Arc<super::AppHandle>,
    root: tempfile::TempDir,
}

impl TestHandle {
    /// The started handle.
    pub(super) fn handle(&self) -> &Arc<super::AppHandle> {
        &self.handle
    }

    /// The directory holding the handle's library and bae directory.
    pub(super) fn root(&self) -> &std::path::Path {
        self.root.path()
    }
}

/// A fresh library opened in its own temporary directory (named for
/// `test_name`, unique per call so concurrent test processes never share
/// one), on the runtime a handle over it runs on. The directory goes when the
/// returned `TempDir` drops.
pub(super) fn fresh_test_library(
    test_name: &str,
) -> (tokio::runtime::Runtime, bae_library::test_libraries::TestLibrary, tempfile::TempDir) {
    let root = tempfile::Builder::new()
        .prefix(&format!("bae-bridge-{test_name}-"))
        .tempdir()
        .expect("test library dir");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("test runtime");
    let app_dir = bae_app_dir::AppDir::under_home(root.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let sources = bae_library::test_libraries::TestSources::system();
    let handle = runtime
        .block_on(
            bae_store::configure_store(sources.builder(&installation), bae_library::SCHEMA)
                .create_store("Test Library", "Test Device", &[], |_, _| Ok(())),
        )
        .expect("test store");
    let library = runtime.block_on(bae_library::test_libraries::test_library_from_handle(
        handle,
        root.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
        &sources,
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

/// A library named `name` created through `host`, run on `caller`, as a host
/// app creates one.
pub(crate) fn create_opened_library(
    host: &Arc<crate::BridgeHost>,
    caller: &tokio::runtime::Runtime,
    name: &str,
) -> Arc<crate::OpenedLibrary> {
    caller
        .block_on(host.clone().create_library(
            Some(name.into()),
            "Creator".into(),
            "Test Device".into(),
            crate::BridgeLibrarySetup::new(host.clone()),
        ))
        .unwrap()
}

/// Start a handle over `library` on `runtime`, for a host over the bae
/// directory under `root`, as the app starts one.
pub(super) fn start_test_handle(
    runtime: tokio::runtime::Runtime,
    library: bae_library::test_libraries::TestLibrary,
    root: tempfile::TempDir,
) -> TestHandle {
    let services = runtime.block_on(async move {
        bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start")
    });
    let handle = Arc::new(
        crate::host::start_handle(test_host(root.path()), bae_diagnostics::Diagnostics::noop())(
            services, runtime,
        )
        .expect("the bridge handle starts"),
    );
    TestHandle { handle, root }
}

#[test]
fn album_browse_subscription_pulls_count_before_any_windows() {
    let handle = fresh_bridge_handle("album-browse-empty-windows");
    let subscription = handle.handle().subscribe_album_browse(Vec::new(), false);

    let snapshot = handle
        .handle()
        .runtime
        .block_on(subscription.clone().next())
        .expect("album browse snapshot");

    assert_eq!(snapshot.total_count, 0);
    assert!(snapshot.windows.is_empty());
    assert_eq!(snapshot.cause, crate::types::BridgeLiveQueryCause::Request);
    assert_eq!(snapshot.request_revision, 0);

    subscription
        .set_windows(vec![crate::types::BridgeLibraryPageWindow { offset: 0, limit: 50 }])
        .expect("request first album window");
    let requested = handle
        .handle()
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

#[test]
fn cancelling_album_browse_finishes_pending_next() {
    let handle = fresh_bridge_handle("album-browse-cancel");
    let subscription = handle.handle().subscribe_album_browse(Vec::new(), false);
    handle
        .handle()
        .runtime
        .block_on(subscription.clone().next())
        .expect("initial album browse snapshot");

    let result = handle.handle().runtime.block_on(async {
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

#[test]
fn save_sync_config_survives_the_uniffi_worker_stack() {
    const CHILD: &str = "BAE_BRIDGE_SAVE_SYNC_CONFIG_STACK_CHILD";

    if std::env::var_os(CHILD).is_some() {
        let runtime_drop_panicked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let panic_flag = runtime_drop_panicked.clone();
        std::panic::set_hook(Box::new(move |_| {
            panic_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        let handle = fresh_bridge_handle("save-sync-config-stack");
        let caller_runtime = tokio::runtime::Runtime::new().expect("create caller runtime");
        std::thread::Builder::new()
            .name("uniffi-worker".to_string())
            .stack_size(544 * 1024)
            .spawn(move || {
                caller_runtime
                    .block_on(Arc::clone(handle.handle()).save_sync_config(
                        crate::types::BridgeSaveSyncConfig {
                            bucket: "bucket".to_string(),
                            region: "us-east-1".to_string(),
                            endpoint: Some("http://127.0.0.1:1".to_string()),
                            access_key: "access-key".to_string(),
                            secret_key: "secret-key".to_string(),
                            profile: None,
                        },
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

#[test]
fn enqueue_export_missing_release_does_not_panic() {
    let handle = fresh_bridge_handle("enqueue-export-missing-release");
    let result = handle.handle().runtime.block_on(async {
        Arc::clone(handle.handle())
            .enqueue_export(
                "missing-release".to_string(),
                coven::path_bytes(&handle.root().join("exports")),
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

/// The text pools the run in flight last read cross with its runtime, for
/// the search form's autocomplete; a queued key carries none.
#[cfg(desktop)]
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
    let value = crate::types::BridgeUploadProgress::from_core(bae_records::UploadProgress {
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
        composer: DbComposerSummary {
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
            work: DbWorkSummary {
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
    let work = DbWorkSummary {
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

/// A start that failed crosses as sync failed, its cause the category line
/// the UI localizes, with Retry now while sync runs.
#[test]
fn a_failed_start_crosses_as_sync_failed_with_its_category_line() {
    let snapshot = bae_sync_status::SyncStatusSnapshot {
        error: Some(bae_diagnostics::UiError::internal("sync cycle: pull Store commits")),
        coven: coven::SyncStatus { state: coven::SyncState::Synced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: None,
        provider: Some(coven::CloudProvider::S3),
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.shown.status, crate::types::BridgeSyncStatusWord::SyncFailed);
    assert_eq!(bridge.shown.tone, crate::types::BridgeSyncTone::Danger);
    assert_eq!(
        bridge.shown.cause.map(|cause| match cause {
            crate::types::BridgeSyncCause::Message { message, .. } => message.key,
            other => panic!("expected a catalog message, got {other:?}"),
        }),
        Some("core.error.category.internal".to_string())
    );
    assert_eq!(bridge.shown.fix, Some(crate::types::BridgeSyncFix::RetryNow));
}

/// A healthy cycle crosses as Synced, green, with nothing to say or do and no
/// time of its last sync.
#[test]
fn a_healthy_sync_status_crosses_as_synced() {
    let snapshot = bae_sync_status::SyncStatusSnapshot {
        error: None,
        coven: coven::SyncStatus {
            state: coven::SyncState::Synced,
            last_synced: Some(std::time::UNIX_EPOCH),
        },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: None,
        provider: Some(coven::CloudProvider::Dropbox),
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.shown.status, crate::types::BridgeSyncStatusWord::Synced);
    assert_eq!(bridge.shown.tone, crate::types::BridgeSyncTone::Success);
    assert_eq!(bridge.shown.cause, None);
    assert_eq!(bridge.shown.fix, None);
    assert_eq!(bridge.shown.last_synced, None);
}

/// A setup that ended undone crosses with the operation that dismisses it
/// and its own line; one running crosses as the status naming it.
#[test]
fn a_storage_setup_crosses_undone_or_running() {
    let snapshot = |storage_setup| bae_sync_status::SyncStatusSnapshot {
        error: None,
        coven: coven::SyncStatus { state: coven::SyncState::NotSynced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: Some(storage_setup),
        provider: None,
    };
    let undone = bae_sync_status::StorageSetupState::Undone {
        operation: coven::OperationId(7),
        why: bae_diagnostics::UiError::diagnostic(
            bae_diagnostics::UiErrorCategory::StorageSetupUndone,
            "bae stopped before the setup could finish",
        ),
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot(undone));
    let Some(crate::types::BridgeUndoneStorageSetup {
        operation: 7,
        error: crate::types::BridgeError::Diagnostic { category, .. },
    }) = bridge.undone_setup
    else {
        panic!("the undone setup crosses with its operation and why");
    };
    assert_eq!(category, crate::types::BridgeErrorCategory::StorageSetupUndone);
    assert_eq!(bridge.shown.status, crate::types::BridgeSyncStatusWord::NotSynced);
    assert!(!bridge.setup_running);

    let running = snapshot(bae_sync_status::StorageSetupState::Running);
    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(running);
    assert!(bridge.undone_setup.is_none());
    assert!(bridge.setup_running);
    assert_eq!(bridge.shown.status, crate::types::BridgeSyncStatusWord::SettingUpStorage);
}

/// A start that failed with sync not running crosses with Reconnect offered
/// beside why.
#[test]
fn a_failed_start_crosses_offering_reconnect() {
    let snapshot = bae_sync_status::SyncStatusSnapshot {
        error: Some(bae_diagnostics::UiError::internal("the store key is missing")),
        coven: coven::SyncStatus { state: coven::SyncState::Synced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::NotSyncing,
        storage_setup: None,
        provider: Some(coven::CloudProvider::S3),
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.shown.status, crate::types::BridgeSyncStatusWord::SyncFailed);
    assert_eq!(bridge.shown.fix, Some(crate::types::BridgeSyncFix::Reconnect));
}

/// A pass the provider failed crosses as sync failed with the provider's
/// error told as it told it, and the pending record beside it.
#[test]
fn a_provider_failure_crosses_as_sync_failed_with_the_providers_words() {
    let failure = coven::StorageFailure::Protocol;
    let snapshot = bae_sync_status::SyncStatusSnapshot {
        error: None,
        coven: coven::SyncStatus {
            state: coven::SyncState::Failed(coven::SyncCause::Pass(coven::LastPass::Failed {
                failure: coven::StatusFailure::Storage(failure),
                error: "storage refused the pass".into(),
            })),
            last_synced: Some(std::time::UNIX_EPOCH),
        },
        pending: vec![bae_sync_status::PendingItem::before_open(coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            coven::PendingReason::Storage {
                provider: coven::CloudProvider::S3,
                failure,
                answered: true,
            },
        ))],
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: None,
        provider: Some(coven::CloudProvider::S3),
    };

    let bridge = crate::types::BridgeSyncStatusSnapshot::from_core(snapshot);

    assert_eq!(bridge.pending.len(), 1);
    let Some(crate::types::BridgeSyncCause::Message { message, .. }) = bridge.shown.cause else {
        panic!("a failure says why in a catalog message");
    };
    assert_eq!(message.key, "core.sync.cause.provider_error");
    assert_eq!(
        message.args,
        vec![
            crate::types::BridgeMessageArg { name: "provider".into(), value: "S3".into() },
            crate::types::BridgeMessageArg {
                name: "detail".into(),
                value: "storage refused the pass".into()
            },
        ]
    );
    assert_eq!(bridge.shown.last_synced, Some(std::time::UNIX_EPOCH));
}

/// One candidate's runtime pulled end to end: a claim crosses as its import
/// in flight, a subscriber opening mid-import reads it, and the import
/// failing leaves nothing in flight.
#[cfg(desktop)]
mod candidate_runtime {
    use super::*;

    #[test]
    fn a_claimed_import_crosses_as_in_flight_and_leaves_when_it_fails() {
        let handle = fresh_bridge_handle("candidate-runtime-claim");
        let key = b"/watch/Album Title".to_vec();
        let watching = handle.handle().subscribe_candidate_runtime(key.clone());
        assert!(
            pull_until(handle.handle(), || watching.clone().next(), |_| true).is_none(),
            "nothing runs yet"
        );

        handle.handle().runtime.block_on(
            handle
                .handle()
                .services
                .claim_candidate_for_import_for_test(&key.clone().into(), "import-1"),
        );

        let claimed = pull_until(handle.handle(), || watching.clone().next(), Option::is_some)
            .expect("a claimed candidate is in flight");
        let import = claimed.import.clone().expect("a claimed candidate has an import in flight");
        assert!(
            matches!(import.step, crate::types::BridgeImportStep::Preparing { .. }),
            "a claim is the queued step, got {:?}",
            import.step
        );

        // A subscriber opening mid-import reads what is running.
        let joining = handle.handle().subscribe_candidate_runtime(key.clone());
        assert!(
            pull_until(handle.handle(), || joining.clone().next(), |_| true)
                .and_then(|runtime| runtime.import)
                .is_some(),
            "a late subscriber reads the running import"
        );

        // The import fails as the worker fails it: its failure ends the
        // claim, then announces it.
        handle.handle().runtime.block_on(
            handle.handle().services.end_candidate_import_for_test(&key.clone().into(), "import-1"),
        );

        // A failed import is a row by the time it is announced, so the key
        // simply stops being in flight.
        pull_until(handle.handle(), || watching.clone().next(), Option::is_none);

        // Cancelling ends the pulls: a pull after it answers that it was
        // cancelled.
        handle.handle().runtime.block_on(watching.clone().cancel()).unwrap();
        assert!(matches!(
            handle.handle().runtime.block_on(watching.clone().next()),
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
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    ));
    runtime.block_on(async {
        bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
            .await;
        library.store().set_uploads_paused(false).await.expect("resume sync");
        library.manager().attach_and_start_sync_at_startup().await;
        let mut status = library.manager().subscribe_sync_status();
        bae_library::followed::value_matching(&mut status, |value| {
            value.running() && value.coven.state == coven::SyncState::Synced
        })
        .await;
    });
    let db = library.store().clone();
    let app = start_test_handle(runtime, library, directory);

    drop(app);

    let read = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime to read the library from")
        .block_on(db.unsent_writes_for_test());
    assert!(
        matches!(
            &read,
            Err(bae_store::DbError::Coven(error))
                if matches!(**error, coven::CovenError::Database(coven::DbError::StoreClosed))
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
    let host = test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = create_opened_library(&host, &caller, "Released Library");
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
            &read,
            Err(bae_library::LibraryError::Database(bae_store::DbError::Coven(error)))
                if matches!(**error, coven::CovenError::Database(coven::DbError::StoreClosed))
        ),
        "the released handle's store is closed: {read:?}"
    );
    reopened.close_library(None).unwrap();
}

/// A library refusal crosses in the class core gave it: a blank credit name
/// is refused as the edit it is, not shown as a database failure.
#[cfg(desktop)]
#[test]
fn a_library_refusal_crosses_in_the_class_core_gave_it() {
    let handle = fresh_bridge_handle("library-refusal-class");
    let result = handle.handle().runtime.block_on(async {
        Arc::clone(handle.handle())
            .add_artist_credit_name("missing-artist".to_string(), "   ".to_string())
            .await
    });
    assert!(
        matches!(
            result,
            Err(crate::types::BridgeError::Diagnostic {
                category: crate::types::BridgeErrorCategory::InvalidEdit {
                    reason: crate::types::BridgeValidationReason::EmptyArtistName
                },
                ..
            })
        ),
        "{result:?}"
    );
}
