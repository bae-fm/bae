//! One import owns a candidate. However many times Import is pressed while
//! the first press is still being answered — and a press lands late when the
//! folder-state lock is busy — the candidate becomes one release.

use super::*;

/// Every release in the library.
async fn release_count(handle: &ImportServiceHandle) -> usize {
    let mut count = 0;
    for album in handle.library_manager.get_albums(&[]).await.unwrap() {
        count += handle
            .library_manager
            .get_releases_for_album(&album.id)
            .await
            .unwrap()
            .len();
    }
    count
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_imports_keep_one_owner_and_later_imports_make_new_copies() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    let presses = (0..5).map(|_| {
        let handle = handle.clone();
        let key = key.clone();
        tokio::spawn(async move { handle.start_import(&key).await })
    });
    let mut started = Vec::new();
    for press in presses.collect::<Vec<_>>() {
        match press.await.unwrap() {
            Ok(start) => started.push(start.expect_queued()),
            Err(crate::import::ImportError::CandidateImportInProgress) => {}
            Err(error) => panic!("a repeated press is refused as in progress: {error}"),
        }
    }
    assert_eq!(started.len(), 1, "exactly one press starts an import");
    await_import_outcome(&mut events, &started[0])
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    for selected in [false, true] {
        let id = if selected {
            handle.import_selected(&key).await
        } else {
            handle.start_import(&key).await
        }
        .expect("a later import creates its own copy")
        .expect_queued();
        await_import_outcome(&mut events, &id).await.unwrap();
    }
    assert_eq!(release_count(&handle).await, 3);
    shut_down(handle).await;
}
