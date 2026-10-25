/// A folder scan finds each album in a multi-album folder as its own candidate.
#[tokio::test]
async fn folder_scan_produces_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album1).unwrap();
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album1, &["01 Track.flac", "02 Track.flac"]);
    generate_album_files(&album2, &["01 Track.flac"]);

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();

    let mut candidates = vec![];
    support::next_matching(&mut scan_rx, HANG_GUARD, |event| match event {
        ScanEvent::FolderCandidate { candidate, .. } => {
            candidates.push(candidate);
            None
        }
        ScanEvent::Finished => Some(()),
        _ => None,
    })
    .await
    .expect("the scan finishes");

    // `Collection/` has no disc-named subfolders, so it only holds albums.
    assert_eq!(candidates.len(), 2, "each album should be a candidate");
    let names: std::collections::BTreeSet<_> = candidates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains("Artist - First Album"));
    assert!(names.contains("Artist - Second Album"));
}

/// A new release folder appears as a candidate and a deleted one emits
/// `CandidateRemoved`. Each step rescans explicitly and waits for its event, so
/// filesystem-event timing does not matter.
#[tokio::test]
async fn watcher_reconciles_added_and_removed_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    fs::create_dir_all(&album1).unwrap();
    generate_album_files(&album1, &["01 Track.flac"]);
    let album1_key = album1.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();

    let first = scan_batch_until(&mut scan_rx, "the first album's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album1_key.as_str()))
    })
    .await;
    assert!(
        first.added.contains(&album1_key),
        "initial scan should surface the first album"
    );
    assert!(
        first.removed.is_empty(),
        "the initial scan of a fresh folder removes nothing"
    );

    // A new release folder appears on disk; re-scan surfaces it.
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album2, &["01 Track.flac"]);
    let album2_key = album2.to_string_lossy().into_owned();
    f.handle.scan_watched_folders().unwrap();

    let second = scan_batch_until(&mut scan_rx, "the newly-added folder's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album2_key.as_str()))
    })
    .await;
    assert!(
        second.added.contains(&album2_key),
        "the newly-added folder should surface as a candidate"
    );

    // The first release folder is deleted; re-scan removes its candidate.
    fs::remove_dir_all(&album1).unwrap();
    f.handle.scan_watched_folders().unwrap();

    let third = scan_batch_until(&mut scan_rx, "the deleted folder's candidate removal", |e| {
        matches!(e, ScanEvent::CandidateRemoved { candidate_key } if candidate_key == &album1_key)
    })
    .await;
    assert!(
        third.removed.contains(&album1_key),
        "the deleted folder's candidate should be removed"
    );
}

struct ScanBatch {
    added: Vec<String>,
    removed: Vec<String>,
}

/// Read scan events until one matching `done` arrives, collecting the
/// candidates added and removed along the way; panics at the hang guard.
async fn scan_batch_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut done: impl FnMut(&ScanEvent) -> bool,
) -> ScanBatch {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    support::next_matching(rx, HANG_GUARD, |event| {
        let finished = done(&event);
        match &event {
            ScanEvent::FolderCandidate { candidate: c, .. } => {
                added.push(c.path.to_string_lossy().into_owned())
            }
            ScanEvent::CandidateRemoved { candidate_key } => removed.push(candidate_key.clone()),
            _ => {}
        }
        finished.then_some(())
    })
    .await
    .unwrap_or_else(|| panic!("timed out waiting for {what}"));
    ScanBatch { added, removed }
}

/// The first candidate list `accept` admits; panics at the hang guard.
async fn wait_for_candidates(
    f: &ImportFixture,
    what: &str,
    accept: impl FnMut(&bae_core::import::ImportListSnapshot) -> bool,
) -> bae_core::import::ImportListSnapshot {
    wait_for_tab(f, what, bae_core::import::TriageTab::Pending, accept).await
}

/// The first read of one tab that `accept` admits; panics at the hang guard.
async fn wait_for_tab(
    f: &ImportFixture,
    what: &str,
    tab: bae_core::import::TriageTab,
    accept: impl FnMut(&bae_core::import::ImportListSnapshot) -> bool,
) -> bae_core::import::ImportListSnapshot {
    let view = bae_core::import::ImportListView {
        tab,
        ..bae_core::import::ImportListView::default()
    };
    tokio::time::timeout(HANG_GUARD, f.handle.wait_for_list(view, accept))
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

/// The candidate rows one read of the list holds.
fn candidate_rows(
    projection: &bae_core::import::ImportListSnapshot,
) -> Vec<&bae_core::import::TriageRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            bae_core::import::ImportListItem::Candidate { row, .. } => Some(row),
            _ => None,
        })
        .collect()
}

/// Wait for a scan event matching `pred`; panics at the hang guard.
async fn wait_for_scan_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut pred: impl FnMut(&ScanEvent) -> bool,
) {
    support::next_matching(rx, HANG_GUARD, |event| pred(&event).then_some(()))
        .await
        .unwrap_or_else(|| panic!("timed out waiting for {what}"));
}

/// `remove_watched_folder` drops the folder from the stored list and its
/// candidates from the import list, broadcasts the shortened list, and stops
/// watching it.
#[tokio::test]
async fn remove_watched_folder_drops_folder_and_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();
    let collection_key = collection.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection_key.clone())
        .await
        .unwrap();

    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(
        batch.added.contains(&album_key),
        "initial scan should surface the album candidate"
    );
    assert_eq!(
        f.handle
            .watched_folders()
            .await
            .unwrap()
            .iter()
            .map(|w| w.path.clone())
            .collect::<Vec<_>>(),
        vec![collection_key.clone()],
    );
    wait_for_candidates(&f, "the list holds the scanned candidate", |projection| {
        !candidate_rows(projection).is_empty()
    })
    .await;

    // Watched throughout: what its watch finds is the sentinel below. Its
    // first read is over before the sentinel is written, so only the watch
    // can find it.
    let other = f.temp_path().join("Other");
    fs::create_dir_all(&other).unwrap();
    let other_key = other.to_string_lossy().into_owned();
    f.handle.add_watched_folder(other_key.clone()).await.unwrap();
    wait_for_scan_event(&mut scan_rx, "the still-watched folder's first read", |event| {
        matches!(
            event,
            ScanEvent::FolderScanStatusChanged { status }
                if status.watched_folder_path == other_key
                    && matches!(status.status, bae_core::import::FolderScanStatus::Complete)
        )
    })
    .await;

    f.handle
        .remove_watched_folder(collection_key.clone())
        .await
        .unwrap();

    // The watched list changes at once; the candidate list once its query
    // reads again.
    assert_eq!(
        f.handle
            .watched_folders()
            .await
            .unwrap()
            .iter()
            .map(|w| w.path.clone())
            .collect::<Vec<_>>(),
        vec![other_key.clone()],
        "removed folder is gone from the persisted list"
    );
    wait_for_candidates(
        &f,
        "the list dropped the removed folder's candidates",
        |projection| candidate_rows(projection).is_empty(),
    )
    .await;

    // The shortened list is broadcast.
    wait_for_scan_event(
        &mut scan_rx,
        "the shortened folder-list broadcast",
        |event| matches!(event, ScanEvent::WatchedFoldersChanged { folders } if folders.len() == 1),
    )
    .await;

    // The watch stopped: a new release folder under the removed root is not
    // found ahead of one written after it under the root still watched.
    let new_album = collection.join("Artist - Second Album");
    fs::create_dir_all(&new_album).unwrap();
    generate_album_files(&new_album, &["01 Track.flac"]);
    let sentinel = other.join("Artist - Third Album");
    fs::create_dir_all(&sentinel).unwrap();
    generate_album_files(&sentinel, &["01 Track.flac"]);
    let sentinel_key = sentinel.to_string_lossy().into_owned();
    let after_unwatch = scan_batch_until(&mut scan_rx, "the still-watched folder's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(sentinel_key.as_str()))
    })
    .await;
    assert_eq!(
        after_unwatch.added,
        vec![sentinel_key],
        "an unwatched folder must not surface new candidates"
    );
}

#[tokio::test]
async fn unavailable_watched_folder_remains_durable_and_reports_scan_failure() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let missing = f.temp_path().join("does-not-exist");
    let missing_key = missing.to_string_lossy().into_owned();

    f.handle
        .add_watched_folder(missing_key.clone())
        .await
        .unwrap();
    wait_for_candidates(
        &f,
        "unavailable watched root reports a failed scan",
        |projection| {
            projection.folder_scans.statuses.iter().any(|status| {
                status.watched_folder_path == missing_key
                    && matches!(
                        status.status,
                        bae_core::import::FolderScanStatus::Failed { .. }
                    )
            })
        },
    )
    .await;
    assert_eq!(f.handle.watched_folders().await.unwrap().len(), 1);
}

/// A scan that cannot read the stored file decisions leaves the root failed,
/// carrying the error.
#[tokio::test]
async fn scan_whose_stored_decisions_cannot_be_read_records_the_failure() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("Collection");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();

    let invalid_folder = format!("{root_key}/a/./b");
    f.db.execute_local_sql_for_test(&format!(
        "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
         VALUES ('grouping:corrupt', '{}', 1, 'user')",
        invalid_folder.replace('\'', "''"),
    )).await.unwrap();
    let mut events = f.handle.every_scan_event_for_test();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    let error = support::next_matching(&mut events, HANG_GUARD, |event| {
        match event {
            ScanEvent::FolderScanStatusChanged { status } if status.watched_folder_path == root_key => {
                match status.status {
                    bae_core::import::FolderScanStatus::Failed { error } => Some(error),
                    _ => None,
                }
            }
            _ => None,
        }
    }).await.expect("unreadable decisions leave a failed scan status");
    assert!(
        error.contains(&invalid_folder),
        "the failed status carries what went wrong, got {error:?}"
    );
}

/// Choosing a folder that is already watched reads it again, so a folder that
/// could not be read reports its status again.
#[tokio::test]
async fn adding_an_already_watched_folder_reads_it_again() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("Collection");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();

    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    wait_for_candidates(&f, "the first scan of the added root", |projection| {
        projection.folder_scans.statuses.iter().any(|status| {
            status.watched_folder_path == root_key
                && matches!(status.status, bae_core::import::FolderScanStatus::Complete)
        })
    })
    .await;

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    wait_for_scan_event(
        &mut scan_rx,
        "the re-added root is scanned again",
        |event| {
            matches!(
                event,
                ScanEvent::FolderScanStatusChanged { status }
                    if status.watched_folder_path == root_key
                        && matches!(
                            status.status,
                            bae_core::import::FolderScanStatus::Scanning { .. }
                        )
            )
        },
    )
    .await;
    assert_eq!(f.handle.watched_folders().await.unwrap().len(), 1);
}

/// Adding a folder inside a watched one reads the watched one again rather
/// than watching an overlapping root.
#[tokio::test]
async fn adding_a_folder_inside_a_watched_one_reads_the_watched_one_again() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("Collection");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();

    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    wait_for_candidates(&f, "the first scan of the added root", |projection| {
        projection.folder_scans.statuses.iter().any(|status| {
            status.watched_folder_path == root_key
                && matches!(status.status, bae_core::import::FolderScanStatus::Complete)
        })
    })
    .await;

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(album.to_string_lossy().into_owned())
        .await
        .unwrap();
    wait_for_scan_event(
        &mut scan_rx,
        "the covering root is scanned again",
        |event| {
            matches!(
                event,
                ScanEvent::FolderScanStatusChanged { status }
                    if status.watched_folder_path == root_key
                        && matches!(
                            status.status,
                            bae_core::import::FolderScanStatus::Scanning { .. }
                        )
            )
        },
    )
    .await;
    let watched = f.handle.watched_folders().await.unwrap();
    assert_eq!(watched.len(), 1);
    assert_eq!(watched[0].path, root_key);
}

/// An album folder with one track at `relative` under the fixture's folder.
fn album_dir(f: &ImportFixture, relative: &str) -> std::path::PathBuf {
    let dir = f.temp_path().join(relative);
    fs::create_dir_all(&dir).unwrap();
    generate_album_files(&dir, &["01 Track.flac"]);
    dir
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Wait until every one of `roots` has been read to the end.
async fn wait_for_reads(f: &ImportFixture, what: &str, roots: &[&Path]) {
    let roots: Vec<String> = roots.iter().map(|root| path_string(root)).collect();
    wait_for_candidates(f, what, |projection| {
        roots.iter().all(|root| {
            projection.folder_scans.statuses.iter().any(|status| {
                &status.watched_folder_path == root
                    && matches!(status.status, bae_core::import::FolderScanStatus::Complete)
            })
        })
    })
    .await;
}

/// The stored candidate at `path`, as a scanned folder.
async fn scanned_folder(
    f: &ImportFixture,
    path: &str,
) -> (bae_core::import::FolderCandidate, bool) {
    match f.handle.get_candidate(path).await.unwrap() {
        Some(bae_core::import::ImportCandidateSnapshot::Folder {
            candidate, skipped, ..
        }) => (candidate, skipped),
        other => panic!("{path} is a scanned folder: {other:?}"),
    }
}

/// A folder holding watched folders is watched in their place, and a skip, a
/// folder read as one release, and a pick made under them all stay.
#[tokio::test]
async fn a_folder_holding_watched_folders_takes_them_over_and_keeps_what_was_decided() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let music = f.temp_path().join("Music");
    let artist = music.join("Artist");
    let other = music.join("Other Artist");
    let skipped = album_dir(&f, "Music/Artist/Album");
    let picked = album_dir(&f, "Music/Artist/Album 2");
    album_dir(&f, "Music/Other Artist/Box/Album A");
    album_dir(&f, "Music/Other Artist/Box/Album B");
    f.handle.add_watched_folder(path_string(&artist)).await.unwrap();
    f.handle.add_watched_folder(path_string(&other)).await.unwrap();
    wait_for_reads(&f, "the inner folders are read", &[&artist, &other]).await;

    f.handle
        .set_candidate_skipped(path_string(&skipped), true)
        .await
        .unwrap();
    f.handle
        .select_candidate_file_tags(path_string(&picked))
        .await
        .unwrap();
    let combined = f
        .handle
        .combine_folder(bae_core::import::FolderReleaseDecisionKey {
            watched_folder_path: path_string(&other),
            relative_folder_path: "Box".to_string(),
        })
        .await
        .unwrap();
    let picked_hash = scanned_folder(&f, &path_string(&picked))
        .await
        .0
        .files
        .content_hash();

    f.handle
        .add_watched_folder(path_string(&music))
        .await
        .expect("the folder takes over the ones inside it");
    wait_for_reads(&f, "the folder that took over is read", &[&music]).await;

    let watched: Vec<String> = f
        .handle
        .watched_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(watched, vec![path_string(&music)]);
    let (album, is_skipped) = scanned_folder(&f, &path_string(&skipped)).await;
    assert_eq!(album.watched_folder_path, path_string(&music));
    assert!(is_skipped, "the skip stays");
    let (release, _) = scanned_folder(&f, &combined).await;
    assert_eq!(
        release.watched_folder_path,
        path_string(&music),
        "the box is still read as one release, under the same key"
    );
    let state = f
        .library_manager
        .load_import_candidate_state(&picked_hash)
        .await
        .unwrap()
        .expect("the picked album keeps its state");
    assert_eq!(
        state.metadata_provenance,
        Some(bae_core::import::MetadataProvenance::FileMetadata),
        "the pick stays"
    );
}

/// Removing a watched folder forgets what was decided under it, so adding it
/// again starts fresh.
#[tokio::test]
async fn removing_a_watched_folder_forgets_what_was_decided_under_it() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let artist = f.temp_path().join("Music/Artist");
    let album = album_dir(&f, "Music/Artist/Album");
    f.handle.add_watched_folder(path_string(&artist)).await.unwrap();
    wait_for_reads(&f, "the folder is read", &[&artist]).await;
    f.handle
        .set_candidate_skipped(path_string(&album), true)
        .await
        .unwrap();

    f.handle
        .remove_watched_folder(path_string(&artist))
        .await
        .unwrap();
    assert!(f
        .library_manager
        .load_skipped_import_candidates(&path_string(&artist))
        .await
        .unwrap()
        .is_empty());
    f.handle.add_watched_folder(path_string(&artist)).await.unwrap();
    wait_for_reads(&f, "the folder is read again", &[&artist]).await;

    let (_, is_skipped) = scanned_folder(&f, &path_string(&album)).await;
    assert!(!is_skipped, "the skip went with the folder");
}

/// Refreshing a watched root that disappeared succeeds, records the failed
/// status, and keeps the candidates rather than treating the missing folder as
/// removals.
#[tokio::test]
async fn refresh_missing_watched_folder_records_failure_and_preserves_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("unplugged-drive");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();
    let album_key = album.to_string_lossy().into_owned();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    f.handle
        .refresh_watched_folder(root_key.clone())
        .await
        .unwrap();

    fs::remove_dir_all(&root).unwrap();
    f.handle
        .refresh_watched_folder(root_key.clone())
        .await
        .expect("the refresh ran; what it found is the folder's status");
    let projection = wait_for_candidates(&f, "the failed refresh leaves its status", |projection| {
        projection
            .folder_scans
            .statuses
            .iter()
            .any(|status| {
                matches!(
                    status.status,
                    bae_core::import::FolderScanStatus::Failed { .. }
                )
            })
    })
    .await;
    assert!(candidate_rows(&projection)
        .iter()
        .any(|row| row.candidate_key == album_key));
    assert!(projection.folder_scans.statuses.iter().any(|status| {
        status.watched_folder_path == root_key
            && matches!(
                status.status,
                bae_core::import::FolderScanStatus::Failed { .. }
            )
    }));
}

/// `set_candidate_skipped` moves the candidate between the Pending and Skipped
/// tabs and broadcasts `CandidateSkipChanged`; repeating it emits nothing.
#[tokio::test]
async fn set_candidate_skipped_flips_flag_and_is_idempotent() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(batch.added.contains(&album_key));

    async fn wait_for_skipped(f: &ImportFixture, album: &std::path::Path, expected: bool) {
        let tab = if expected {
            bae_core::import::TriageTab::Skipped
        } else {
            bae_core::import::TriageTab::Pending
        };
        let key = album.to_string_lossy().into_owned();
        wait_for_tab(f, "the candidate's skip flag", tab, |projection| {
            candidate_rows(projection)
                .iter()
                .any(|row| row.candidate_key == key)
        })
        .await;
    }
    wait_for_skipped(&f, &album, false).await;

    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    wait_for_skipped(&f, &album, true).await;
    wait_for_scan_event(
        &mut scan_rx,
        "the CandidateSkipChanged broadcast",
        |event| {
            matches!(
                event,
                ScanEvent::CandidateSkipChanged { candidate_key, skipped }
                    if candidate_key == &album_key && *skipped
            )
        },
    )
    .await;

    // A repeated skip emits nothing, so the un-skip after it is the next skip
    // change broadcast.
    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    f.handle
        .set_candidate_skipped(album_key.clone(), false)
        .await
        .unwrap();
    let mut next_skip = None;
    wait_for_scan_event(&mut scan_rx, "the un-skip's broadcast", |event| match event {
        ScanEvent::CandidateSkipChanged {
            candidate_key,
            skipped,
        } if candidate_key == &album_key => {
            next_skip = Some(*skipped);
            true
        }
        _ => false,
    })
    .await;
    assert_eq!(
        next_skip,
        Some(false),
        "a redundant skip must not re-broadcast"
    );
    wait_for_skipped(&f, &album, false).await;
}

/// A Done row is the row the folder was in Found, its import status naming
/// the release this device imported from it. Done is keyed by the folder's
/// hash: renamed on disk, the folder is Done under its new name, and deleting
/// the release puts it back in Found.
#[tokio::test]
async fn a_done_row_follows_its_folder_and_its_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        "Album",
        "Artist",
        None,
        &[TaggedTrack {
            filename: "01 Track.flac",
            title: "Track",
            track_number: 1,
        }],
    );
    let album_key = album.to_string_lossy().into_owned();
    let collection_key = collection.to_string_lossy().into_owned();

    f.handle
        .add_watched_folder(collection_key.clone())
        .await
        .unwrap();
    wait_for_candidates(&f, "the scanned candidate", |projection| {
        candidate_rows(projection)
            .iter()
            .any(|row| row.candidate_key == album_key)
    })
    .await;

    f.handle
        .select_candidate_file_tags(album_key.clone())
        .await
        .unwrap();
    let found = wait_for_candidates(&f, "the row reading its tags", |projection| {
        candidate_rows(projection)
            .iter()
            .any(|row| row.candidate_key == album_key && row.metadata_summary.is_some())
    })
    .await;
    let found = candidate_rows(&found)
        .into_iter()
        .find(|row| row.candidate_key == album_key)
        .cloned()
        .unwrap();
    let import_id = f.handle.start_import(&album_key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    let release = bae_core::import::ImportedRelease {
        release_id: release_id.clone(),
        album_id,
    };

    let done = done_row(&f, "the imported folder", &album_key).await;
    assert_eq!(
        done.import_status,
        Some(bae_core::import::TriageImportStatus::Complete {
            release: release.clone()
        })
    );
    assert_eq!(done.metadata_summary, found.metadata_summary);
    assert_eq!(done.reading, found.reading);
    assert_eq!(done.cover, found.cover);
    assert_eq!(done.folder_name, found.folder_name);

    let renamed = collection.join("Renamed Album");
    fs::rename(&album, &renamed).unwrap();
    let renamed_key = renamed.to_string_lossy().into_owned();
    f.handle
        .refresh_watched_folder(collection_key.clone())
        .await
        .unwrap();
    let moved = done_row(&f, "the renamed folder", &renamed_key).await;
    assert_eq!(
        moved.import_status,
        Some(bae_core::import::TriageImportStatus::Complete { release })
    );

    f.library_manager.delete_release(&release_id).await.unwrap();
    wait_for_candidates(&f, "the folder back in Found", |projection| {
        candidate_rows(projection)
            .iter()
            .any(|row| row.candidate_key == renamed_key && row.import_status.is_none())
    })
    .await;
}

/// The Done row for `key`, once one read of the Done tab holds it.
async fn done_row(f: &ImportFixture, what: &str, key: &str) -> bae_core::import::TriageRow {
    let done = wait_for_tab(f, what, bae_core::import::TriageTab::Done, |list| {
        candidate_rows(list).iter().any(|row| row.candidate_key == key)
    })
    .await;
    candidate_rows(&done)
        .into_iter()
        .find(|row| row.candidate_key == key)
        .cloned()
        .unwrap()
}

/// A folder is Done when this device imported its files: each imported
/// folder's row names its own release, and a copy whose files hash alike —
/// the same files, to the hash — names the release imported from them.
#[tokio::test]
async fn a_folder_whose_files_this_device_imported_is_done() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let titles = ["First", "Second", "Third"];
    let mut keys = Vec::new();
    for title in titles {
        let album = collection.join(format!("Artist - {title}"));
        fs::create_dir_all(&album).unwrap();
        generate_tagged_album_files(
            &album,
            title,
            "Artist",
            None,
            &[TaggedTrack {
                filename: "01 Track.flac",
                title: "Track",
                track_number: 1,
            }],
        );
        keys.push(album.to_string_lossy().into_owned());
    }
    // A copy that keeps the modification times hashes like the original.
    let original = collection.join("Artist - First");
    let copy = collection.join("Backup of Artist - First");
    fs::create_dir_all(&copy).unwrap();
    for entry in fs::read_dir(&original).unwrap() {
        let source = entry.unwrap().path();
        let target = copy.join(source.file_name().unwrap());
        fs::copy(&source, &target).unwrap();
        fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(fs::metadata(&source).unwrap().modified().unwrap()),
            )
            .unwrap();
    }
    let copy_key = copy.to_string_lossy().into_owned();

    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    wait_for_candidates(&f, "every scanned candidate", |projection| {
        candidate_rows(projection).len() == 4
    })
    .await;

    let mut release_ids = Vec::new();
    for key in &keys {
        f.handle.select_candidate_file_tags(key.clone()).await.unwrap();
        let import_id = f.handle.start_import(key).await.unwrap().expect_queued();
        let mut progress_rx = f.handle.subscribe_import(import_id);
        release_ids.push(support::wait_for_import_complete(&mut progress_rx).await.0);
    }

    // One read of the Done tab holding all four folders.
    let done = wait_for_tab(&f, "every folder Done", bae_core::import::TriageTab::Done, |list| {
        candidate_rows(list).len() == 4
    })
    .await;
    let imported_release = |key: &str| {
        let row = candidate_rows(&done)
            .into_iter()
            .find(|row| row.candidate_key == key)
            .unwrap_or_else(|| panic!("{key} is Done"));
        match &row.import_status {
            Some(bae_core::import::TriageImportStatus::Complete { release }) => {
                release.release_id.clone()
            }
            other => panic!("{key} is Done with {other:?}"),
        }
    };
    for (key, release_id) in keys.iter().zip(&release_ids) {
        assert_eq!(&imported_release(key), release_id, "{key} is its own import");
        let pane = f.handle.candidate_pane(key).await.unwrap().unwrap();
        assert!(
            matches!(
                &pane.import_status,
                Some(bae_core::import::CandidateImportStatus::Complete { release })
                    if &release.release_id == release_id
            ),
            "{key}'s pane is its own import"
        );
    }
    assert_eq!(
        imported_release(&copy_key),
        release_ids[0],
        "the copy's files are the ones imported from the original"
    );
}
