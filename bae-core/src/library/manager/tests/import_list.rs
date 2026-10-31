// The import list a surface opens through the manager: its Done tab read
// beside coven's upload queue, and the list's own life.
//
// `include!`d into `manager::tests`.

/// Four imported releases on Done — A, then C and B with uploads queued in
/// that order, then D, written in that order — under a watched folder, each
/// listed for the folder it was imported from.
#[cfg(feature = "test-utils")]
async fn done_with_two_uploading(
    manager: &TestManager,
    dir: &std::path::Path,
) -> Vec<(String, crate::import::CandidateKey)> {
    connect_test_cloud(manager).await;
    let files: &[(&str, &[u8])] = &[("track.flac", b"track-bytes")];
    let a = insert_local_release_with_files(manager, &dir.join("a"), "A", files).await;
    let c = insert_release_with_queued_uploads(manager, &dir.join("c"), "C", files).await;
    let b = insert_release_with_queued_uploads(manager, &dir.join("b"), "B", files).await;
    let d = insert_local_release_with_files(manager, &dir.join("d"), "D", files).await;
    let root = crate::import::watched_folder::host_root("/music");
    let root_sql = format!("X'{}'", hex::encode(coven::path_bytes(&root)));
    let mut sql = format!(
        "INSERT INTO watched_import_folders (path, position) VALUES ({root_sql}, 0);
         INSERT INTO folder_scan_roots (watched_folder_path, generation, status, volume)
         VALUES ({root_sql}, 1, 'complete', 'local');"
    );
    let mut keys = Vec::new();
    for (name, release) in [("A", &a), ("B", &b), ("C", &c), ("D", &d)] {
        let key = crate::import::CandidateKey::folder(&root.join(name));
        let path = format!("X'{}'", hex::encode(key.as_bytes()));
        let relative = format!("X'{}'", hex::encode(name.as_bytes()));
        sql.push_str(&format!(
            "INSERT INTO scan_candidate
                 (watched_folder_path, path, generation, kind, name, relative_folder, folder,
                  file_root, scope, content_hash)
             VALUES ({root_sql}, {path}, 1, 'valid', '{name}', {relative}, {path}, {path},
                     'direct', 'hash-{name}');
             INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{id}', 'hash-{name}');",
            id = release.id,
        ));
        keys.push((name.to_string(), key));
    }
    manager
        .database
        .execute_local_sql_for_test(&sql)
        .await
        .unwrap();
    keys
}

/// The release names one read of a list shows, in order.
fn done_names(projection: &crate::import::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

/// One read of `list` through `offset` and `limit`.
async fn read_list(
    manager: &LibraryManager,
    list: crate::import::ImportListId,
    offset: u64,
    limit: u64,
) -> crate::import::ImportListProjection {
    manager
        .load_import_list(crate::import::ImportListRequest {
            list,
            windows: std::iter::once(crate::library::LibraryPageWindow { offset, limit }).collect(),
        })
        .await
        .unwrap()
}

/// Done runs the releases coven still has uploads queued for first, the first
/// to go first, and then the rest in the view's order: whichever order the
/// view asks for, what is still uploading leads.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn done_runs_the_releases_still_uploading_first() {
    let (manager, temp_dir) = setup_test_manager().await;
    let keys = done_with_two_uploading(&manager, temp_dir.path()).await;
    let done = |order| crate::import::ImportListView {
        tab: crate::import::TriageTab::Done,
        order,
        ..crate::import::ImportListView::default()
    };
    for (order, expected) in [
        (crate::import::ImportListOrder::NewestFirst, ["C", "B", "D", "A"]),
        (crate::import::ImportListOrder::OldestFirst, ["C", "B", "A", "D"]),
        (crate::import::ImportListOrder::PathAscending, ["C", "B", "A", "D"]),
        (crate::import::ImportListOrder::PathDescending, ["C", "B", "D", "A"]),
    ] {
        let list = manager.open_import_list(done(order)).await.unwrap();
        assert_eq!(done_names(&read_list(&manager, list, 0, 50).await), expected, "{order:?}");
    }

    let list = manager
        .open_import_list(done(crate::import::ImportListOrder::NewestFirst))
        .await
        .unwrap();
    let across = read_list(&manager, list, 1, 2).await;
    assert_eq!(done_names(&across), ["B", "D"], "a window across the two parts");
    assert_eq!(across.total_count, 4);
    assert_eq!(done_names(&read_list(&manager, list, 3, 5).await), ["A"]);
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    let location = manager
        .locate_import_candidate(list, &key("A"))
        .await
        .unwrap()
        .expect("A is on Done");
    assert_eq!(location.visible_position, 3);
    manager
        .change_import_list_selection(
            list,
            crate::import::selection::SelectionChange::Replace {
                keys: vec![key("A"), key("B")],
            },
        )
        .await
        .unwrap();
    manager
        .set_import_list_view(list, done(crate::import::ImportListOrder::OldestFirst))
        .await
        .unwrap();
    let selected = read_list(&manager, list, 0, 0).await;
    assert_eq!(
        selected.summary.opened_at, 1,
        "the view opens at B, still uploading, behind C"
    );
    assert_eq!(
        manager
            .load_import_list_selected(list)
            .await
            .unwrap()
            .candidates
            .into_iter()
            .map(|candidate| candidate.name)
            .collect::<Vec<_>>(),
        ["B", "A"],
        "the selected rows come in the list's order"
    );
}

/// Each Done row's name and how its release's upload stands, in order.
fn done_uploads(
    windows: &[crate::import::ImportListWindow],
) -> Vec<(String, Option<crate::library::UploadProgress>)> {
    windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => {
                Some((row.folder_name.clone(), row.upload.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A Done row carries how its release's upload stands, as the Storage
/// Manager counts it: its files waiting, then one being sent, and nothing
/// once they are stored. The list's live read follows each step.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_done_row_carries_its_release_upload() {
    let (manager, temp_dir) = setup_test_manager().await;
    done_with_two_uploading(&manager, temp_dir.path()).await;
    let subscription = crate::import::ImportListSubscription::open(
        manager.library.clone(),
        crate::import::ImportListView {
            tab: crate::import::TriageTab::Done,
            order: crate::import::ImportListOrder::PathAscending,
            ..crate::import::ImportListView::default()
        },
        std::iter::once(crate::library::LibraryPageWindow { offset: 0, limit: 50 }).collect(),
    )
    .await
    .unwrap();
    let waiting = Some(crate::library::UploadProgress {
        queued: 1,
        uploading: 0,
    });
    assert_eq!(
        done_uploads(&subscription.next().await.unwrap().windows),
        [
            ("C".to_string(), waiting.clone()),
            ("B".to_string(), waiting),
            ("A".to_string(), None),
            ("D".to_string(), None),
        ]
    );

    // Held in flight long enough that its start is read while it runs.
    manager
        .storage
        .set_faults(coven::Faults {
            delay: std::time::Duration::from_secs(2),
            ..coven::Faults::none()
        })
        .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = Some(crate::library::UploadProgress {
        queued: 0,
        uploading: 1,
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !done_uploads(&subscription.next().await.unwrap().windows)
            .iter()
            .any(|(_, upload)| *upload == sending)
        {}
    })
    .await
    .expect("a row counts its file being sent");
    drain.await.unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while done_uploads(&subscription.next().await.unwrap().windows)
            .iter()
            .any(|(_, upload)| upload.is_some())
        {}
    })
    .await
    .expect("every row's files are stored");
}

/// A run of rows selected on Done takes the rows between its ends across the
/// part still uploading and the rest, both ends included.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn extending_on_done_selects_the_run_across_its_two_parts() {
    let (manager, temp_dir) = setup_test_manager().await;
    let keys = done_with_two_uploading(&manager, temp_dir.path()).await;
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    let list = manager
        .open_import_list(crate::import::ImportListView {
            tab: crate::import::TriageTab::Done,
            order: crate::import::ImportListOrder::NewestFirst,
            ..crate::import::ImportListView::default()
        })
        .await
        .unwrap();
    manager
        .change_import_list_selection(
            list,
            crate::import::selection::SelectionChange::Extend {
                from: key("A"),
                to: key("B"),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        manager
            .load_import_list_selected(list)
            .await
            .unwrap()
            .candidates
            .into_iter()
            .map(|candidate| candidate.name)
            .collect::<Vec<_>>(),
        ["B", "D", "A"],
        "C, B, D, A from B to A"
    );
}

/// The windows are the query's request: asking for one reruns the read as a
/// request change, and leaves the view's revision where it was.
#[tokio::test]
async fn setting_the_windows_reruns_the_list_as_a_request_change() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let subscription = crate::import::ImportListSubscription::open(
        manager.library.clone(),
        crate::import::ImportListView::default(),
        crate::library::LibraryPageWindows::default(),
    )
    .await
    .unwrap();
    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert!(initial.windows.is_empty());

    subscription
        .set_windows(
            std::iter::once(crate::library::LibraryPageWindow {
                offset: 0,
                limit: 50,
            })
            .collect(),
        )
        .expect("the first window is requested");
    let requested = subscription.next().await.expect("the requested value");
    assert_eq!(requested.cause, coven::LiveQueryCause::Request);
    assert_eq!(requested.windows.len(), 1);
    assert_eq!(requested.view_revision, initial.view_revision);

    let revision = subscription
        .set_view(crate::import::ImportListView {
            tab: crate::import::TriageTab::Done,
            ..crate::import::ImportListView::default()
        })
        .await
        .unwrap();
    let moved = subscription.next().await.expect("the view's value");
    assert_eq!(moved.view_revision, revision);
    assert_eq!(moved.summary.narrowing.tab, crate::import::TriageTab::Done);
}

/// Cancelling settles a pending read and closes the list: its row goes, and a
/// later change to it is refused.
#[tokio::test]
async fn cancelling_closes_the_list() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let subscription = crate::import::ImportListSubscription::open(
        manager.library.clone(),
        crate::import::ImportListView::default(),
        crate::library::LibraryPageWindows::default(),
    )
    .await
    .unwrap();
    subscription.next().await.expect("the initial value");

    subscription.cancel().await.unwrap();

    assert!(manager
        .database
        .query_texts_for_test("SELECT CAST(id AS TEXT) FROM import_list")
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        subscription.set_view(crate::import::ImportListView::default()).await,
        Err(crate::import::ImportListSubscriptionError::Library(_))
    ));
    assert!(matches!(
        subscription.next().await,
        Err(crate::import::ImportListSubscriptionError::Cancelled)
    ));
}

/// Closing the library ends a list's waiting read before the store closes:
/// the read answers that the list was cancelled, never that the store it read
/// is closed. The app quits with the import tab's read waiting like this.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_cancels_a_waiting_read_before_the_store_closes() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let subscription = Arc::new(
        crate::import::ImportListSubscription::open(
            manager.library.clone(),
            crate::import::ImportListView::default(),
            crate::library::LibraryPageWindows::default(),
        )
        .await
        .unwrap(),
    );
    subscription.next().await.expect("the initial value");
    let waiting = tokio::spawn({
        let subscription = subscription.clone();
        async move { subscription.next().await }
    });

    manager.library.close().await.unwrap();

    let read = tokio::time::timeout(bae_test_support::HANG_GUARD, waiting)
        .await
        .expect("the waiting read ends")
        .unwrap();
    assert!(
        matches!(read, Err(crate::import::ImportListSubscriptionError::Cancelled)),
        "the waiting read is cancelled: {read:?}"
    );
    assert!(
        matches!(
            subscription.next().await,
            Err(crate::import::ImportListSubscriptionError::Cancelled)
        ),
        "a read after the close is cancelled too"
    );
}
