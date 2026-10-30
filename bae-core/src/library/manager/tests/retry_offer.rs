// Retry Now as the activity list offers it, against coven's own retry: each
// failure it is offered for runs a pass when retried; each failure only the
// person can clear, and a provider's cooldown, offers none, since coven would
// answer the retry at once and run no pass.
//
// `include!`d into `manager::tests`.

/// A syncing library on a clock that never moves, so no backoff ends by
/// itself: a pass after a failed one runs only because a retry ended its
/// wait.
#[cfg(feature = "test-utils")]
async fn library_on_a_stopped_clock() -> (TestManager, TempDir, Arc<coven::FixedClock>) {
    let clock = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
    let (manager, temp_dir) = setup_test_manager_with_clock(clock.clone()).await;
    connect_test_cloud_with_sync_loop(&manager).await;
    manager.start().unwrap();
    (manager, temp_dir, clock)
}

/// Follow the status until `accept` takes it.
#[cfg(feature = "test-utils")]
async fn status_until(
    manager: &TestManager,
    accept: impl Fn(&SyncStatusSnapshot) -> bool,
) -> SyncStatusSnapshot {
    let mut values = manager.subscribe_sync_status_values();
    let waited = tokio::time::timeout(std::time::Duration::from_secs(20), values.wait_for(accept))
        .await
        .map(|value| value.unwrap().clone());
    waited.unwrap_or_else(|_| {
        let value = values.borrow();
        panic!(
            "the status never came: {:?}, {:?}",
            value.status, value.retry_effect
        )
    })
}

/// The next request fails as `failure`, answered with HTTP `response`, or
/// with no answer when `None`.
#[cfg(feature = "test-utils")]
fn next_request_fails(failure: coven::StorageFailure, response: Option<u16>) -> coven::Faults {
    coven::Faults {
        fail_next: 1,
        failure,
        response,
        ..coven::Faults::none()
    }
}

/// Fail the next pass with `faults`, then follow the status until it shows
/// the failure beside the connection's record coven saved for it. Storage
/// works again afterwards, so only coven's wait stands between the library
/// and its next pass.
#[cfg(feature = "test-utils")]
async fn fail_a_pass(manager: &TestManager, faults: coven::Faults) -> SyncStatusSnapshot {
    let mut values = manager.subscribe_sync_status_values();
    manager.storage.set_faults(faults).await;
    manager.trigger_sync();
    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        values.wait_for(|value| {
            matches!(
                value.status,
                coven::SyncStatus::Failed { .. } | coven::SyncStatus::Offline { .. }
            ) && matches!(&value.pending, Some(Ok(items))
                if items.iter().any(|item| item.subject == PendingSubjectName::Connection))
        }),
    )
    .await
    .map(|value| value.unwrap().clone());
    let failed =
        waited.unwrap_or_else(|_| panic!("the pass never failed: {:?}", values.borrow().status));
    manager.storage.set_faults(coven::Faults::none()).await;
    failed
}

/// Each failure coven retries by itself offers Retry Now, and retrying runs
/// a pass at once, which syncs: storage unreachable, a server error,
/// throttling without a cooldown, a conflict, an expired upload session. A
/// missing object fails no pass here: the pass reads it as absent.
#[cfg(feature = "test-utils")]
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
        let failed = fail_a_pass(&manager, next_request_fails(failure, response)).await;
        assert_eq!(failed.retry_effect, coven::RetryEffect::Pass);
        assert!(
            failed.retry_offered(),
            "{failure:?} {response:?}: {:?}",
            failed.status
        );
        let requests = manager.storage.request_trace().len();
        let answer = tokio::time::timeout(std::time::Duration::from_secs(20), manager.retry_now())
            .await
            .expect("the retry's pass ends");
        assert!(
            matches!(answer, crate::library::SyncRetryOutcome::Synced),
            "{failure:?} {response:?}: {answer:?}"
        );
        assert!(
            manager.storage.request_trace().len() > requests,
            "{failure:?} {response:?}: the retry ran a pass"
        );
        manager.close().await.unwrap();
    }
}

/// Each failure only the person can clear, or that never retries, offers no
/// Retry Now: coven holds every pass for it, and a retry would answer with
/// the failure at once, sending nothing to storage.
#[cfg(feature = "test-utils")]
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
        let failed = fail_a_pass(&manager, next_request_fails(failure, response)).await;
        assert_eq!(failed.retry_effect, coven::RetryEffect::Failed);
        assert!(
            !failed.retry_offered(),
            "{failure:?} {response:?}: {:?}",
            failed.status
        );
        let requests = manager.storage.request_trace().len();
        let answer = tokio::time::timeout(std::time::Duration::from_secs(20), manager.retry_now())
            .await
            .expect("a held pass answers the retry at once");
        assert!(
            matches!(answer, crate::library::SyncRetryOutcome::Failed(_)),
            "{failure:?} {response:?}: {answer:?}"
        );
        assert_eq!(
            manager.storage.request_trace().len(),
            requests,
            "{failure:?} {response:?}: no pass ran"
        );
        manager.close().await.unwrap();
    }
}

/// A 429 asking every request to wait ten minutes offers no Retry Now while
/// the wait lasts: a retry would only say when it ends. Once it ends Retry
/// Now is offered again, and retrying runs a pass that syncs.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_provider_cooldown_offers_no_retry_until_it_ends() {
    let (manager, _temp_dir, clock) = library_on_a_stopped_clock().await;
    let cooling = fail_a_pass(
        &manager,
        coven::Faults {
            retry_after: Some(600),
            ..next_request_fails(coven::StorageFailure::RateLimited, Some(429))
        },
    )
    .await;
    let ends = std::time::UNIX_EPOCH + std::time::Duration::from_secs(600);
    assert_eq!(
        cooling.retry_effect,
        coven::RetryEffect::CoolingDown { until: ends }
    );
    assert!(!cooling.retry_offered(), "{:?}", cooling.status);
    let answer = tokio::time::timeout(std::time::Duration::from_secs(20), manager.retry_now())
        .await
        .expect("a cooldown answers the retry at once");
    assert!(
        matches!(answer, crate::library::SyncRetryOutcome::CoolingDown { until } if until == ends),
        "{answer:?}"
    );

    // The pass the cooldown's end lets run finds storage unreachable, so a
    // failure a retry clears holds work back.
    manager
        .storage
        .set_faults(next_request_fails(coven::StorageFailure::Network, None))
        .await;
    clock.set(ends + std::time::Duration::from_secs(1));
    let offline = status_until(&manager, |value| {
        matches!(value.status, coven::SyncStatus::Offline { .. })
    })
    .await;
    assert!(offline.retry_offered(), "{:?}", offline.retry_effect);
    manager.storage.set_faults(coven::Faults::none()).await;
    let requests = manager.storage.request_trace().len();
    let answer = tokio::time::timeout(std::time::Duration::from_secs(20), manager.retry_now())
        .await
        .expect("the retry's pass ends");
    assert!(
        matches!(answer, crate::library::SyncRetryOutcome::Synced),
        "{answer:?}"
    );
    assert!(manager.storage.request_trace().len() > requests, "the retry ran a pass");
    manager.close().await.unwrap();
}
