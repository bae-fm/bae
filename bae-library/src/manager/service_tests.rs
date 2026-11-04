//! The manager's service: work run apart from its caller that panics,
//! closing, emptying the transfer queues as the library opens, the
//! upload transfer watches, and the import list a surface opens over the
//! library.

use super::*;
use crate::manager::tests::*;
use crate::test_libraries::TestLibrary;
use crate::{LiveRead, LiveReadError};
use coven::MemoryStorage;
use tempfile::TempDir;

/// No transfer outlives the process that ran it: a library opened over a
/// store whose queues still hold entries opens with both empty and running.
#[tokio::test]
async fn opening_a_library_empties_its_transfer_queues() {
    let temp_dir = TempDir::new().unwrap();
    let app_dir = bae_app_dir::AppDir::at(temp_dir.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let handle = bae_db::configure_store(installation.builder(), crate::SCHEMA)
        .clock(clock.clone())
        .storage_connector(Arc::new(MemoryStorage::builder().clock(clock.clone()).build().unwrap()))
        .create_store("Test Library")
        .await
        .unwrap();
    let left =
        Database::from_handle(handle.clone(), clock.clone(), std::sync::Arc::new(coven::UuidIds));
    let left = bae_transfers::Downloads::new(left);
    left.enqueue_all(vec![bae_transfers::DownloadEntry {
        release_id: REL_1.to_string(),
        created_at: 0,
        payload: (),
    }])
    .await
    .unwrap();
    left.set_paused(true).await.unwrap();

    let manager = TestLibrary::open_with(
        Database::from_handle(handle, clock.clone(), Arc::new(coven::UuidIds)),
        app_dir,
        bae_catalogs::providers::Providers::offline(),
        clock,
        Arc::new(coven::UuidIds),
        bae_diagnostics::Diagnostics::noop(),
    )
    .await;

    let queue = manager.download_snapshot().await.unwrap();
    assert!(queue.rows.is_empty(), "{queue:?}");
    assert!(!queue.paused);
}

// The import list a surface opens through the manager: its Done tab read
// beside coven's upload queue, and the list's own life.

/// Four imported releases on Done — A, then C and B with uploads queued in
/// that order, then D, written in that order — under a watched folder, each
/// listed for the folder it was imported from.
async fn done_with_two_uploading(
    manager: &TestManager,
    dir: &std::path::Path,
) -> Vec<(String, bae_import::CandidateKey)> {
    connect_test_cloud(manager).await;
    let files: &[(&str, &[u8])] = &[("track.flac", b"track-bytes")];
    let a = insert_local_release_with_files(manager, &dir.join("a"), "A", files).await;
    let c = insert_release_with_queued_uploads(manager, &dir.join("c"), "C", files).await;
    let b = insert_release_with_queued_uploads(manager, &dir.join("b"), "B", files).await;
    let d = insert_local_release_with_files(manager, &dir.join("d"), "D", files).await;
    let root = bae_db::watched_path::host_root("/music");
    let root_sql = format!("X'{}'", hex::encode(coven::path_bytes(&root)));
    let mut sql = format!(
        "INSERT INTO watched_import_folders (path, position) VALUES ({root_sql}, 0);
         INSERT INTO folder_scan_roots (watched_folder_path, generation, status, volume)
         VALUES ({root_sql}, 1, 'complete', 'local');"
    );
    let mut keys = Vec::new();
    for (name, release) in [("A", &a), ("B", &b), ("C", &c), ("D", &d)] {
        let key = bae_import::CandidateKey::folder(&root.join(name));
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
    manager.database.execute_local_sql_for_test(&sql).await.unwrap();
    keys
}

/// The release names one read of a list shows, in order.
fn done_names(projection: &bae_import::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            bae_import::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

/// A list opened as the import handle opens one: closing the library ends
/// it.
async fn open_list(
    manager: &TestLibrary,
    view: bae_import::ImportListView,
    windows: bae_db::LibraryPageWindows,
) -> Result<bae_import::LiveImportList, bae_import::ImportError> {
    manager.start_import().await?.open_list(view, windows).await
}

/// One read of `list` through `offset` and `limit`.
async fn read_list(
    database: &bae_db::Database,
    list: bae_import::ImportListId,
    offset: u64,
    limit: u64,
) -> bae_import::ImportListProjection {
    let tables = bae_import::tables::ImportTables::new(database.clone());
    tables
        .load_import_list(bae_import::ImportListRequest {
            list,
            windows: std::iter::once(bae_db::LibraryPageWindow { offset, limit }).collect(),
        })
        .await
        .unwrap()
}

/// Done runs the releases coven still has uploads queued for first, the first
/// to go first, and then the rest in the view's order: whichever order the
/// view asks for, what is still uploading leads.
#[tokio::test]
async fn done_runs_the_releases_still_uploading_first() {
    let (manager, temp_dir) = setup_test_manager().await;
    let tables = bae_import::tables::ImportTables::new(manager.database.clone());
    let keys = done_with_two_uploading(&manager, temp_dir.path()).await;
    let done = |order| bae_import::ImportListView {
        tab: bae_import::TriageTab::Done,
        order,
        ..bae_import::ImportListView::default()
    };
    for (order, expected) in [
        (bae_import::ImportListOrder::NewestFirst, ["C", "B", "D", "A"]),
        (bae_import::ImportListOrder::OldestFirst, ["C", "B", "A", "D"]),
        (bae_import::ImportListOrder::PathAscending, ["C", "B", "A", "D"]),
        (bae_import::ImportListOrder::PathDescending, ["C", "B", "D", "A"]),
    ] {
        let list = tables.open_import_list(done(order)).await.unwrap();
        assert_eq!(
            done_names(&read_list(&manager.database, list, 0, 50).await),
            expected,
            "{order:?}"
        );
    }

    let list =
        tables.open_import_list(done(bae_import::ImportListOrder::NewestFirst)).await.unwrap();
    let across = read_list(&manager.database, list, 1, 2).await;
    assert_eq!(done_names(&across), ["B", "D"], "a window across the two parts");
    assert_eq!(across.total_count, 4);
    assert_eq!(done_names(&read_list(&manager.database, list, 3, 5).await), ["A"]);
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    let location =
        tables.locate_import_candidate(list, &key("A")).await.unwrap().expect("A is on Done");
    assert_eq!(location.visible_position, 3);
    tables
        .change_import_list_selection(
            list,
            bae_import::selection::SelectionChange::Replace { keys: vec![key("A"), key("B")] },
        )
        .await
        .unwrap();
    tables
        .set_import_list_view(list, done(bae_import::ImportListOrder::OldestFirst))
        .await
        .unwrap();
    let selected = read_list(&manager.database, list, 0, 0).await;
    assert_eq!(selected.summary.opened_at, 1, "the view opens at B, still uploading, behind C");
    assert_eq!(
        tables
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
    windows: &[bae_import::ImportListWindow],
) -> Vec<(String, Option<bae_db::UploadProgress>)> {
    windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            bae_import::ImportListItem::Candidate { row, .. } => {
                Some((row.folder_name.clone(), row.upload.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A Done row carries how its release's upload stands, as the Storage
/// Manager counts it: its files waiting, then one being sent, and nothing
/// once they are stored. The list's live read follows each step.
#[tokio::test]
async fn a_done_row_carries_its_release_upload() {
    let (manager, temp_dir) = setup_test_manager().await;
    done_with_two_uploading(&manager, temp_dir.path()).await;
    let subscription = open_list(
        &manager,
        bae_import::ImportListView {
            tab: bae_import::TriageTab::Done,
            order: bae_import::ImportListOrder::PathAscending,
            ..bae_import::ImportListView::default()
        },
        std::iter::once(bae_db::LibraryPageWindow { offset: 0, limit: 50 }).collect(),
    )
    .await
    .unwrap();
    let waiting = Some(bae_db::UploadProgress { queued: 1, uploading: 0 });
    assert_eq!(
        done_uploads(&subscription.next().await.unwrap().windows),
        [
            ("C".to_string(), waiting.clone()),
            ("B".to_string(), waiting),
            ("A".to_string(), None),
            ("D".to_string(), None),
        ]
    );

    // Held in flight until its start is read.
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    manager
        .storage
        .hold_next_creation(
            coven::ObjectPrefix::files(manager.database.store_id_for_test()),
            started,
            resumed,
        )
        .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = Some(bae_db::UploadProgress { queued: 0, uploading: 1 });
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while !done_uploads(&subscription.next().await.unwrap().windows)
            .iter()
            .any(|(_, upload)| *upload == sending)
        {}
    })
    .await
    .expect("a row counts its file being sent");
    created.await.unwrap();
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
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
#[tokio::test]
async fn extending_on_done_selects_the_run_across_its_two_parts() {
    let (manager, temp_dir) = setup_test_manager().await;
    let tables = bae_import::tables::ImportTables::new(manager.database.clone());
    let keys = done_with_two_uploading(&manager, temp_dir.path()).await;
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    let list = tables
        .open_import_list(bae_import::ImportListView {
            tab: bae_import::TriageTab::Done,
            order: bae_import::ImportListOrder::NewestFirst,
            ..bae_import::ImportListView::default()
        })
        .await
        .unwrap();
    tables
        .change_import_list_selection(
            list,
            bae_import::selection::SelectionChange::Extend { from: key("A"), to: key("B") },
        )
        .await
        .unwrap();
    assert_eq!(
        tables
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
    let subscription = open_list(
        &manager,
        bae_import::ImportListView::default(),
        bae_db::LibraryPageWindows::default(),
    )
    .await
    .unwrap();
    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert!(initial.windows.is_empty());

    subscription
        .set_windows(std::iter::once(bae_db::LibraryPageWindow { offset: 0, limit: 50 }).collect())
        .expect("the first window is requested");
    let requested = subscription.next().await.expect("the requested value");
    assert_eq!(requested.cause, coven::LiveQueryCause::Request);
    assert_eq!(requested.windows.len(), 1);
    assert_eq!(requested.view_revision, initial.view_revision);

    let revision = subscription
        .set_view(bae_import::ImportListView {
            tab: bae_import::TriageTab::Done,
            ..bae_import::ImportListView::default()
        })
        .await
        .unwrap();
    let moved = subscription.next().await.expect("the view's value");
    assert_eq!(moved.view_revision, revision);
    assert_eq!(moved.summary.narrowing.tab, bae_import::TriageTab::Done);
}

/// Cancelling settles a pending read and closes the list: its row goes, and a
/// later change to it is refused.
#[tokio::test]
async fn cancelling_closes_the_list() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let subscription = open_list(
        &manager,
        bae_import::ImportListView::default(),
        bae_db::LibraryPageWindows::default(),
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
        subscription.set_view(bae_import::ImportListView::default()).await,
        Err(bae_import::LiveImportListError::Database(_))
    ));
    assert!(matches!(subscription.next().await, Err(bae_import::LiveImportListError::Cancelled)));
}

/// Closing the library ends a list's waiting read before the store closes:
/// the read answers that the list was cancelled, never that the store it read
/// is closed. The app quits with the import tab's read waiting like this.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_cancels_a_waiting_read_before_the_store_closes() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let subscription = Arc::new(
        open_list(
            &manager,
            bae_import::ImportListView::default(),
            bae_db::LibraryPageWindows::default(),
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

    let read = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, waiting)
        .await
        .expect("the waiting read ends")
        .unwrap();
    assert!(
        matches!(read, Err(bae_import::LiveImportListError::Cancelled)),
        "the waiting read is cancelled: {read:?}"
    );
    assert!(
        matches!(subscription.next().await, Err(bae_import::LiveImportListError::Cancelled)),
        "a read after the close is cancelled too"
    );
}

/// A live read whose task panics ends with the failure, which its reader
/// tells apart from its own cancel, and the panic is reported once.
#[tokio::test]
async fn a_live_read_whose_task_panics_ends_with_its_failure() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;
    let (requests, _) = tokio::sync::watch::channel(());
    let (values, read) = crate::live_read_channel::<()>();
    let task = crate::CancellationToken::new();
    manager.spawn_stoppable_subscription_task(
        "test subscription",
        task.clone(),
        values.clone(),
        async move {
            let _values = values;
            panic!("the subscription broke");
        },
    );
    let read = LiveRead::new(requests, read, task);

    let ended = read.next().await;

    assert!(matches!(ended, Err(LiveReadError::Failed(LibraryError::Internal(_)))), "{ended:?}");
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
}

/// A file operation that panics fails as something that went wrong, is
/// reported once, and leaves the library able to close.
#[tokio::test]
async fn a_file_operation_that_panics_fails_and_is_reported() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;

    let error = manager
        .run_file_operation("test operation", |_closing| async { broken_operation() })
        .await
        .expect_err("a panicked operation fails");

    assert!(matches!(error, LibraryError::Internal(_)), "{error}");
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
    manager.close().await.expect("the library closes");
}

fn broken_operation() -> Result<(), LibraryError> {
    panic!("the operation broke")
}

/// Work asked of a closed library starts nothing, and the caller hears so
/// rather than a log line: starting sync after close is refused.
#[tokio::test]
async fn starting_work_after_close_is_refused_to_the_caller() {
    let (manager, _temp) = setup_test_manager().await;
    manager.close().await.unwrap();
    let error = manager.start_sync_at_startup().expect_err("a closed library starts no work");
    assert!(matches!(error, LibraryError::Closed("startup sync attachment")), "got {error:?}");
    assert!(matches!(manager.start(), Err(LibraryError::Closed(_))));
}

#[tokio::test]
async fn close_waits_for_a_file_operations_rollback_and_refuses_new_work() {
    let (manager, _home) = setup_test_manager().await;
    let (entered, reached) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let database = manager.database.clone();
    let (admitted, admission) = tokio::sync::oneshot::channel();
    let working = manager.clone();
    let operation = tokio::spawn(async move {
        working
            .run_file_operation("rollback test", move |closing| async move {
                admitted.send(()).unwrap();
                closing.cancelled().await;
                entered.send(()).unwrap();
                released.await.unwrap();
                database.store_name().await?;
                Ok(())
            })
            .await
    });
    admission.await.unwrap();
    let closing = manager.clone();
    let close = tokio::spawn(async move { closing.close().await });
    reached.await.unwrap();
    // Close waits for the rollback: the operation's read of the database
    // after the release fails if close had closed it first.
    assert!(manager.run_file_operation("late operation", |_| async { Ok(()) }).await.is_err());
    release.send(()).unwrap();
    operation.await.unwrap().unwrap();
    close.await.unwrap().unwrap();
}

// How far and how fast the files being sent go, as the transfer watches
// tell them from the live query of the files being sent and coven's upload
// watch.

/// What a reader of the transfer watch holds after applying `changes`.
fn apply_transfer_changes(
    held: &mut Option<crate::OutboxTransfers>,
    changes: Vec<crate::OutboxTransferChange>,
) {
    use crate::OutboxTransferChange;
    use bae_transfers::ReleaseTransferChange;
    for change in changes {
        match change {
            OutboxTransferChange::Release(ReleaseTransferChange::Reset(releases)) => {
                *held = Some(crate::OutboxTransfers {
                    releases,
                    total: crate::UploadTransfer { bar: None, throughput_bps: 0 },
                });
            }
            OutboxTransferChange::Release(ReleaseTransferChange::Updated {
                release_id,
                progress,
            }) => {
                held.as_mut().expect("a reset comes first").releases.insert(release_id, progress);
            }
            OutboxTransferChange::Release(ReleaseTransferChange::Removed { release_id }) => {
                held.as_mut().expect("a reset comes first").releases.remove(&release_id);
            }
            OutboxTransferChange::Total(total) => {
                held.as_mut().expect("a reset comes first").total = total;
            }
        }
    }
}

/// Only the releases with files being sent have a transfer: files that only
/// wait have none. While a file is sent the total has a bar, and
/// once every file is sent the file and its release leave and the bar goes.
#[tokio::test]
async fn the_transfers_name_only_the_files_being_sent() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a"), ("b.flac", b"bytes-b")],
    )
    .await;
    manager.database.set_transfer_limits(coven::TransferLimits {
        uploads: std::num::NonZeroUsize::new(1).unwrap(),
        ..manager.database.transfer_limits()
    });
    let mut transfers = manager.watch_outbox_transfers();
    let mut held = None;
    apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
    assert_eq!(
        held,
        Some(crate::OutboxTransfers {
            releases: Default::default(),
            total: crate::UploadTransfer { bar: None, throughput_bps: 0 },
        }),
        "files that only wait have no transfer"
    );

    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.database.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
            let current = held.as_ref().unwrap();
            if current.total.bar.is_some() {
                break current.clone();
            }
        }
    })
    .await
    .expect("the transfers tell the file being sent");
    assert!(
        sending.releases.contains_key(&release.id),
        "the release of the file being sent has its transfer: {sending:?}"
    );

    created.await.unwrap();
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let current = held.as_ref().unwrap();
            if current.releases.is_empty() && current.total.bar.is_none() {
                break;
            }
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
        }
    })
    .await
    .expect("every file sent, nothing has a transfer");
}

/// The total's watch tells the files being sent together as they stand
/// first, and then only as they change.
#[tokio::test]
async fn the_total_transfer_is_told_first_and_then_as_it_changes() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut total = manager.watch_outbox_total_transfer();
    let idle = crate::UploadTransfer { bar: None, throughput_bps: 0 };
    assert_eq!(total.next().await.unwrap().unwrap(), idle);

    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.database.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, total.next())
        .await
        .expect("the file being sent moves the total")
        .unwrap()
        .unwrap();
    assert!(sending.bar.is_some(), "{sending:?}");
    created.await.unwrap();
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        let mut told = sending;
        while told != idle {
            let next = total.next().await.unwrap().unwrap();
            assert_ne!(next, told, "the total is told only as it changes");
            told = next;
        }
    })
    .await
    .expect("the file sent leaves the total idle");
}

/// A release being sent reads its bytes against its whole size from the
/// moment its send starts: its stored files count as done, and a file sent in
/// one request counts its share as its object goes, before the provider
/// answers. Its file's own share is told by file.
#[tokio::test]
async fn a_file_sent_in_one_request_moves_its_release_as_it_goes() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut transfers = manager.watch_outbox_transfers();
    let mut files = manager.watch_upload_file_transfers();
    let mut held = None;
    apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());

    // The memory provider takes a request's bytes as it starts, then holds
    // the answer.
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.database.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let bar = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            apply_transfer_changes(&mut held, transfers.next().await.unwrap().unwrap());
            let bar = held.as_ref().unwrap().releases.get(&release.id).map(|transfer| transfer.bar);
            if bar.is_some_and(|bar| bar.bytes_done > 0) {
                break bar.unwrap();
            }
        }
    })
    .await
    .expect("the release moves while its one request goes");
    assert_eq!(
        bar,
        crate::UploadBar { bytes_done: 7, bytes_total: 7 },
        "the file's whole share counts once its request has taken it"
    );
    let fraction = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        let mut held = std::collections::BTreeMap::new();
        loop {
            for change in files.next().await.unwrap() {
                match change {
                    crate::FileTransferChange::Reset(all) => held = all,
                    crate::FileTransferChange::Updated { file, fraction } => {
                        held.insert(file, fraction);
                    }
                    crate::FileTransferChange::Removed { file } => {
                        held.remove(&file);
                    }
                }
            }
            if let Some(fraction) = held.values().next() {
                break *fraction;
            }
        }
    })
    .await
    .expect("the file being sent is told by file");
    assert_eq!(fraction, 1.0);
    created.await.unwrap();
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
}

/// A test manager whose bugs go to the returned reporter.
async fn setup_test_manager_reporting_bugs(
) -> (TestManager, TempDir, Arc<bae_diagnostics::RecordingBugReporter>) {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let (manager, temp_dir) = setup_test_manager_with(
        Arc::new(coven::SystemClock),
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    )
    .await;
    (manager, temp_dir, bugs)
}
