//! The answers of the commands that add to Up Next.

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

/// Each add answers how many tracks it put in, which the screen that asked
/// confirms.
#[test]
fn each_add_answers_how_many_tracks_went_in() {
    let (handle, tracks) = handle_over_one_album("queue-adds", &["One", "Two"]);
    let runtime = &handle.handle().runtime;

    let added = runtime.block_on(handle.handle().clone().add_to_queue(tracks.clone()));
    assert_eq!(added.expect("the tracks are added"), 2);
    let added = runtime.block_on(handle.handle().clone().add_next(vec![tracks[0].clone()]));
    assert_eq!(added.expect("the track is added"), 1);
    let added = runtime.block_on(handle.handle().clone().insert_in_queue(tracks, 1));
    assert_eq!(added.expect("the tracks are inserted"), 2);
}
