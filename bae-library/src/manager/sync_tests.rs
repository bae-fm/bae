//! Sync: storage setup and its failures, starting, reconnecting and retrying,
//! the pending list, the upload pause, and signing in to a provider.

use super::*;
use crate::manager::tests::*;
use crate::test_libraries::TestLibrary;
use coven::Storage;
use tempfile::TempDir;

/// The text of the error behind `item`, with every cause below it.
fn cause_text(item: &bae_sync_status::PendingItem) -> Option<String> {
    item.wait.cause.clone()
}

/// A file this device is uploading is named on the pending list by its file
/// name and its album, not by coven's ids.
#[tokio::test]
async fn a_waiting_upload_is_named_by_its_file_and_album() {
    let (manager, temp, _release, _file_id) = queued_upload_fixture("named").await;
    let mut values = manager.library.manager().subscribe_sync_status();
    manager.library.manager().start().unwrap();
    // The upload can't read a source that moved, so it waits on the file.
    std::fs::remove_file(temp.path().join("named/a.flac")).unwrap();
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);

    pending_list_until(&mut values, |items| {
        items.iter().any(|item| {
            item.subject
                == bae_sync_status::PendingSubjectName::ReleaseFile {
                    file_name: "a.flac".into(),
                    album_title: "Test Album".into(),
                }
        })
    })
    .await;
}

/// An original changed since its import fails its release's move, even one
/// that kept its size and modification time: the upload checks what it reads
/// against the hashes the import's read recorded, so the pending list names
/// the original as changed and no file switches to the cloud.
#[tokio::test]
async fn an_original_changed_since_its_import_fails_the_move() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("changed").await;
    let mut values = manager.library.manager().subscribe_sync_status();
    manager.library.manager().start().unwrap();
    let path = temp.path().join("changed/a.flac");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, vec![b'b'; 1000]).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);

    pending_list_until(&mut values, |items| {
        items.iter().any(|item| {
            matches!(
                &item.wait.reason,
                coven::PendingReason::Original {
                    failure: coven::FileSourceFailure::Changed,
                    path: failed,
                } if *failed == path
            )
        })
    })
    .await;
    assert!(release_files(manager.library.manager(), &release.id)
        .await
        .iter()
        .all(|file| !file.stored));
}

/// A retry that fails for the same reason keeps its record as it was, and the
/// list still shows the error the retry met, not the first attempt's.
#[tokio::test]
async fn a_retry_that_fails_again_shows_its_own_error() {
    let (manager, _temp, _release, _file_id) = queued_upload_fixture("retried").await;
    let mut values = manager.library.manager().subscribe_sync_status();
    manager.library.manager().start().unwrap();
    let refuse = |status: u16| coven::Faults {
        refuse_file_uploads: 1,
        failure: coven::StorageFailure::Network,
        response: Some(status),
        ..coven::Faults::none()
    };

    manager.storage.set_faults(refuse(500)).await;
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    let first = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("500")))
    })
    .await;

    manager.storage.set_faults(refuse(503)).await;
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    let again = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("503")))
    })
    .await;
    assert_eq!(
        first.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        again.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        "the record stays as it was"
    );
}

/// The pending list keeps the order its records arose in, whatever coven's
/// ids for them: a move whose original went missing first stays first after
/// a later one's does, though the later record sorts first.
#[tokio::test]
async fn the_pending_list_keeps_the_order_its_records_arose_in() {
    let (manager, temp, first, _file_id) = queued_upload_fixture("first").await;
    let mut values = manager.library.manager().subscribe_sync_status();
    manager.library.manager().start().unwrap();
    // The move whose original at `path` is missing, from its record.
    let missing_move = |path: std::path::PathBuf| {
        let manager = manager.library.manager().clone();
        async move {
            manager
                .store
                .pending()
                .await
                .unwrap()
                .into_iter()
                .find_map(|record| match (record.subject, record.reason) {
                    (
                        coven::PendingSubject::Move { id },
                        coven::PendingReason::Original { path: missing, .. },
                    ) if missing == path => Some(id),
                    _ => None,
                })
                .expect("the move's record names its missing original")
        }
    };
    // A move's record as coven stores it begins with its subject, so two
    // records of one reason sort as their subjects' stored text does.
    let stored =
        |id: coven::MoveId| serde_json::to_vec(&coven::PendingSubject::Move { id }).unwrap();
    let first_path = temp.path().join("first/a.flac");
    std::fs::remove_file(&first_path).unwrap();
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    let first_move = missing_move(first_path).await;

    // This device numbers its moves in the order it makes them, each write
    // staging files for the cloud one. Covers staged now are moves of their
    // own, until the next move's stored subject sorts before the first's, so
    // an order by record would put the second failure first.
    let mut next = first_move.0 + 1;
    while stored(coven::MoveId(next)) >= stored(first_move) {
        store_test_cover(manager.library.manager(), &first.id, format!("cover {next}").as_bytes())
            .await;
        next += 1;
    }
    let second = insert_release_with_queued_uploads(
        manager.library.manager(),
        &temp.path().join("second"),
        "Album Title Second",
        &[("b.flac", b"second-bytes")],
    )
    .await;
    let second_path = temp.path().join("second/b.flac");
    std::fs::remove_file(&second_path).unwrap();
    manager.library.manager().drain_uploads_expecting_work().await.unwrap();
    assert!(
        stored(missing_move(second_path).await) < stored(first_move),
        "the second failure's record sorts first"
    );

    let album = |title: &str| bae_sync_status::PendingSubjectName::ReleaseFile {
        file_name: if title == "Test Album" { "a.flac" } else { "b.flac" }.into(),
        album_title: title.into(),
    };
    let listed = pending_list_until(&mut values, |items| {
        items
            .iter()
            .filter(|item| {
                matches!(item.subject, bae_sync_status::PendingSubjectName::ReleaseFile { .. })
            })
            .count()
            == 2
    })
    .await;
    assert_eq!(
        listed
            .iter()
            .map(|item| item.subject.clone())
            .filter(|subject| matches!(
                subject,
                bae_sync_status::PendingSubjectName::ReleaseFile { .. }
            ))
            .collect::<Vec<_>>(),
        vec![album("Test Album"), album("Album Title Second")],
        "{second:?} failed after the first"
    );
}

#[tokio::test]
async fn failed_storage_setup_keeps_the_library_unconfigured() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    test.storage.set_online(false);
    manager
        .store
        .setup_s3_storage(
            test.storage.config(),
            "Device",
            "key".into(),
            coven::SecretText::new("secret".into()),
            &|_| (),
        )
        .await
        .expect_err("offline storage cannot be set up");
    assert!(!manager.is_sync_configured().await.unwrap());
    assert_eq!(manager.library_name().await.unwrap(), "Test Library");
}

#[tokio::test]
async fn a_local_library_does_not_need_storage_for_its_rows() {
    let (manager, _temp, album, release) = manager_with_release().await;
    assert!(!manager.library.manager().is_sync_configured().await.unwrap());
    assert!(find_album(manager.library.manager(), &album.id).await.is_some());
    assert!(find_release(manager.library.manager(), &release.id).await.is_some());
}

#[tokio::test]
async fn reconnect_sync_refuses_a_library_with_no_provider() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let error = manager.reconnect_sync().await.unwrap_err();
    assert!(matches!(error, LibraryError::Sync(coven::SyncError::NoStorage)));
    assert!(manager.sync_status().await.unwrap().error.is_none());
}

#[tokio::test]
async fn startup_and_retry_publish_the_native_connection_status() {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (test, _temp) = setup_test_manager_with_clock(clock).await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    test.storage.set_online(false);
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status();
    manager.attach_and_start_sync_at_startup().await;
    crate::followed::value_matching(&mut values, |value| {
        matches!(
            value.shown_pass(),
            Some(coven::LastPass::Failed { .. } | coven::LastPass::Offline { .. })
        )
    })
    .await;
    assert!(
        manager.is_sync_ready().await.unwrap(),
        "the loop keeps the failed connection retryable"
    );
    test.storage.set_online(true);
    // The failed pass left the connection backing off. The clock is fixed, so
    // the backoff ends only by the person's retry, which also answers when
    // the pass it runs has ended. Moving the clock instead would race the
    // loop arming its wait: a wait armed after the move waits from the new
    // time, and a fixed clock never reaches it.
    manager.retry_now().await.unwrap().await;
    crate::followed::value_matching(&mut values, |value| {
        value.coven.state == coven::SyncState::Synced
    })
    .await;
    manager.close().await.unwrap();
}

/// A running loop backing off after a failed connection runs its next pass at
/// once on Reconnect. The clock is fixed, so the backoff ends only if
/// Reconnect ends it.
#[tokio::test]
async fn reconnect_sync_runs_the_pass_a_backing_off_loop_was_waiting_for() {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (test, _temp) = setup_test_manager_with_clock(clock).await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    test.storage.set_online(false);
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status();
    manager.attach_and_start_sync_at_startup().await;
    crate::followed::value_matching(&mut values, |value| {
        matches!(
            value.shown_pass(),
            Some(coven::LastPass::Failed { .. } | coven::LastPass::Offline { .. })
        )
    })
    .await;
    assert!(manager.is_sync_ready().await.unwrap());
    test.storage.set_online(true);
    manager.reconnect_sync().await.unwrap();
    // A guard so a pass stuck behind the backoff fails instead of hanging.
    tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        crate::followed::value_matching(&mut values, |value| {
            value.coven.state == coven::SyncState::Synced
        }),
    )
    .await
    .expect("reconnect ran the pass without waiting out the backoff");
    manager.close().await.unwrap();
}

/// A launch whose start failed records why in the failure's own category —
/// a keychain locked for this session reads as locked, not as an internal
/// failure, on a device whose storage credentials are gone, whose start reads
/// custody for them —
/// and a retry that gets under way supersedes it: the recorded error clears as
/// the retry starts, so the status line reflects the attempt in flight instead
/// of the one that already failed.
#[tokio::test]
async fn reconnect_sync_clears_the_failure_it_retries() {
    let (test, temp) = setup_test_manager().await;
    connect_test_cloud(&test).await;
    let manager = reopen_without_credentials(test, &temp).await;
    manager.library.manager().start().unwrap();
    manager.installation.fail_next_keychain_operation();
    manager.library.manager().attach_and_start_sync_at_startup().await;
    let recorded = manager
        .library
        .manager()
        .sync_status()
        .await
        .unwrap()
        .error
        .expect("the failed startup connect is recorded");
    let bae_diagnostics::UiError::Diagnostic { category, .. } = &recorded;
    assert_eq!(
        *category,
        bae_diagnostics::UiErrorCategory::KeyringLocked,
        "a keychain refused for this session reads as locked: {recorded:?}"
    );

    manager.library.manager().reconnect_sync().await.expect("the keychain answers the retry");
    assert!(
        manager.library.manager().sync_status().await.unwrap().error.is_none(),
        "the stale failure is gone the moment the retry runs"
    );
    manager.library.manager().close().await.unwrap();
}

/// A launch whose start failed with storage set up on this device, here
/// because this session forgot the store keys, leaves sync not running and
/// offers Reconnect beside why. Without storage set up on this device, as
/// when its credentials are gone, a failed start offers no Reconnect: there
/// is no sync to start, and signing in again is the way back.
#[tokio::test]
async fn only_a_failed_start_with_storage_set_up_offers_reconnect() {
    let (test, temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    manager.forget_encryption_key().await.unwrap();
    manager.start().unwrap();
    manager.attach_and_start_sync_at_startup().await;
    let status = manager.sync_status().await.unwrap();
    assert!(status.indicator().is_some() && !status.running(), "{status:?}");
    assert!(status.error.is_some(), "the failed start is recorded: {status:?}");
    assert!(status.reconnect_offered(), "{status:?}");

    let manager = reopen_without_credentials(test, &temp).await;
    manager.library.manager().start().unwrap();
    manager.installation.fail_next_keychain_operation();
    manager.library.manager().attach_and_start_sync_at_startup().await;
    let status = manager.library.manager().sync_status().await.unwrap();
    assert!(status.error.is_some(), "the failed start is recorded: {status:?}");
    assert!(!status.reconnect_offered(), "{status:?}");
    manager.library.manager().close().await.unwrap();
}

/// A device whose start failed, here on a keychain locked for the session
/// while its storage credentials were gone, signs in to its storage again:
/// the setup starts sync, and the failed start stops showing as the status
/// says sync runs, never beside it.
#[tokio::test]
async fn a_storage_setup_ends_the_failed_start_before_it() {
    let (test, temp) = setup_test_manager().await;
    connect_test_cloud(&test).await;
    let manager = reopen_without_credentials(test, &temp).await;
    manager.library.manager().start().unwrap();
    manager.installation.fail_next_keychain_operation();
    manager.library.manager().attach_and_start_sync_at_startup().await;
    let failed = manager.library.manager().sync_status().await.unwrap();
    assert!(
        matches!(failed.coven.state, coven::SyncState::SignedOut { cause: None, .. }),
        "{failed:?}"
    );
    assert!(failed.error.is_some(), "the failed start is recorded: {failed:?}");

    let mut values = manager.library.manager().subscribe_sync_status();
    manager
        .library
        .manager()
        .store
        .setup_s3_storage(
            manager.storage.config(),
            "Test Device",
            "test-key".into(),
            coven::SecretText::new("test-secret".into()),
            &|_| (),
        )
        .await
        .unwrap();
    let synced = crate::followed::value_matching(&mut values, |value| {
        // Setup writes storage set up on this device and its finish in one
        // write, which ends the failed start, and the status reads its rows
        // after the state it shows.
        assert!(
            value.indicator().is_none() || value.error.is_none(),
            "sync set up shows a failed start: {value:?}"
        );
        value.coven.state == coven::SyncState::Synced
    })
    .await;
    assert_eq!(synced.indicator(), Some(bae_sync_status::SyncIndicator::Synced), "{synced:?}");
    manager.library.manager().close().await.unwrap();
}

/// A start that gets under way ends the failed start before it even when its
/// pass ends the way the last one did: here the last pass synced on a clock
/// that never moves, so the next one syncs at the same time again and
/// changes none of how passes ended.
#[tokio::test]
async fn a_start_whose_pass_ends_as_the_last_did_ends_the_failed_start() {
    let (manager, _temp, _clock) = library_on_a_stopped_clock().await;
    manager.library.manager().forget_encryption_key().await.unwrap();
    manager.library.manager().attach_and_start_sync_at_startup().await;
    assert!(
        manager.library.manager().sync_status().await.unwrap().error.is_some(),
        "the start without the store keys failed"
    );

    manager.library.manager().unlock_store_key().await.unwrap();
    manager.library.manager().attach_and_start_sync().await.unwrap();
    let status = manager.library.manager().sync_status().await.unwrap();
    assert!(status.error.is_none(), "a start under way shows the failed one: {status:?}");
    manager.library.manager().close().await.unwrap();
}

// Retry now as the activity list offers it, against coven's own retry: each
// failure it is offered for runs a pass when retried; each failure only the
// person can clear, and a provider's cooldown, offers none, since coven would
// answer the retry at once and run no pass.

/// A syncing library on a clock that never moves, so no backoff ends by
/// itself: a pass after a failed one runs only because a retry ended its
/// wait.
async fn library_on_a_stopped_clock() -> (TestManager, TempDir, Arc<coven::FixedClock>) {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (test, temp_dir) = setup_test_manager_with_clock(clock.clone()).await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;
    manager.start().unwrap();
    (test, temp_dir, clock)
}

/// Follow the status until `accept` takes it.
async fn status_until(
    manager: &TestManager,
    accept: impl Fn(&bae_sync_status::SyncStatusSnapshot) -> bool,
) -> bae_sync_status::SyncStatusSnapshot {
    let mut values = manager.library.manager().subscribe_sync_status();
    crate::followed::value_matching(&mut values, accept).await
}

/// The next request fails as `failure`, answered with HTTP `response`, or
/// with no answer when `None`.
fn next_request_fails(failure: coven::StorageFailure, response: Option<u16>) -> coven::Faults {
    coven::Faults { fail_next: 1, failure, response, ..coven::Faults::none() }
}

/// Fail the next pass with `faults`, then follow the status until it lists
/// the connection's record coven saved for the failure and `shown` takes
/// it: a failure a person must see names coven's sync state, while one coven
/// retries by itself may name none. Storage works again afterwards, so only
/// coven's wait stands between the library and its next pass.
async fn fail_a_pass(
    manager: &TestManager,
    faults: coven::Faults,
    shown: impl Fn(&bae_sync_status::SyncStatusSnapshot) -> bool,
) -> bae_sync_status::SyncStatusSnapshot {
    let mut values = manager.library.manager().subscribe_sync_status();
    manager.storage.set_faults(faults).await;
    manager.library.manager().store.sync_now();
    let failed = crate::followed::value_matching(&mut values, |value| {
        value
            .pending
            .iter()
            .any(|item| item.subject == bae_sync_status::PendingSubjectName::Connection)
            && shown(value)
    })
    .await;
    manager.storage.set_faults(coven::Faults::none()).await;
    failed
}

/// Each failure coven retries by itself offers Retry now, and retrying runs
/// a pass at once, which syncs: storage unreachable, a server error,
/// throttling without a cooldown, a conflict, an expired upload session. A
/// missing object fails no pass here: the pass reads it as absent.
#[tokio::test]
async fn each_offered_failure_is_retried_by_a_pass_that_syncs() {
    use coven::StorageFailure as F;
    for (failure, response) in [
        (F::Network, None),
        (F::Network, Some(500)),
        (F::Network, Some(503)),
        (F::RateLimited, Some(429)),
        (F::Conflict, Some(409)),
        (F::SessionExpired, Some(404)),
    ] {
        let (manager, _temp_dir, _clock) = library_on_a_stopped_clock().await;
        let failed = fail_a_pass(&manager, next_request_fails(failure, response), |_| true).await;
        assert_eq!(failed.retry_effect, coven::RetryEffect::Pass);
        assert!(failed.retry_offered(), "{failure:?} {response:?}: {:?}", failed.coven);
        let requests = manager.storage.request_trace().len();
        let answer = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            manager.library.manager().retry_now().await.unwrap().await
        })
        .await
        .expect("the retry's pass ends");
        assert!(
            matches!(answer, bae_sync_status::SyncRetryOutcome::Synced),
            "{failure:?} {response:?}: {answer:?}"
        );
        assert!(
            manager.storage.request_trace().len() > requests,
            "{failure:?} {response:?}: the retry ran a pass"
        );
        manager.library.manager().close().await.unwrap();
    }
}

/// Each failure only the person can clear, or that never retries, offers no
/// Retry now: coven holds every pass for it, and a retry would answer with
/// the failure at once, sending nothing to storage.
#[tokio::test]
async fn each_failure_the_person_clears_offers_no_retry() {
    use coven::StorageFailure as F;
    for (failure, response) in [
        (F::Authentication, Some(401)),
        (F::PermissionDenied, Some(403)),
        (F::ContainerNotFound, Some(404)),
        (F::ClockSkew, Some(403)),
        (F::QuotaExceeded, Some(507)),
        (F::Protocol, Some(503)),
        (F::Refused, Some(400)),
    ] {
        let (manager, _temp_dir, _clock) = library_on_a_stopped_clock().await;
        let failed = fail_a_pass(&manager, next_request_fails(failure, response), |value| {
            matches!(value.shown_pass(), Some(coven::LastPass::Failed { .. }))
        })
        .await;
        assert_eq!(failed.retry_effect, coven::RetryEffect::Failed);
        assert!(!failed.retry_offered(), "{failure:?} {response:?}: {:?}", failed.coven);
        let requests = manager.storage.request_trace().len();
        let answer = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            manager.library.manager().retry_now().await.unwrap().await
        })
        .await
        .expect("a held pass answers the retry at once");
        assert!(
            matches!(answer, bae_sync_status::SyncRetryOutcome::Failed(_)),
            "{failure:?} {response:?}: {answer:?}"
        );
        assert_eq!(
            manager.storage.request_trace().len(),
            requests,
            "{failure:?} {response:?}: no pass ran"
        );
        manager.library.manager().close().await.unwrap();
    }
}

/// A 429 asking every request to wait ten minutes offers no Retry now while
/// the wait lasts: a retry would only say when it ends. Once it ends Retry
/// Now is offered again, and retrying runs a pass that syncs.
#[tokio::test]
async fn a_provider_cooldown_offers_no_retry_until_it_ends() {
    let (manager, _temp_dir, clock) = library_on_a_stopped_clock().await;
    let cooling = fail_a_pass(
        &manager,
        coven::Faults {
            retry_after: Some(600),
            ..next_request_fails(coven::StorageFailure::RateLimited, Some(429))
        },
        |value| matches!(value.retry_effect, coven::RetryEffect::CoolingDown { .. }),
    )
    .await;
    let ends = std::time::UNIX_EPOCH + std::time::Duration::from_secs(600);
    assert_eq!(cooling.retry_effect, coven::RetryEffect::CoolingDown { until: ends });
    assert!(!cooling.retry_offered(), "{:?}", cooling.coven);
    let answer = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        manager.library.manager().retry_now().await.unwrap().await
    })
    .await
    .expect("a cooldown answers the retry at once");
    assert!(
        matches!(answer, bae_sync_status::SyncRetryOutcome::CoolingDown { until } if until == ends),
        "{answer:?}"
    );

    // The pass the cooldown's end lets run finds storage unreachable, so a
    // failure a retry clears holds work back.
    manager.storage.set_faults(next_request_fails(coven::StorageFailure::Network, None)).await;
    clock.set(ends + std::time::Duration::from_secs(1));
    let offline = status_until(&manager, |value| {
        matches!(value.shown_pass(), Some(coven::LastPass::Offline { .. }))
    })
    .await;
    assert!(offline.retry_offered(), "{:?}", offline.retry_effect);
    manager.storage.set_faults(coven::Faults::none()).await;
    let requests = manager.storage.request_trace().len();
    let answer = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        manager.library.manager().retry_now().await.unwrap().await
    })
    .await
    .expect("the retry's pass ends");
    assert!(matches!(answer, bae_sync_status::SyncRetryOutcome::Synced), "{answer:?}");
    assert!(manager.storage.request_trace().len() > requests, "the retry ran a pass");
    manager.library.manager().close().await.unwrap();
}

/// An upload a server error refused waits on coven's backoff, which a
/// stopped clock never ends, and offers Retry now; retrying ends the wait,
/// and the file uploads.
#[tokio::test]
async fn a_refused_upload_offers_retry_and_retrying_uploads_it() {
    let (manager, temp_dir, _clock) = library_on_a_stopped_clock().await;
    manager.library.manager().store.set_uploads_paused(true).await.unwrap();
    let release = insert_release_with_queued_uploads(
        manager.library.manager(),
        &temp_dir.path().join("refused"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    manager
        .storage
        .set_faults(coven::Faults {
            refuse_file_uploads: 1,
            failure: coven::StorageFailure::Network,
            response: Some(500),
            ..coven::Faults::none()
        })
        .await;
    manager.library.manager().store.set_uploads_paused(false).await.unwrap();

    let refused = status_until(&manager, |value| {
        value.pending.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("500")))
    })
    .await;
    assert!(refused.retry_offered(), "{:?} {:?}", refused.retry_effect, refused.pending);
    let answer = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        manager.library.manager().retry_now().await.unwrap().await
    })
    .await
    .expect("the retry's pass ends");
    assert!(matches!(answer, bae_sync_status::SyncRetryOutcome::Synced), "{answer:?}");
    inspector_rows_until(manager.library.manager(), &release.id, |rows| {
        rows.files.iter().all(|file| file.file.stored)
    })
    .await;
    let settled = status_until(&manager, |value| value.pending.is_empty()).await;
    assert!(!settled.retry_offered(), "nothing is left to retry: {:?}", settled.pending);
    manager.library.manager().close().await.unwrap();
}

/// Whether uploads are paused reads from coven's pause, with nothing queued
/// too, and follows each change.
#[tokio::test]
async fn the_upload_pause_follows_each_change_with_an_empty_queue() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut paused = views(manager).subscribe_uploads_paused();
    assert!(!paused.next().await.unwrap().unwrap());
    manager.set_sync_paused(true).await.unwrap();
    assert!(paused.next().await.unwrap().unwrap());
    assert!(manager.is_sync_paused().await.unwrap());
    manager.set_sync_paused(false).await.unwrap();
    assert!(!paused.next().await.unwrap().unwrap());
}

/// Whether files move only on unmetered networks reads from coven's setting
/// and follows each change.
#[tokio::test]
async fn the_unmetered_only_setting_follows_each_change() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut unmetered_only = views(manager).subscribe_unmetered_only();
    assert!(!unmetered_only.next().await.unwrap().unwrap());
    manager.set_unmetered_only(true).await.unwrap();
    assert!(unmetered_only.next().await.unwrap().unwrap());
    manager.set_unmetered_only(false).await.unwrap();
    assert!(!unmetered_only.next().await.unwrap().unwrap());
}

/// The settings that set S3 storage up on the manager's memory storage.
fn memory_s3_config(manager: &TestManager) -> crate::sync::S3ConfigData {
    let coven::StorageConfig::S3 { bucket, region, endpoint, profile } = manager.storage.config()
    else {
        panic!("the memory provider uses S3 configuration");
    };
    crate::sync::S3ConfigData {
        bucket,
        region,
        endpoint: endpoint.map(|url| url.to_string()),
        profile,
        access_key: "test-key".into(),
        secret_key: "test-secret".into(),
    }
}

/// A setup call that fails says why itself, so the setup it undid leaves
/// nothing for the sync status to show.
#[tokio::test]
async fn a_failed_setup_call_leaves_nothing_for_the_status() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    test.storage.set_online(false);

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|_| ())
        .await
        .expect_err("offline storage cannot be set up");

    assert_eq!(manager.sync_status().await.unwrap().storage_setup, None);
    let listed = manager
        .store
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT count(*) FROM coven_operations WHERE kind = 'StorageSetup'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(listed, 0);
}

/// Setting S3 up as the library's storage tells the caller the steps coven
/// reports as it runs them, in order, the last its creating the store.
#[tokio::test]
async fn setting_up_s3_storage_tells_each_step_coven_reports() {
    use coven::StorageSetupStep::*;
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let told = std::sync::Mutex::new(Vec::new());

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|progress| {
            told.lock().unwrap().push(progress)
        })
        .await
        .unwrap();

    let told = told.into_inner().unwrap();
    let order = [CheckingLocation, DetectingSettings, Connecting, CreatingStore];
    let positions: Vec<usize> = told
        .iter()
        .map(|progress| {
            order
                .iter()
                .position(|step| *step == progress.step)
                .unwrap_or_else(|| panic!("a step S3 setup doesn't run: {progress:?}"))
        })
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]), "{told:?}");
    assert_eq!(
        told.last(),
        Some(&coven::Progress { step: CreatingStore, done: 1, total: 1 }),
        "{told:?}"
    );
}

/// The steps `storage_setup_finishing` names are past setup's point of no
/// return: a cancel there answers `Finishing` and the setup lands, so the
/// screen that hides Cancel at them hides it where it could no longer undo.
/// Every step before is not finishing.
#[tokio::test]
async fn a_finishing_step_is_past_the_point_of_no_return() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let told = std::sync::Mutex::new(Vec::new());

    manager
        .save_s3_config(memory_s3_config(&test), "Test Device", &|progress| {
            let finishing = crate::storage_setup_finishing(&progress.step);
            let answer = finishing.then(|| manager.store.cancel_storage_setup());
            told.lock().unwrap().push((progress.step, answer));
        })
        .await
        .expect("a setup cancelled past its point of no return finishes");

    let told = told.into_inner().unwrap();
    let first_finishing =
        told.iter().position(|(_, answer)| answer.is_some()).expect("setup reached creating");
    let (before, finishing) = told.split_at(first_finishing);
    assert!(before.iter().all(|(_, answer)| answer.is_none()), "{told:?}");
    assert!(
        finishing.iter().all(|step| step
            == &(
                coven::StorageSetupStep::CreatingStore,
                Some(coven::SetupCancellation::Finishing)
            )),
        "{told:?}"
    );
    assert!(manager.is_sync_configured().await.unwrap());
}

/// Leaving a setup's screen drops its call, which cancels the setup: coven
/// alone would run it to its end with nobody waiting. The cancelled setup
/// shows nothing, and the library stays unconfigured.
#[tokio::test]
async fn a_dropped_setup_call_cancels_its_setup() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    // Every write to storage waits, so setup can't pass its point of no
    // return while the call runs.
    let writes = Arc::new(tokio::sync::Semaphore::new(0));
    test.storage.gate_creations(writes.clone()).await;
    let mut setup =
        Box::pin(manager.save_s3_config(memory_s3_config(&test), "Test Device", &|_| ()));
    let running = |status: &bae_sync_status::SyncStatusSnapshot| {
        status.storage_setup == Some(bae_sync_status::StorageSetupState::Running)
    };
    tokio::select! {
        returned = setup.as_mut() => panic!("a setup whose writes wait returned: {returned:?}"),
        _ = status_until(&test, running) => {}
    }

    drop(setup);
    writes.add_permits(usize::MAX >> 4);
    status_until(&test, |status| status.storage_setup.is_none()).await;
    let outcomes = manager
        .store
        .read(|sql| {
            Ok(sql.query(
                "SELECT outcome FROM coven_operations WHERE kind = 'StorageSetup'",
                [],
                |row| row.get::<_, Option<coven::OperationOutcome>>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(outcomes, vec![Some(coven::OperationOutcome::Cancelled)]);
    assert!(!manager.is_sync_configured().await.unwrap());
}

/// Close `manager`, delete this device's storage credentials as a keychain
/// entry deleted by hand does, and open the library again: its location
/// stays recorded, with no storage set up on this device.
async fn reopen_without_credentials(manager: TestManager, temp_dir: &TempDir) -> TestManager {
    let store = manager.library.manager().store.store_id();
    manager.library.manager().close().await.unwrap();
    manager.installation.lose_storage_credentials(store).unwrap();
    let app_dir = bae_app_dir::AppDir::at(temp_dir.path()).unwrap();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let handle = bae_store::configure_store(manager.installation.builder(), crate::SCHEMA)
        .clock(clock.clone())
        .storage_connector(manager.storage.clone())
        .open(store, |_| ())
        .await
        .unwrap();
    let database = bae_store::test_devices::store_under(handle, clock.clone());
    let library = TestLibrary::open_with(
        database,
        app_dir,
        manager.fakes.providers(),
        bae_util::http::Http::for_test(),
        clock,
        Arc::new(coven::UuidIds),
        bae_diagnostics::Diagnostics::noop(),
    )
    .await;
    TestManager {
        library,
        fakes: manager.fakes,
        storage: manager.storage,
        installation: manager.installation,
    }
}

#[cfg(feature = "oauth-providers")]
mod sign_in {
    use super::*;
    use coven::MemoryStorage;

    /// Records each authorization URL coven presents, then cancels the sign-in.
    #[derive(Default)]
    struct RecordingPresenter {
        presented: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl coven::OAuthPresenter for RecordingPresenter {
        fn redirect_uri(&self, _: coven::CloudProvider) -> &str {
            "bae-test:/oauth"
        }

        async fn present(
            &self,
            authorization_url: &str,
        ) -> Result<coven::SecretText, coven::OAuthError> {
            self.presented.lock().unwrap().push(authorization_url.to_owned());
            Err(coven::OAuthError::Cancelled)
        }
    }

    /// A manager whose store signs in through `presenter`, with Full Dropbox and
    /// OneDrive applications registered as the apps register them.
    async fn manager_signing_in_with(
        presenter: Arc<RecordingPresenter>,
    ) -> (LibraryManager, Arc<MemoryStorage>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let app_dir = bae_app_dir::AppDir::at(temp_dir.path()).unwrap();
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(MemoryStorage::builder().clock(clock.clone()).build().unwrap());
        let handle = bae_store::configure_store(
            coven::Coven::in_memory(app_dir.coven_app_dir()).builder(),
            crate::SCHEMA,
        )
        .oauth_clients(coven::OAuthClients::new(
            Some(coven::DropboxClients::FullDropbox { app_key: "dropbox-key".into() }),
            Some("onedrive-id".into()),
        ))
        .oauth_presenter(presenter)
        .storage_connector(storage.clone())
        .create_store("Test Library")
        .await
        .unwrap();
        let database = bae_store::test_devices::store_under(handle, clock.clone());
        let library = TestLibrary::open_with(
            database,
            app_dir,
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
            clock,
            Arc::new(coven::UuidIds),
            bae_diagnostics::Diagnostics::noop(),
        )
        .await;
        let manager = LibraryManager::clone(library.manager());
        (manager, storage, temp_dir)
    }

    #[tokio::test]
    async fn signing_in_to_dropbox_asks_for_the_shared_folder_access_households_need() {
        let presenter = Arc::new(RecordingPresenter::default());
        let (manager, _storage, _temp) = manager_signing_in_with(presenter.clone()).await;

        manager.sign_in(coven::CloudProvider::Dropbox).await.expect_err("the presenter cancels");

        let presented = presenter.presented.lock().unwrap().clone();
        assert_eq!(presented.len(), 1);
        let url = reqwest::Url::parse(&presented[0]).unwrap();
        let scope = url
            .query_pairs()
            .find(|(name, _)| name == "scope")
            .expect("the request names its scope")
            .1
            .into_owned();
        assert!(scope.split(' ').any(|s| s == "sharing.read"), "{scope}");
    }

    #[tokio::test]
    async fn signing_in_again_to_s3_storage_presents_no_sign_in() {
        let presenter = Arc::new(RecordingPresenter::default());
        let (manager, storage, _temp) = manager_signing_in_with(presenter.clone()).await;
        manager
            .store
            .setup_s3_storage(
                storage.config(),
                "Test Device",
                "test-key".into(),
                coven::SecretText::new("test-secret".into()),
                &|_| (),
            )
            .await
            .unwrap();

        let error = manager
            .sign_in_again("Test Device", &|_| ())
            .await
            .expect_err("an S3 library takes a new access key instead");

        assert!(matches!(error, LibraryError::Internal(_)), "{error}");
        assert!(presenter.presented.lock().unwrap().is_empty());
    }
}
