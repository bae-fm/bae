//! The library's lists and values the app pulls, each from a live query.

use std::time::Duration;

/// Pull `next` until `wanted` takes a value, failing the test after ten
/// seconds.
fn pull_until<T>(
    handle: &super::AppHandle,
    next: impl Fn() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, crate::types::BridgeError>> + Send>,
    >,
    wanted: impl Fn(&T) -> bool,
) -> T {
    handle.runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
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

/// Each answers the library as it stands at once, and the profiles follow
/// the one made.
#[test]
fn each_pull_answers_at_once_and_the_profiles_follow_a_new_one() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    // A library with its storage, which making a profile needs.
    let (manager, _db, storage) = runtime.block_on(bae_test_support::storage_library(
        root.path(),
        bae_core::providers::Providers::offline(),
    ));
    runtime.block_on(bae_test_support::connect_storage(&manager, &storage));
    let syncing = manager.clone();
    let services = runtime
        .block_on(bae_core::library::AppServices::for_test(manager))
        .unwrap();
    let handle = std::sync::Arc::new(
        super::AppHandle::start(
            services,
            runtime,
            bae_core::diagnostics::Diagnostics::noop(),
        )
        .unwrap(),
    );

    let queue = handle.subscribe_queue();
    let snapshot = pull_until(&handle, || Box::pin(queue.clone().next()), |_| true);
    assert!(snapshot.manual.is_empty() && !snapshot.has_next);
    let paused = handle.subscribe_uploads_paused();
    pull_until(&handle, || Box::pin(paused.clone().next()), |_| true);
    let counts = handle.subscribe_download_counts();
    pull_until(&handle, || Box::pin(counts.clone().next()), |_| true);
    let activity = handle.subscribe_sync_activity();
    let quiet = pull_until(&handle, || Box::pin(activity.clone().next()), |_| true);
    assert!(!quiet.moving && quiet.uploads.is_none(), "{quiet:?}");
    let profiles = handle.subscribe_profiles();
    pull_until(&handle, || Box::pin(profiles.clone().next()), |_| true);
    let playlists = handle.subscribe_playlists();
    assert!(pull_until(&handle, || Box::pin(playlists.clone().next()), |_| true).is_empty());
    let invites = handle.subscribe_invites();
    assert!(pull_until(&handle, || Box::pin(invites.clone().next()), |_| true).is_empty());
    let ratings = handle.subscribe_ratings();
    assert!(pull_until(&handle, || Box::pin(ratings.clone().next()), |_| true).is_empty());

    let name = handle.subscribe_library_name();
    pull_until(&handle, || Box::pin(name.clone().next()), |_| true);
    let requested = handle.subscribe_requested_profile();
    assert!(pull_until(&handle, || Box::pin(requested.clone().next()), |_| true).is_none());
    handle
        .runtime
        .block_on(handle.clone().create_profile("Ana".to_string()))
        .unwrap();
    assert_eq!(
        pull_until(
            &handle,
            || Box::pin(requested.clone().next()),
            Option::is_some
        ),
        Some(crate::types::BridgeRequestedProfile {
            name: "Ana".into(),
            state: crate::types::BridgeRequestedProfileState::Making,
        })
    );
    // The profile is written once sync has made Ana's private circle, and
    // the request ends with it.
    handle
        .runtime
        .block_on(syncing.attach_and_start_sync())
        .unwrap();
    pull_until(
        &handle,
        || Box::pin(profiles.clone().next()),
        |profiles: &Vec<crate::types::BridgePersonProfile>| profiles.len() == 1,
    );
    pull_until(
        &handle,
        || Box::pin(requested.clone().next()),
        Option::is_none,
    );

    handle.runtime.block_on(name.clone().cancel()).unwrap();
    assert!(matches!(
        handle.runtime.block_on(name.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}

/// The sync status answers whether storage is set up at once, and follows a
/// disconnect from coven's sync status table.
#[test]
fn the_sync_status_follows_a_disconnect() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (manager, _db, storage) = runtime.block_on(bae_test_support::storage_library(
        root.path(),
        bae_core::providers::Providers::offline(),
    ));
    runtime.block_on(bae_test_support::connect_storage(&manager, &storage));
    let services = runtime
        .block_on(bae_core::library::AppServices::for_test(manager))
        .unwrap();
    let handle = std::sync::Arc::new(
        super::AppHandle::start(
            services,
            runtime,
            bae_core::diagnostics::Diagnostics::noop(),
        )
        .unwrap(),
    );

    let status = handle.subscribe_sync_status();
    let connected = handle.runtime.block_on(status.clone().next()).unwrap();
    assert!(connected.connected, "{connected:?}");

    handle
        .runtime
        .block_on(handle.clone().disconnect_cloud_provider())
        .unwrap();
    let disconnected = pull_until(
        &handle,
        || Box::pin(status.clone().next()),
        |status: &crate::types::BridgeSyncStatusSnapshot| !status.connected,
    );
    assert_eq!(
        disconnected.indicator,
        crate::types::BridgeSyncIndicator::Unconnected
    );
}
