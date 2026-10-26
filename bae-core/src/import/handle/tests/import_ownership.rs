//! One import owns a candidate. However many times Import is pressed while
//! the first press is still being answered — and a press lands late when the
//! folder-state lock is busy — the candidate becomes one release. An import
//! of the same files asked for after it is refused, and one asked for before
//! it lands fails when it writes: one folder hash is one release on this
//! device.

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
async fn concurrent_imports_keep_one_owner_and_later_imports_of_its_files_are_refused() {
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
    let (release_id, _) = await_import_outcome(&mut events, &started[0])
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    for selected in [false, true] {
        let refused = if selected {
            handle.import_selected(&key).await
        } else {
            handle.start_import(&key).await
        };
        assert!(
            matches!(
                &refused,
                Err(crate::import::ImportError::FilesAlreadyImported { release_id: imported })
                    if *imported == release_id
            ),
            "a later import of the same files is refused before it runs: {refused:?}"
        );
    }
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}

/// A copy of `original` in a sibling folder named `name`, its files copied
/// with their sizes and modification times, so the two folders' files hash
/// alike.
fn twin_of(original: &FolderCandidate, name: &str) -> FolderCandidate {
    let folder = original.path.with_file_name(name);
    std::fs::create_dir_all(&folder).unwrap();
    let mut twin = original.clone();
    for candidate_file in &mut twin.files.files {
        let source = candidate_file.file.path.clone();
        let target = folder.join(&candidate_file.file.relative_path);
        std::fs::copy(&source, &target).unwrap();
        set_modified(&target, std::fs::metadata(&source).unwrap().modified().unwrap());
        candidate_file.file.path = target;
    }
    twin.path = folder.clone();
    twin.file_root = folder;
    twin.name = name.to_string();
    twin.display_path = name.to_string();
    assert_eq!(twin.files.content_hash(), original.files.content_hash());
    twin
}

/// Store `candidates`, all under one watched root, as the whole of a fresh
/// scan generation of it.
async fn rescan_all_into(manager: &LibraryManager, candidates: &[FolderCandidate]) {
    let root = candidates[0].watched_folder_path.clone();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    for candidate in candidates {
        manager
            .save_folder_scan_item(
                &root,
                generation,
                &crate::import::folder_scanner::ScanItem::Valid(candidate.clone()),
            )
            .await
            .unwrap()
            .expect("the scan generation is current");
    }
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
}

/// Two folders whose files hash alike share one preparation. Both asked to
/// import before either runs, both run: the one that writes second finds the
/// hash taken and fails with the typed error, and the files make one release.
#[tokio::test(flavor = "multi_thread")]
async fn two_imports_of_the_same_files_asked_before_either_runs_make_one_release() {
    let (manager, tmp) = setup_test_manager().await;
    let (original, original_key, _) = picked_candidate(&manager, &tmp, "Album").await;
    let copy = twin_of(&original, "Copy");
    let copy_key = copy.path.to_string_lossy().into_owned();
    rescan_all_into(&manager, &[original, copy]).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    // Each folder's own tags are read; the draft they share is then given its
    // artist. Nothing changes it once an import is asked for.
    for key in [&original_key, &copy_key] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }
    handle
        .set_candidate_album_artists(
            &original_key,
            vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    handle.import_cancels.hold_runs();
    let first = handle.start_import(&original_key).await.unwrap().expect_queued();
    let second = handle.start_import(&copy_key).await.unwrap().expect_queued();
    handle.import_cancels.release_runs();

    let (release_id, _) = await_import_outcome(&mut events, &first)
        .await
        .unwrap_or_else(|error| panic!("the first import lands: {error}"));
    assert_eq!(
        await_import_outcome(&mut events, &second).await,
        Err(crate::import::ImportError::FilesAlreadyImported { release_id }.to_string()),
        "the second finds the files imported here"
    );
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}
