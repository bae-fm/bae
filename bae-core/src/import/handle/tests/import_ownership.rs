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
async fn repeated_imports_of_one_candidate_make_one_release() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::import::ArtistAssignment::named("Artist Name")],
        )
        .await
        .unwrap();
    let mut events = handle.subscribe_events();

    let presses = (0..5).map(|_| {
        let handle = handle.clone();
        let key = key.clone();
        tokio::spawn(async move {
            handle
                .start_import(&key, crate::import::StorageMode::Local, false)
                .await
        })
    });
    let mut started = Vec::new();
    for press in presses.collect::<Vec<_>>() {
        match press.await.unwrap() {
            Ok(import_id) => started.push(import_id),
            Err(crate::import::ImportError::CandidateImportInProgress) => {}
            Err(error) => panic!("a repeated press is refused as in progress: {error}"),
        }
    }
    assert_eq!(started.len(), 1, "exactly one press starts an import");
    await_import_outcome(&mut events, &started[0])
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    let after = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await;
    let ready = handle
        .import_ready(&key, crate::import::StorageMode::Local, false)
        .await;
    let releases = release_count(&handle).await;
    shut_down(handle).await;

    assert!(matches!(
        after,
        Err(crate::import::ImportError::CandidateAlreadyImported)
    ));
    assert!(matches!(
        ready,
        Err(crate::import::ImportError::CandidateAlreadyImported)
    ));
    assert_eq!(releases, 1);
}
