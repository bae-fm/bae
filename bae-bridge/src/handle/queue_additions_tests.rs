//! The additions to Up Next, as the app pulls them.

use bae_util::test_channels::HANG_GUARD;

/// A handle over a fresh library holding one album of `tracks` tracks, and
/// the album's track ids in order.
fn handle_over_one_album(
    test_name: &str,
    tracks: &[&str],
) -> (super::super::tests::TestHandle, Vec<String>) {
    let (runtime, library, root) = super::super::tests::fresh_test_library(test_name);
    let fixture = bae_library::LibraryFixture {
        albums: vec![bae_library::FixtureAlbum {
            title: "Album Title".to_string(),
            artists: vec!["Artist Name".to_string()],
            tracks: tracks.iter().map(|title| title.to_string()).collect(),
            cover: None,
        }],
        watched_folders: Vec::new(),
        playing: None,
    };
    let track_ids = runtime.block_on(async {
        library.manager().write_fixture(&fixture).await.expect("write the library fixture");
        let albums = bae_views::LibraryViews::new(library.store().clone())
            .get_albums(&[])
            .await
            .expect("read the albums");
        let releases = library
            .records()
            .get_releases_for_album(&albums[0].id)
            .await
            .expect("read the releases");
        library
            .records()
            .get_tracks_for_release(&releases[0].id)
            .await
            .expect("read the tracks")
            .into_iter()
            .map(|track| track.id)
            .collect()
    });
    (super::super::tests::start_test_handle(runtime, library, root), track_ids)
}

/// An addition reaches the app as a value of its own, with how many tracks
/// it put in, even after one alike; cancelling ends the pulls.
#[test]
fn each_addition_reaches_the_app_once() {
    let (handle, tracks) = handle_over_one_album("queue-additions", &["One", "Two"]);
    let additions = handle.handle().subscribe_queue_additions();
    let pull = || {
        handle.handle().runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, additions.clone().next())
                .await
                .expect("an addition in time")
                .expect("the pull reads")
        })
    };

    let add_to_queue = handle.handle().clone().add_to_queue(tracks.clone());
    handle.handle().runtime.block_on(add_to_queue).expect("the tracks are added");
    assert_eq!(pull(), crate::types::BridgeQueueAddition { number: 1, tracks: 2 });
    for (number, track) in [(2, &tracks[0]), (3, &tracks[1])] {
        let add_next = handle.handle().clone().add_next(vec![track.clone()]);
        handle.handle().runtime.block_on(add_next).expect("the track is added");
        assert_eq!(pull(), crate::types::BridgeQueueAddition { number, tracks: 1 });
    }

    handle.handle().runtime.block_on(additions.clone().cancel()).unwrap();
    assert!(matches!(
        handle.handle().runtime.block_on(additions.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
