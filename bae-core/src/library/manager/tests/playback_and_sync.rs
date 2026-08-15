#[tokio::test]
async fn discogs_operations_withheld_when_rejected() {
    use crate::config::DiscogsValidation;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-withheld-test").await;
    manager
        .set_discogs_key(
            "f7228aaf-52b3-40ea-8526-a7e8aa0bf5da",
            DiscogsValidation::Valid,
        )
        .unwrap();

    assert!(
        manager.discogs_available_for_test().unwrap(),
        "a Valid key is served"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Unvalidated)
        .unwrap();
    assert!(
        manager.discogs_available_for_test().unwrap(),
        "an Unvalidated key is served optimistically"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Rejected)
        .unwrap();
    assert!(
        !manager.discogs_available_for_test().unwrap(),
        "a Rejected key is withheld"
    );
}

/// A key present in the keyring but absent from config (the residue a torn write
/// or external keyring tampering can leave) is not served: a usable key requires
/// both stores to agree it exists.
#[tokio::test]
async fn discogs_operations_withheld_when_config_has_no_key() {
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-orphan-key").await;

    // Keyring bytes present, config untouched (still `None`).
    manager
        .database
        .set_host_secret(crate::keys::DISCOGS_API_KEY, "orphan-key")
        .unwrap();

    assert_eq!(manager.discogs_validation(), None);
    assert!(
        !manager.discogs_available_for_test().unwrap(),
        "a keyring key with no config hint is not served",
    );
}

/// `set_discogs_key` and `clear_discogs_key` move both durable stores together.
#[tokio::test]
async fn set_and_clear_discogs_key_move_both_stores() {
    use crate::config::DiscogsValidation;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-atomic").await;

    manager
        .set_discogs_key("the-key", DiscogsValidation::Valid)
        .unwrap();
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));
    assert_eq!(
        manager
            .database
            .host_secret(crate::keys::DISCOGS_API_KEY)
            .unwrap()
            .as_deref(),
        Some("the-key"),
    );
    assert!(manager.discogs_available_for_test().unwrap());

    manager.clear_discogs_key().unwrap();
    assert_eq!(manager.discogs_validation(), None);
    assert_eq!(
        manager
            .database
            .host_secret(crate::keys::DISCOGS_API_KEY)
            .unwrap(),
        None
    );
    assert!(!manager.discogs_available_for_test().unwrap());
}

/// Revalidation surfaces the config-says-stored/keyring-empty mismatch as an
/// error, not a swallowed warning — the one torn state our writes can't produce
/// but external tampering can.
#[tokio::test]
async fn revalidate_errors_when_config_claims_a_key_the_keyring_lacks() {
    use crate::config::DiscogsValidation;

    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-revalidate-torn").await;
    // Config claims an Unvalidated key; the keyring has none — the torn state.
    manager
        .config_handle
        .update(|c| c.discogs = Some(DiscogsValidation::Unvalidated))
        .unwrap();

    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    assert!(
        handle.revalidate_discogs_token().await.is_err(),
        "a stored-but-keyless config must fail revalidation, not warn and continue",
    );
}

#[tokio::test]
async fn discogs_validation_signals_confirm_and_reject() {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsKeySignal;
    let (manager, _temp_dir) = setup_test_manager().await;
    // A success confirms a stored Unvalidated key.
    manager
        .config_handle
        .update(|c| c.discogs = Some(DiscogsValidation::Unvalidated))
        .unwrap();
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Rejected);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success does NOT flip an already-Rejected key back to Valid.
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    manager
        .set_discogs_validation(DiscogsValidation::Valid)
        .unwrap();
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));
}

/// Aborting the transfer driver must remove its action from the value stream so
/// subscribers cannot retain an in-flight transfer after its future is gone.
#[tokio::test]
async fn aborted_transfer_clears_the_streamed_action() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut values = manager.subscribe_transfer_values();

    // A channel whose sender we hold open: `drive_transfer` parks in
    // `rx.recv().await` forever, so the only way out is the abort below.
    let (tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();

    let driver = manager.clone();
    let handle = tokio::spawn(async move {
        let result = driver
            .drive_transfer(REL_ABORT, ReleaseStorageAction::Pin, progress_rx)
            .await;
        panic!("the parked driver must only exit by abort, returned {result:?}");
    });

    tx.send(crate::storage::transfer::TransferProgress::Started)
        .expect("the transfer driver is listening");
    values
        .changed()
        .await
        .expect("the active transfer value must arrive");
    assert_eq!(
        values.borrow().get(REL_ABORT),
        Some(&ReleaseStorageAction::Pin)
    );

    handle.abort();
    let join = handle.await;
    assert!(
        join.expect_err("the parked driver can only exit by abort")
            .is_cancelled(),
        "the driver must end by cancellation, not a panic"
    );
    values
        .changed()
        .await
        .expect("the cleared transfer value must arrive");
    assert!(!values.borrow().contains_key(REL_ABORT));
}

/// Seed an album with two releases, each holding two tracks with explicit
/// side/track-number so the library order is deterministic. Track ids are
/// chosen so the `(release_id, side, track_number, id)` order is unambiguous.
async fn seed_two_release_library(manager: &LibraryManager) -> (String, String) {
    use crate::db::DbTrack;
    let mut album = create_test_album();
    album.id = "1250a7bb-41ed-4500-8ab4-04f5d3461e30".to_string();
    let mut rel1 = create_test_release(&album.id);
    rel1.id = REL_1.to_string();
    let mut rel2 = create_test_release(&album.id);
    rel2.id = REL_2.to_string();
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&rel1).await.unwrap();
    manager.database.insert_release(&rel2).await.unwrap();

    let track = |release_id: &str, id: &str, side: i32, number: i32| {
        let t = DbTrack {
            side,
            ..DbTrack::new_test(release_id, id, "Track Title", Some(number))
        };
        let database = &manager.database;
        async move { database.insert_track(&t).await.unwrap() }
    };
    // rel-1: side 1 then side 2; rel-2: two side-1 tracks.
    track(REL_1, "48ae00a1-d7a5-443c-8240-f999fc4ddfcc", 1, 1).await;
    track(REL_1, "48ae03a1-d7a5-4955-8240-fc99fc4de4e5", 2, 1).await;
    track(REL_2, "cc4180bc-58f5-456f-8116-f9b2099f5b7f", 1, 1).await;
    track(REL_2, "cc4181bc-58f5-4722-8116-fab2099f5d32", 1, 2).await;
    (rel1.id, rel2.id)
}

/// `get_all_track_ids` returns every library track in the deterministic base
/// order — by release, then side, track number, id — so a shuffle seed
/// permutes a stable list.
#[tokio::test]
async fn test_get_all_track_ids_returns_library_in_base_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_two_release_library(&manager).await;
    let all = manager.get_all_track_ids().await.unwrap();
    assert_eq!(
        all,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
            "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
            "cc4181bc-58f5-4722-8116-fab2099f5d32"
        ]
    );
}

/// The two track-id queries the service's source dispatcher routes between:
/// a release's own ordered tracks (`get_track_ids`) vs the whole library
/// (`get_all_track_ids`). The library is the union of the releases, so a
/// release's tracks are a strict subset of it.
#[tokio::test]
async fn test_release_and_library_track_id_queries_return_their_sets() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let (rel1, _rel2) = seed_two_release_library(&manager).await;
    let release_tracks = manager.get_track_ids(&rel1).await.unwrap();
    assert_eq!(
        release_tracks,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5"
        ]
    );
    let library_tracks = manager.get_all_track_ids().await.unwrap();
    assert_eq!(
        library_tracks,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
            "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
            "cc4181bc-58f5-4722-8116-fab2099f5d32"
        ]
    );
    assert!(release_tracks.iter().all(|t| library_tracks.contains(t)));
}

/// A `playback_state` row carrying a library source survives save → load: the
/// `source` column stores the library sentinel and reads back unchanged, and a
/// release row stores/reads its id. (Decoding the sentinel back to the source
/// enum is covered in `playback::persisted`.)
#[tokio::test]
async fn test_playback_state_source_column_round_trips_both_kinds() {
    use crate::db::{DbPlaybackContext, DbPlaybackState};
    use crate::playback::source_to_str;
    use crate::playback::ContextSource;
    let (manager, _temp_dir) = setup_test_manager().await;

    for source in [
        ContextSource::Library,
        ContextSource::Release(REL_1.to_string()),
    ] {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: source_to_str(&source),
                shuffled: true,
            }),
            manual: "[]".to_string(),
            repeat: "off".to_string(),
            current_track_id: None,
            position_ms: None,
            volume: 1.0,
            is_muted: false,
        };
        manager.save_playback_state(&row).await.unwrap();
        let crate::db::LoadedPlaybackState::Present(loaded) =
            manager.load_playback_state().await.unwrap()
        else {
            panic!("a saved row loads");
        };
        assert_eq!(
            loaded.context.unwrap().source,
            source_to_str(&source),
            "the source column round-trips for {source:?}"
        );
    }
}

/// Each sync/membership/cloud-setup failure class carries a distinct diagnostic
/// category to the bridge, so the UI shows different messages for bad
/// credentials, an unreachable backend, a keyring failure, a config-write
/// failure, and a membership-chain failure. Builds the exact coven boundary
/// errors these flows return and asserts the class the bridge reads.
#[test]
fn setup_failure_classes_map_to_distinct_categories() {
    use crate::ui::UiErrorCategory as C;

    let cases: Vec<(LibraryError, C)> = vec![
        (
            coven::CloudHomeError::Configuration("rejected credentials".into()).into(),
            C::Credentials,
        ),
        (
            coven::CloudHomeError::NotFound("missing bucket".into()).into(),
            C::Credentials,
        ),
        (
            coven::CloudHomeError::Transport("unreachable endpoint".into()).into(),
            C::Network,
        ),
        (
            LibraryError::CloudSetup(coven::CloudHomeSetupError::Connection(Box::new(
                coven::SyncError::CloudHome(coven::CloudHomeError::Configuration(
                    "oauth denied".into(),
                )),
            ))),
            C::Credentials,
        ),
        (
            coven::KeyError::Custody {
                operation: "write keyring",
                source: Box::new(std::io::Error::other("keyring write failed")),
            }
            .into(),
            C::Keyring,
        ),
        (
            crate::config::ConfigError::Config("config write failed".into()).into(),
            C::Config,
        ),
        (
            coven::SyncError::Key(coven::KeyError::Custody {
                operation: "write keyring",
                source: Box::new(std::io::Error::other("keyring write failed")),
            })
            .into(),
            C::Keyring,
        ),
        (
            coven::SyncError::CloudHome(coven::CloudHomeError::Transport("t".into())).into(),
            C::Network,
        ),
        // `SyncError::Membership` maps to `C::Membership` (see `sync_category`),
        // but its payload `MembershipOpsError` is no longer part of coven's
        // curated public API, so a host test can't fabricate that variant.
        (coven::SyncError::NotConfigured.into(), C::Internal),
        (
            LibraryError::Storage("pin ended without completion".into()),
            C::Internal,
        ),
        (
            LibraryError::Validation("library name cannot be empty".into()),
            C::Config,
        ),
    ];

    for (error, expected) in &cases {
        assert_eq!(error.category(), *expected, "{error}");
    }
}

/// A coven-typed sync error propagates through the sync controller and the
/// manager forwarder without being flattened to a string: an unconfigured
/// library surfaces `SyncError::NotConfigured` intact, and its class is Internal.
#[tokio::test]
async fn get_members_on_unconfigured_library_propagates_typed_sync_error() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let Err(err) = manager.get_members().await else {
        panic!("no cloud home is connected, so get_members must fail");
    };
    assert!(
        matches!(err, LibraryError::Sync(coven::SyncError::NotConfigured)),
        "expected a typed SyncError::NotConfigured, got {err:?}"
    );
    assert_eq!(err.category(), crate::ui::UiErrorCategory::Internal);
}

// ── Queue windowing tests ───────────────────────────────────────────

/// Insert one release with `count` sequentially-numbered tracks
/// (`track-0`..`track-{count-1}`); return their ids in track order.
async fn seed_release_tracks(manager: &LibraryManager, count: usize) -> Vec<String> {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let mut track_ids = Vec::with_capacity(count);
    for i in 0..count {
        let track_id = bae_test_support::test_uuid(&format!("track-{i}"));
        let track = crate::db::DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track Title {i}"),
            Some(i as i32),
        );
        manager.database.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }
    track_ids
}

/// A `Library`-source context projection whose upcoming tail is `track_ids`,
/// in order, each wrapped in a freshly-minted per-instance entry id.
fn context_projection_over(track_ids: &[String]) -> crate::playback::ContextProjection {
    crate::playback::ContextProjection {
        source: crate::playback::ContextSource::Library,
        shuffled: false,
        upcoming: track_ids
            .iter()
            .enumerate()
            .map(|(i, t)| crate::playback::QueueEntry {
                id: crate::playback::QueueEntryId(format!("ctx-{i}")),
                track_id: t.clone(),
            })
            .collect(),
    }
}

/// `resolve_queue_projection` resolves only the first `QUEUE_UPCOMING_WINDOW`
/// entries of a library-scaled context tail, not the whole thing — the
/// windowing this feature exists for — while still reporting the tail's real
/// length via `upcoming_total` and preserving order.
#[tokio::test]
async fn resolve_queue_projection_windows_a_library_scaled_context_tail() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, crate::queue::QUEUE_UPCOMING_WINDOW + 50).await;

    let projection = crate::playback::PlaybackQueueProjection {
        manual: Vec::new(),
        context: Some(context_projection_over(&track_ids)),
        has_next: true,
        has_previous: false,
        revision: 7,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    let context = snapshot.context.expect("a context was set");

    assert_eq!(
        context.upcoming.len(),
        crate::queue::QUEUE_UPCOMING_WINDOW,
        "only the window is resolved, not the whole library-scaled tail"
    );
    assert_eq!(
        context.upcoming_total,
        track_ids.len() as u64,
        "upcoming_total reports the full tail length"
    );
    let resolved_track_ids: Vec<&str> = context
        .upcoming
        .iter()
        .map(|i| i.track_id.as_str())
        .collect();
    let expected: Vec<&str> = track_ids[..crate::queue::QUEUE_UPCOMING_WINDOW]
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(resolved_track_ids, expected, "the window preserves order");
    assert_eq!(
        snapshot.revision, 7,
        "the snapshot carries the projection's revision"
    );
}

/// A context tail shorter than the window resolves in full, and
/// `upcoming_total` still matches its real (smaller) length.
#[tokio::test]
async fn resolve_queue_projection_shorter_than_window_resolves_it_all() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 5).await;

    let projection = crate::playback::PlaybackQueueProjection {
        manual: Vec::new(),
        context: Some(context_projection_over(&track_ids)),
        has_next: false,
        has_previous: false,
        revision: 1,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    let context = snapshot.context.expect("a context was set");
    assert_eq!(context.upcoming.len(), 5);
    assert_eq!(context.upcoming_total, 5);
}

/// The manual lane is explicit and user-curated, not library-scaled — it
/// resolves in full even when it is larger than the context window.
#[tokio::test]
async fn resolve_queue_projection_resolves_manual_lane_in_full_regardless_of_window() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, crate::queue::QUEUE_UPCOMING_WINDOW + 10).await;

    let manual_count = crate::queue::QUEUE_UPCOMING_WINDOW + 3;
    let manual: Vec<crate::playback::QueueEntry> = track_ids[..manual_count]
        .iter()
        .enumerate()
        .map(|(i, t)| crate::playback::QueueEntry {
            id: crate::playback::QueueEntryId(format!("m{i}")),
            track_id: t.clone(),
        })
        .collect();

    let projection = crate::playback::PlaybackQueueProjection {
        manual,
        context: None,
        has_next: false,
        has_previous: false,
        revision: 0,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    assert_eq!(
        snapshot.manual.len(),
        manual_count,
        "the manual lane is never windowed"
    );
}

/// A failed opaque-home setup leaves both the proposed provider and generated
/// master key uncommitted. Coven owns that transaction; bae persists the returned
/// provider config only after Coven returns success.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_failed_connect_commits_neither_provider_nor_master_key() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let error = manager
        .use_cloudkit(crate::config::HomeStorage::Opaque)
        .await
        .expect_err("no CloudKit driver is installed, so the connect must fail");
    assert!(
        error.to_string().contains("CloudKit driver not provided"),
        "the failure must be the connect, not the key step: {error}"
    );

    assert_eq!(manager.get_config().cloud_home.provider, None);
    assert_eq!(
        manager
            .database
            .cloud_home_key_state(crate::config::HomeStorage::Opaque)
            .unwrap(),
        coven::CloudHomeKeyState::Locked,
        "a generated master key is not retained after connection failure"
    );
}

#[tokio::test]
async fn a_local_library_without_a_cloud_provider_does_not_require_a_master_key() {
    let (manager, _temp_dir) = setup_test_manager().await;

    assert_eq!(manager.get_config().cloud_home.provider, None);
    assert_eq!(
        manager.cloud_home_key_state().unwrap(),
        coven::CloudHomeKeyState::NotRequired,
    );
}

/// Cancelling a release's upload has to leave the durable state telling the truth:
/// the release is Local again, coven's make-Remote intent is gone, and its outbox
/// carries no pending uploads. That outbox — not a status column — is what a restart
/// reads to know an import is still uploading, and it is what the Processing pane
/// renders. bae used to keep a second copy of that fact in an `imports.status`
/// column that the cancel never touched (so it stayed `importing` forever) and that
/// nothing ever read back; this pins the fact to the place that is actually correct.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn cancelling_an_upload_leaves_no_in_flight_import_behind() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;

    manager.cancel_release_upload(&release.id).await.unwrap();

    assert!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap()
            .is_none(),
        "the cancel clears coven's make-Remote intent"
    );
    assert!(
        manager
            .database
            .queued_upload_count_for_test()
            .await
            .unwrap()
            == 0,
        "no upload is left queued, so nothing reads as still importing"
    );
    let after = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .expect("the release survives the cancel");
    assert!(!after.remote, "the cancelled release stays Local");
}
