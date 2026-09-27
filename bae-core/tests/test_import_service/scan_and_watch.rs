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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();

    let mut candidates = vec![];
    support::next_matching(
        &mut scan_rx,
        std::time::Duration::from_secs(5),
        |event| match event {
            ScanEvent::FolderCandidate { candidate, .. } => {
                candidates.push(candidate);
                None
            }
            ScanEvent::Finished => Some(()),
            _ => None,
        },
    )
    .await
    .expect("Scan did not finish within 5s");

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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
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
/// candidates added and removed along the way; panics after the deadline.
async fn scan_batch_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut done: impl FnMut(&ScanEvent) -> bool,
) -> ScanBatch {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    support::next_matching(rx, std::time::Duration::from_secs(10), |event| {
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
    .unwrap_or_else(|| panic!("timed out after 10s waiting for {what}"));
    ScanBatch { added, removed }
}

/// The first candidate list `accept` admits, within the test deadline.
async fn wait_for_candidates(
    f: &ImportFixture,
    what: &str,
    accept: impl FnMut(&bae_core::import::ImportListSnapshot) -> bool,
) -> bae_core::import::ImportListSnapshot {
    wait_for_tab(f, what, bae_core::import::TriageTab::Pending, accept).await
}

/// The first read of one tab that `accept` admits, within the test deadline.
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
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        f.handle.wait_for_list(view, accept),
    )
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

/// Wait for a scan event matching `pred`; panics after the deadline.
async fn wait_for_scan_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut pred: impl FnMut(&ScanEvent) -> bool,
) {
    support::next_matching(rx, std::time::Duration::from_secs(10), |event| {
        pred(&event).then_some(())
    })
    .await
    .unwrap_or_else(|| panic!("timed out after 10s waiting for {what}"));
}

/// Every scan event that arrives within `window`, for asserting that an event
/// does not arrive.
async fn drain_scan_events(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    window: std::time::Duration,
) -> Vec<ScanEvent> {
    let mut events = Vec::new();
    // The predicate never accepts, so this runs the whole window.
    support::next_matching(rx, window, |event| {
        events.push(event);
        None::<()>
    })
    .await;
    events
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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
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

    f.handle
        .remove_watched_folder(collection_key.clone())
        .await
        .unwrap();

    // The watched list changes at once; the candidate list once its query
    // reads again.
    assert!(
        f.handle.watched_folders().await.unwrap().is_empty(),
        "removed folder is gone from the persisted list"
    );
    wait_for_candidates(
        &f,
        "the list dropped the removed folder's candidates",
        |projection| candidate_rows(projection).is_empty(),
    )
    .await;

    // The empty list is broadcast.
    wait_for_scan_event(
        &mut scan_rx,
        "the shortened folder-list broadcast",
        |event| matches!(event, ScanEvent::WatchedFoldersChanged { folders } if folders.is_empty()),
    )
    .await;

    // The watch stopped: a new release folder under the root is not found.
    let new_album = collection.join("Artist - Second Album");
    fs::create_dir_all(&new_album).unwrap();
    generate_album_files(&new_album, &["01 Track.flac"]);
    let after_unwatch = drain_scan_events(&mut scan_rx, std::time::Duration::from_secs(2)).await;
    assert!(
        !after_unwatch
            .iter()
            .any(|event| matches!(event, ScanEvent::FolderCandidate { candidate: _, .. })),
        "an unwatched folder must not surface new candidates, got {after_unwatch:?}",
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

    f.db.rename_host_table_for_test("import_candidate_file_edit")
        .await
        .unwrap();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();

    let projection = wait_for_candidates(
        &f,
        "the unreadable file decisions leave the root failed",
        |projection| {
            projection.folder_scans.statuses.iter().any(|status| {
                status.watched_folder_path == root_key
                    && matches!(
                        status.status,
                        bae_core::import::FolderScanStatus::Failed { .. }
                    )
            })
        },
    )
    .await;
    let status = projection
        .folder_scans
        .statuses
        .iter()
        .find(|status| status.watched_folder_path == root_key)
        .expect("the added root reports a scan status");
    let bae_core::import::FolderScanStatus::Failed { error } = &status.status else {
        panic!("expected a failed scan, got {:?}", status.status);
    };
    assert!(
        error.contains("import_candidate_file_edit"),
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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
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
        .select_candidate_metadata_provenance(
            path_string(&picked),
            MetadataProvenance::FileMetadata,
        )
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
        Some(MetadataProvenance::FileMetadata),
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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
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

    // A repeated skip emits nothing.
    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    wait_for_skipped(&f, &album, true).await;
    let events = drain_scan_events(&mut scan_rx, std::time::Duration::from_millis(300)).await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ScanEvent::CandidateSkipChanged { .. })),
        "a redundant skip must not re-broadcast, got {events:?}",
    );

    f.handle
        .set_candidate_skipped(album_key.clone(), false)
        .await
        .unwrap();
    wait_for_skipped(&f, &album, false).await;
}

/// A Done row shows the library release its import became, and an edit to
/// that release reaches the row through the open list subscription.
#[tokio::test]
async fn a_done_row_follows_the_library_release_it_became() {
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

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    wait_for_candidates(&f, "the scanned candidate", |projection| {
        candidate_rows(projection)
            .iter()
            .any(|row| row.candidate_key == album_key)
    })
    .await;

    f.handle
        .select_candidate_metadata_provenance(album_key.clone(), MetadataProvenance::FileMetadata)
        .await
        .unwrap();
    let import_id = f.handle.start_import(&album_key).await.unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let done = f.handle.subscribe_whole_list(bae_core::import::ImportListView {
        tab: bae_core::import::TriageTab::Done,
        ..bae_core::import::ImportListView::default()
    });
    let imported = next_imported_row(&done, &album_key, |_| true).await;
    assert_eq!(imported.release.release_id, release_id);
    assert_eq!(imported.release.title, "Album");
    assert_eq!(imported.release.artist.as_deref(), Some("Artist"));

    let mut form = f
        .library_manager
        .release_edit_seed(&release_id)
        .await
        .unwrap()
        .edit;
    form.album_title = "Album (Edited)".to_string();
    f.library_manager
        .apply_release_metadata_user_edit(&release_id, &form.shape().unwrap())
        .await
        .unwrap();

    let edited = next_imported_row(&done, &album_key, |row| {
        row.release.title != "Album"
    })
    .await;
    assert_eq!(edited.release.title, "Album (Edited)");
    assert_eq!(edited.release.artist.as_deref(), Some("Artist"));
}

/// The next Done row for `key` that `accept` admits, read off one open list
/// subscription.
async fn next_imported_row(
    subscription: &bae_core::import::ImportListSubscription,
    key: &str,
    mut accept: impl FnMut(&bae_core::import::ImportedRow) -> bool,
) -> bae_core::import::ImportedRow {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = subscription
                .next()
                .await
                .expect("the import list query stays open");
            let row = snapshot
                .windows
                .iter()
                .flat_map(|window| &window.items)
                .find_map(|item| match item {
                    bae_core::import::ImportListItem::Imported { row } if row.candidate_key == key => {
                        Some(row.clone())
                    }
                    _ => None,
                });
            if let Some(row) = row.filter(|row| accept(row)) {
                return row;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for the Done row for {key}"))
}
