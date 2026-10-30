#[tokio::test]
async fn discogs_operations_withheld_when_rejected() {
    use crate::config::DiscogsValidation;
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_discogs_key(
            "f7228aaf-52b3-40ea-8526-a7e8aa0bf5da",
            DiscogsValidation::Valid,
        ).await
        .unwrap();

    assert!(
        manager.discogs_available_for_test().unwrap(),
        "a Valid key is served"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Unvalidated).await
        .unwrap();
    assert!(
        manager.discogs_available_for_test().unwrap(),
        "an Unvalidated key is served optimistically"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Rejected).await
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
    let (manager, _temp_dir) = setup_test_manager().await;

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
    let (manager, _temp_dir) = setup_test_manager().await;

    manager
        .set_discogs_key("the-key", DiscogsValidation::Valid).await
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

    manager.clear_discogs_key().await.unwrap();
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

    let (manager, _temp_dir) = setup_test_manager().await;
    // Config claims an Unvalidated key; the keyring has none — the torn state.
    manager
        .config_handle
        .update_preferences(|prefs| prefs.discogs = Some(DiscogsValidation::Unvalidated)).await
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

/// A key Discogs cannot be asked about is stored unvalidated, and the save
/// says why; asking again fails with why rather than quietly leaving the same
/// state.
#[tokio::test]
async fn a_key_discogs_cannot_be_asked_about_is_stored_unchecked_with_why() {
    use crate::config::DiscogsValidation;
    use bae_catalogs::discogs::client::DiscogsError;
    use crate::import::{DiscogsSaveOutcome, ImportError};

    let (manager, _temp_dir) = setup_test_manager().await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    let outcome = handle
        .save_discogs_token("discogs-test-key")
        .await
        .expect("the key is stored though it went unchecked");
    let DiscogsSaveOutcome::Unvalidated(reason) = outcome else {
        panic!("an unreachable Discogs leaves the key unchecked: {outcome:?}");
    };
    assert!(
        reason.to_string().contains("tcp connect error"),
        "{reason}"
    );
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Unvalidated)
    );

    let error = handle
        .revalidate_discogs_token()
        .await
        .expect_err("a re-check Discogs cannot answer fails with why");
    assert!(
        matches!(error, ImportError::Discogs(DiscogsError::Transport(_))),
        "{error}"
    );
}

#[tokio::test]
async fn discogs_validation_signals_confirm_and_reject() {
    use crate::config::DiscogsValidation;
    use bae_catalogs::discogs::client::DiscogsKeySignal;
    let (manager, _temp_dir) = setup_test_manager().await;
    // A success confirms a stored Unvalidated key.
    manager
        .config_handle
        .update_preferences(|prefs| prefs.discogs = Some(DiscogsValidation::Unvalidated)).await
        .unwrap();
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted).await;
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Rejected).await;
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success does NOT flip an already-Rejected key back to Valid.
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted).await;
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    manager
        .set_discogs_validation(DiscogsValidation::Valid).await
        .unwrap();
    manager.record_discogs_validation_for_test(DiscogsKeySignal::Accepted).await;
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
            .drive_transfer(REL_ABORT, ReleaseStorageAction::Pin, progress_rx, None)
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

/// Seed an album with two releases, each holding two tracks inserted in
/// playing order, so the library order is deterministic.
async fn seed_two_release_library(manager: &LibraryManager) -> (String, String) {
    use crate::db::DbTrack;
    let mut album = create_test_album();
    album.id = "1250a7bb-41ed-4500-8ab4-04f5d3461e30".to_string();
    let mut rel1 = create_test_release(&album.id);
    rel1.id = REL_1.to_string();
    let mut rel2 = create_test_release(&album.id);
    rel2.id = REL_2.to_string();
    manager.database.insert_album(&album).await.unwrap();
    insert_release(manager, &rel1).await;
    insert_release(manager, &rel2).await;

    let track = |release_id: &str, id: &str, side: u8, number: u32| {
        let t = DbTrack {
            side: std::num::NonZeroU8::new(side).expect("sides count from one"),
            ..DbTrack::new_test(release_id, id, "Track Title", number)
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
/// order — by release, then side, track number, id.
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

/// A library play's lane holds every library track; unshuffled, the tracks
/// after the playing one are in the library's own order — by release, then
/// position — the order a release plays its own tracks in.
#[tokio::test]
async fn test_library_play_unshuffles_into_the_library_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let (rel1, _rel2) = seed_two_release_library(&manager).await;
    let tables = manager.playback_queue_tables();
    let library_order = [
        "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
        "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
        "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
        "cc4181bc-58f5-4722-8116-fab2099f5d32",
    ];

    tables
        .fill(crate::db::ContextFill::Release {
            release_id: rel1,
            start: crate::db::ReleaseStart::First,
        })
        .await
        .unwrap();
    assert_eq!(tables.context_order().await.unwrap(), library_order[..2]);

    tables.fill(crate::db::ContextFill::LibraryShuffled).await.unwrap();
    tables.set_shuffle(false).await.unwrap();
    let lane = tables.context_order().await.unwrap();
    let playing = lane[0].clone();
    let rest: Vec<&str> = library_order.iter().copied().filter(|t| *t != playing).collect();
    assert_eq!(lane[1..], rest[..]);
}

#[test]
fn setup_failure_classes_retain_the_actionable_category() {
    use crate::ui::UiErrorCategory as C;
    use coven::StorageFailure as F;
    let missing_entitlement = || {
        coven::KeyError::MissingKeychainEntitlement(
            keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
        )
    };
    let cases: Vec<(LibraryError, C)> = vec![
        (coven::StorageError::from(F::Authentication).into(), C::Storage(F::Authentication)),
        (coven::StorageError::from(F::ContainerNotFound).into(), C::Storage(F::ContainerNotFound)),
        (coven::StorageError::from(F::Network).into(), C::Storage(F::Network)),
        // A full store and a wrong clock say so, rather than blaming the
        // credentials or reporting an internal fault.
        (coven::StorageError::from(F::QuotaExceeded).into(), C::Storage(F::QuotaExceeded)),
        (coven::SyncError::Storage(F::ClockSkew.into()).into(), C::Storage(F::ClockSkew)),
        (coven::SyncError::Storage(F::Conflict.into()).into(), C::Storage(F::Conflict)),
        (coven::KeyError::PassphraseAuthentication.into(), C::Keyring),
        (coven::KeyError::ServiceNotRegistered.into(), C::Internal),
        (crate::config::ConfigError::Config("invalid preference".into()).into(), C::Config),
        (coven::SyncError::SecureStorage(coven::KeyError::Unsupported).into(), C::Keyring),
        (coven::SyncError::Storage(F::Network.into()).into(), C::Storage(F::Network)),
        (coven::SyncError::NoStorage.into(), C::Config),
        (coven::SyncError::Removed.into(), C::DeviceRemoved),
        (coven::KeyError::Left.into(), C::DeviceRemoved),
        (coven::SyncError::SecureStorage(coven::KeyError::Left).into(), C::DeviceRemoved),
        (missing_entitlement().into(), C::KeychainEntitlementMissing),
        (coven::SyncError::SecureStorage(missing_entitlement()).into(), C::KeychainEntitlementMissing),
        // Block Store's limits on Android say which one was met, not that the
        // keyring broke.
        (coven::KeyError::SavedEntriesFull.into(), C::SavedRestoreCodesFull),
        (
            coven::SyncError::SecureStorage(coven::KeyError::SavedEntryTooLarge).into(),
            C::SavedRestoreCodeTooLarge,
        ),
        (LibraryError::CloudSetup(coven::StorageSetupError::Removed), C::DeviceRemoved),
        (LibraryError::CloudUnlock(coven::StoreKeyUnlockError::Removed), C::DeviceRemoved),
        (
            LibraryError::CloudSetup(coven::StorageSetupError::UpdateRequired(
                coven::RequiredUpdate::AppSchema { version: 9 },
            )),
            C::SyncUpdateRequired,
        ),
        (
            LibraryError::CloudUnlock(coven::StoreKeyUnlockError::UpdateRequired(
                coven::RequiredUpdate::CovenFormat { version: 9 },
            )),
            C::SyncUpdateRequired,
        ),
        (LibraryError::Storage("pin ended without completion".into()), C::Internal),
        (LibraryError::Validation("library name cannot be empty".into()), C::Config),
    ];
    for (error, expected) in cases { assert_eq!(error.category(), expected, "{error}"); }
}

#[test]
fn every_storage_setup_failure_keeps_its_exact_user_action() {
    use coven::StorageFailure as F;
    for failure in [F::Authentication, F::PermissionDenied, F::ContainerNotFound,
        F::RegionMismatch, F::QuotaExceeded, F::InvalidConfiguration, F::Network] {
        let error = LibraryError::CloudSetup(coven::StorageSetupError::Storage(failure.into()));
        assert_eq!(error.category(), crate::ui::UiErrorCategory::Storage(failure));
    }
}

#[tokio::test]
async fn membership_reads_committed_local_state_without_a_provider() {
    let (manager, _temp) = setup_test_manager().await;
    let membership = manager.get_members().await.unwrap();
    assert!(!manager.is_sync_configured().await.unwrap());
    assert!(membership.members.iter().all(|member| member.is_self));
}

// ── Queue windowing tests ───────────────────────────────────────────

/// Insert one release with `count` sequentially-numbered tracks
/// (`track-0`..`track-{count-1}`); return their ids in track order.
async fn seed_release_tracks(manager: &LibraryManager, count: usize) -> Vec<String> {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    let mut track_ids = Vec::with_capacity(count);
    for i in 0..count {
        let track_id = bae_test_support::test_uuid(&format!("track-{i}"));
        let track = crate::db::DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track Title {i}"),
            u32::try_from(i + 1).expect("a test track number fits u32"),
        );
        manager.database.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }
    track_ids
}

/// Play the seeded release from its first track.
async fn play_seeded(manager: &LibraryManager, first_track: &str) {
    let tables = manager.playback_queue_tables();
    let release_id = manager
        .database
        .find_track_by_id(first_track)
        .await
        .unwrap()
        .unwrap()
        .release_id;
    tables
        .fill(crate::db::ContextFill::Release {
            release_id,
            start: crate::db::ReleaseStart::First,
        })
        .await
        .unwrap();
}

/// The context lane's tail reads in order after the playing track, however
/// long it is; the queue value says only that it has one.
#[tokio::test]
async fn the_context_tail_reads_in_order_after_the_playing_track() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 151).await;
    play_seeded(&manager, &track_ids[0]).await;

    let value = manager.queue_value().await.unwrap();
    assert!(value.context.is_some() && value.has_next);
    let upcoming: Vec<String> = manager
        .queue_upcoming()
        .await
        .unwrap()
        .into_iter()
        .map(|item| item.track_id)
        .collect();
    assert_eq!(upcoming, track_ids[1..].to_vec());
}

/// The manual lane is explicit and user-curated, not library-scaled — the
/// queue value carries it in full.
#[tokio::test]
async fn queue_value_resolves_manual_lane_in_full_regardless_of_window() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 110).await;
    let manual_count = 103;
    manager
        .playback_queue_tables()
        .add_manual(track_ids[..manual_count].to_vec(), crate::db::ManualPlace::End)
        .await
        .unwrap();

    assert_eq!(
        manager.queue_value().await.unwrap().manual.len(),
        manual_count,
        "the manual lane is never windowed"
    );
}

/// A reordered Up Next shows the new order with each entry in its place, a
/// repeated track once per entry.
#[tokio::test]
async fn a_reordered_queue_shows_each_entry_in_its_new_place() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 3).await;
    let tables = manager.playback_queue_tables();
    let mut queued = track_ids.clone();
    queued.push(track_ids[0].clone());
    tables
        .add_manual(queued, crate::db::ManualPlace::End)
        .await
        .unwrap();
    let entries: Vec<String> = manager
        .queue_value()
        .await
        .unwrap()
        .manual
        .iter()
        .map(|item| item.entry_id.clone())
        .collect();
    let id = |index: usize| entries[index].parse::<crate::playback::QueueEntryId>().unwrap();
    tables.reorder(id(2), Some(id(0))).await.unwrap();

    let shown: Vec<(String, String)> = manager
        .queue_value()
        .await
        .unwrap()
        .manual
        .into_iter()
        .map(|item| (item.entry_id, item.track_id))
        .collect();
    assert_eq!(
        shown,
        vec![
            (entries[2].clone(), track_ids[2].clone()),
            (entries[0].clone(), track_ids[0].clone()),
            (entries[1].clone(), track_ids[1].clone()),
            (entries[3].clone(), track_ids[0].clone()),
        ]
    );
}

#[tokio::test]
async fn failed_storage_setup_keeps_the_library_unconfigured() {
    let (manager, _temp) = setup_test_manager().await;
    manager.storage.set_online(false);
    manager.database.setup_s3_storage(manager.storage.config(), "Device", "key".into(), coven::SecretText::new("secret".into()))
        .await.expect_err("offline storage cannot be set up");
    assert!(!manager.is_sync_configured().await.unwrap());
    assert_eq!(manager.library_name().await.unwrap(), "Test Library");
}

#[tokio::test]
async fn a_local_library_does_not_need_storage_for_its_rows() {
    let (manager, _temp, album, release) = manager_with_release().await;
    assert!(!manager.is_sync_configured().await.unwrap());
    assert!(find_album(&manager, &album.id).await.is_some());
    assert!(find_release(&manager, &release.id).await.is_some());
}

#[tokio::test]
async fn reconnect_sync_refuses_a_library_with_no_provider() {
    let (manager, _temp) = setup_test_manager().await;
    let error = manager.reconnect_sync().await.unwrap_err();
    assert!(matches!(error, LibraryError::Sync(coven::SyncError::NoStorage)));
    assert!(manager.get_sync_status().error.is_none());
}

#[tokio::test]
async fn startup_and_retry_publish_the_native_connection_status() {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (manager, _temp) = setup_test_manager_with_clock(clock.clone()).await;
    connect_test_cloud(&manager).await;
    manager.storage.set_online(false);
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status_values();
    manager.attach_and_start_sync_at_startup().await;
    tokio::time::timeout(std::time::Duration::from_secs(20), values.wait_for(|value| matches!(value.status, coven::SyncStatus::Failed { .. } | coven::SyncStatus::Offline { .. }))).await.unwrap().unwrap();
    assert!(manager.is_sync_ready(), "the loop keeps the failed connection retryable");
    manager.storage.set_online(true);
    clock.set(std::time::UNIX_EPOCH + std::time::Duration::from_secs(60));
    manager.reconnect_sync().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), values.wait_for(|value| matches!(value.status, coven::SyncStatus::Synced { .. }))).await.unwrap().unwrap();
    manager.close().await.unwrap();
}

/// Android blocked the network while the phone was locked, so the pass found
/// no network and backs off. Once the network is back, a sync request still
/// waits out that backoff; the app's network report ends it and a pass runs
/// at once. The fixed clock never moves, so no backoff ends by itself.
#[tokio::test]
async fn a_network_report_ends_the_offline_backoff_and_syncs() {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (manager, _temp) = setup_test_manager_with_clock(clock).await;
    connect_test_cloud_with_sync_loop(&manager).await;
    let mut status = manager.database.subscribe_sync_status();
    manager.storage.set_online(false);
    manager.trigger_sync();
    tokio::time::timeout(std::time::Duration::from_secs(20), status.wait_for(|status| matches!(status, coven::SyncStatus::Offline { .. }))).await.unwrap().unwrap();
    manager.storage.set_online(true);
    let requests = manager.storage.request_trace().len();
    manager.trigger_sync();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(manager.storage.request_trace().len(), requests, "a sync request ran a pass inside the offline backoff");
    assert!(matches!(*status.borrow(), coven::SyncStatus::Offline { .. }));
    manager.network_changed();
    tokio::time::timeout(std::time::Duration::from_secs(20), status.wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. }))).await.unwrap().unwrap();
    manager.close().await.unwrap();
}

/// A launch whose connect failed records why in the failure's own category —
/// a keychain locked for this session reads as locked, not as an internal
/// failure —
/// and a retry that gets under way supersedes it: the recorded error clears as
/// the retry starts, so the status line reflects the attempt in flight instead
/// of the one that already failed.
#[tokio::test]
async fn reconnect_sync_clears_the_failure_it_retries() {
    let (manager, _temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    manager.database.disconnect_storage().await.unwrap();
    manager.start().unwrap();
    manager.installation.fail_next_keychain_operation();
    manager.attach_and_start_sync_at_startup().await;
    let recorded = manager
        .get_sync_status()
        .error
        .expect("the failed startup connect is recorded");
    let crate::ui::UiError::Diagnostic { category, .. } = &recorded;
    assert_eq!(
        *category,
        crate::ui::UiErrorCategory::KeyringLocked,
        "a keychain refused for this session reads as locked: {recorded:?}"
    );

    manager
        .reconnect_sync()
        .await
        .expect("the keychain answers the retry");
    assert!(
        manager.get_sync_status().error.is_none(),
        "the stale failure is gone the moment the retry runs"
    );
    manager.close().await.unwrap();
}

/// Work asked of a closed library starts nothing, and the caller hears so
/// rather than a log line: starting sync after close is refused.
#[tokio::test]
async fn starting_work_after_close_is_refused_to_the_caller() {
    let (manager, _temp) = setup_test_manager().await;
    manager.close().await.unwrap();
    let error = manager
        .start_sync_at_startup()
        .expect_err("a closed library starts no work");
    assert!(
        matches!(error, LibraryError::Closed("startup sync attachment")),
        "got {error:?}"
    );
    assert!(matches!(manager.start(), Err(LibraryError::Closed(_))));
}
