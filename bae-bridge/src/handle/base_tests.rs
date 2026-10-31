//! Library writes through the bridge object the platforms hold.

use std::sync::Arc;

/// A handle over a fresh library holding `fixture`, built the way the app
/// builds one.
fn bridge_handle_holding(
    test_name: &str,
    fixture: &bae_core::library::LibraryFixture,
) -> (Arc<super::AppHandle>, std::path::PathBuf) {
    let (runtime, manager, root) = super::super::tests::fresh_test_library(test_name);
    runtime
        .block_on(manager.write_fixture(fixture))
        .expect("write the library fixture");
    (
        super::super::tests::start_test_handle(runtime, manager),
        root,
    )
}

/// Deleting an album through the handle takes it out of the grid and leaves
/// the other albums; deleting it again is refused, since the library no
/// longer holds it.
#[test]
fn delete_album_takes_the_album_out_of_the_grid() {
    let album = |title: &str| bae_core::library::FixtureAlbum {
        title: title.to_string(),
        artists: vec!["Artist Name".to_string()],
        tracks: vec!["Track Title".to_string()],
        cover: None,
    };
    let (handle, _root) = bridge_handle_holding(
        "delete-album",
        &bae_core::library::LibraryFixture {
            albums: vec![album("Deleted Album"), album("Kept Album")],
            watched_folders: Vec::new(),
            playing: None,
        },
    );
    let subscription = handle.subscribe_album_browse(Vec::new(), false);
    subscription
        .set_windows(vec![crate::types::BridgeLibraryPageWindow {
            offset: 0,
            limit: 10,
        }])
        .expect("request the grid's window");
    let titles = |rows: &[crate::types::BridgeAlbum]| -> Vec<String> {
        rows.iter().map(|album| album.title.clone()).collect()
    };
    let listed = loop {
        let snapshot = handle
            .runtime
            .block_on(subscription.clone().next())
            .expect("album browse snapshot");
        if !snapshot.windows.is_empty() {
            break snapshot;
        }
    };
    assert_eq!(
        titles(&listed.windows[0].rows),
        ["Kept Album", "Deleted Album"]
    );
    let deleted = listed.windows[0]
        .rows
        .iter()
        .find(|album| album.title == "Deleted Album")
        .expect("the album to delete is listed")
        .id
        .clone();

    handle
        .runtime
        .block_on(Arc::clone(&handle).delete_album(deleted.clone()))
        .expect("delete the album");

    let after = loop {
        let snapshot = handle
            .runtime
            .block_on(subscription.clone().next())
            .expect("album browse snapshot");
        if snapshot.total_count == 1 {
            break snapshot;
        }
    };
    assert_eq!(titles(&after.windows[0].rows), ["Kept Album"]);
    let again = handle
        .runtime
        .block_on(Arc::clone(&handle).delete_album(deleted));
    assert!(
        matches!(again, Err(crate::types::BridgeError::Diagnostic { .. })),
        "{again:?}"
    );
}
