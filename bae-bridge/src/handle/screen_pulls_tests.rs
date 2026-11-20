//! The library's lists and values the app pulls, each from a live query.

use super::super::tests::pull_until;

/// Each answers the library as it stands at once, the creator's profile
/// among them from the library's creation.
#[test]
fn each_pull_answers_at_once_with_the_creator_s_profile() {
    let root = bae_util::test_files::TestFolder::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (library, _storage) = runtime.block_on(bae_library::test_libraries::storage_library(
        root.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    ));
    let started = super::super::tests::start_test_handle(runtime, library, root);
    let handle = started.handle().clone();

    let queue = handle.subscribe_queue();
    let snapshot = pull_until(&handle, || Box::pin(queue.clone().next()), |_| true);
    assert!(snapshot.manual.is_empty() && !snapshot.has_next);
    let paused = handle.subscribe_uploads_paused();
    pull_until(&handle, || Box::pin(paused.clone().next()), |_| true);
    let unmetered_only = handle.subscribe_unmetered_only();
    assert!(!pull_until(&handle, || Box::pin(unmetered_only.clone().next()), |_| true));
    handle.runtime.block_on(handle.clone().set_unmetered_only(true)).unwrap();
    assert!(pull_until(&handle, || Box::pin(unmetered_only.clone().next()), |on| *on));
    assert!(handle.runtime.block_on(handle.clone().get_unmetered_only()).unwrap());
    let counts = handle.subscribe_download_counts();
    pull_until(&handle, || Box::pin(counts.clone().next()), |_| true);
    let activity = handle.subscribe_sync_activity();
    let quiet = pull_until(&handle, || Box::pin(activity.clone().next()), |_| true);
    assert!(!quiet.moving && quiet.uploads.is_none(), "{quiet:?}");
    let people = handle.subscribe_people();
    let alone = pull_until(&handle, || Box::pin(people.clone().next()), |_| true);
    // The creator is this device's person, with the profile the library's
    // creation gave them, and alone.
    let creator = alone.own.clone().unwrap();
    assert_eq!(
        creator.profile.as_ref().map(|profile| profile.name.as_str()),
        Some(bae_store::test_devices::TEST_CREATOR)
    );
    assert!(alone.others.is_empty() && alone.people == 1, "{alone:?}");
    assert!(matches!(
        handle.subscribe_person("not a member id".to_string()),
        Err(crate::types::BridgeError::Diagnostic {
            category: crate::types::BridgeErrorCategory::Config,
            ..
        })
    ));
    let invites = handle.subscribe_invites();
    assert!(pull_until(&handle, || Box::pin(invites.clone().next()), |_| true).is_empty());
    let unknown =
        handle.subscribe_invite("2f1d6a3e-0000-4000-8000-000000000001".to_string()).unwrap();
    assert!(pull_until(&handle, || Box::pin(unknown.clone().next()), |_| true).is_none());
    assert!(matches!(
        handle.subscribe_invite("not an invite id".to_string()),
        Err(crate::types::BridgeError::Diagnostic {
            category: crate::types::BridgeErrorCategory::Config,
            ..
        })
    ));
    let waiting = handle.subscribe_waiting_join_request(Vec::new()).unwrap();
    assert!(pull_until(&handle, || Box::pin(waiting.clone().next()), |_| true).is_none());
    let changes =
        handle.subscribe_admin_changes(crate::types::BridgeAdminChangePlace::People).unwrap();
    let none = pull_until(&handle, || Box::pin(changes.clone().next()), |_| true);
    assert!(!none.any && none.changes.is_empty(), "{none:?}");

    let name = handle.subscribe_library_name();
    pull_until(&handle, || Box::pin(name.clone().next()), |_| true);
    // The library starts with stars, and its creator, an admin, changes
    // how it shows the ratings of its tracks.
    use crate::types::{BridgeRatedItems, BridgeRatingSystem, BridgeRatingSystems};
    let systems = handle.subscribe_rating_systems();
    let stars = BridgeRatingSystems {
        albums: BridgeRatingSystem::Stars,
        tracks: BridgeRatingSystem::Stars,
    };
    assert_eq!(pull_until(&handle, || Box::pin(systems.clone().next()), |_| true), stars);
    handle
        .runtime
        .block_on(
            handle.clone().set_rating_system(BridgeRatedItems::Tracks, BridgeRatingSystem::Heart),
        )
        .unwrap();
    pull_until(
        &handle,
        || Box::pin(systems.clone().next()),
        |shown| {
            shown.tracks == BridgeRatingSystem::Heart && shown.albums == BridgeRatingSystem::Stars
        },
    );
    // The creator's own page follows them by member id.
    let page = handle.subscribe_person(creator.membership.id.clone()).unwrap();
    let shown = pull_until(&handle, || Box::pin(page.clone().next()), Option::is_some).unwrap();
    assert_eq!(shown.membership.id, creator.membership.id);
    let nobody =
        handle.subscribe_person("00000000-0000-4000-8000-000000000099".to_string()).unwrap();
    assert!(pull_until(&handle, || Box::pin(nobody.clone().next()), |_| true).is_none());
    // With a profile the person can make a playlist, and holds none yet.
    let playlists = handle.subscribe_playlists();
    let listing = pull_until(&handle, || Box::pin(playlists.clone().next()), |l| l.can_create);
    assert!(listing.listed.is_empty() && listing.add_targets.is_empty());

    handle.runtime.block_on(name.clone().cancel()).unwrap();
    assert!(matches!(
        handle.runtime.block_on(name.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}

/// The sync status answers at once that this device has no storage set up,
/// and follows storage being set up from coven's sync status table.
#[test]
fn the_sync_status_follows_storage_being_set_up() {
    let root = bae_util::test_files::TestFolder::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (library, storage) = runtime.block_on(bae_library::test_libraries::storage_library(
        root.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    ));
    let setup = bae_library::LibraryManager::clone(library.manager());
    let setup_store = library.store().clone();
    let started = super::super::tests::start_test_handle(runtime, library, root);
    let handle = started.handle().clone();

    let status = handle.subscribe_sync_status();
    let unset = handle.runtime.block_on(status.clone().next()).unwrap();
    assert_eq!(unset.shown.status, crate::types::BridgeSyncStatusWord::NotSynced, "{unset:?}");

    handle.runtime.block_on(bae_library::test_libraries::connect_storage(
        &setup,
        &setup_store,
        &storage,
    ));
    pull_until(
        &handle,
        || Box::pin(status.clone().next()),
        |status: &crate::types::BridgeSyncStatusSnapshot| {
            status.shown.status != crate::types::BridgeSyncStatusWord::NotSynced
        },
    );
}
