//! The manager's service: work run apart from its caller that panics,
//! closing, emptying the transfer queues as the library opens, the
//! upload transfer watches, and the import list a surface opens over the
//! library.

use super::*;
use crate::manager::tests::*;
use crate::test_libraries::TestLibrary;
use bae_import::triage::TriageStateLine;
use bae_store::live_query::LiveQueryError;
use bae_util::test_files::TestFolder;
use coven::MemoryStorage;

/// A library opened over a store whose download queue holds an entry opens
/// with that entry and the pause the person left it with.
#[tokio::test]
async fn opening_a_library_keeps_its_download_queue() {
    let app_dir = bae_app_dir::AppDir::temporary().unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let sources = bae_store::TestSources::on(&clock);
    let handle = bae_store::test_devices::create_test_store(
        sources.builder(&installation).storage_connector(Arc::new(
            MemoryStorage::builder().clock(clock.clone()).build().unwrap(),
        )),
        crate::SCHEMA,
        "Test Library",
        "Test Device",
    )
    .await
    .unwrap();
    let left = sources.store(handle.clone());
    let left = bae_transfers::Downloads::new(left);
    left.enqueue_all(vec![bae_transfers::QueuedPin {
        release_id: REL_1.to_string(),
        created_at: 0,
        payload: bae_transfers::DownloadRequest::Pin,
    }])
    .await
    .unwrap();
    left.set_paused(true).await.unwrap();

    let manager = TestLibrary::open(
        sources.store(handle),
        app_dir,
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;

    let queue = manager.manager().download_snapshot().await.unwrap();
    let kept: Vec<_> = queue.rows.iter().map(|row| row.op.release_id.as_str()).collect();
    assert_eq!(kept, vec![REL_1], "{queue:?}");
    assert!(queue.paused);
}

// The import list a surface opens through the manager: its Done tab read
// beside coven's upload queue, and the list's own life.

/// Four releases imported from folders, so on Done — A, then C and B, each
/// queued for upload once imported, then D, imported in that order — each
/// listed for the folder it was imported from.
async fn done_with_two_uploading(
    manager: &TestManager,
    dir: &TestFolder,
) -> Vec<(String, bae_import::CandidateKey)> {
    connect_test_cloud(manager).await;
    let import = manager.library.start_discogs_import().await;
    let mut keys = Vec::new();
    for name in ["A", "C", "B", "D"] {
        let folder = dir.create_folder(dir.canonicalize(dir.path()).join(name));
        // Named for its folder, so that no two folders' files hash alike.
        dir.copy_fixture(
            bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"),
            folder.join(format!("{name}.flac")),
        );
        let release = manager.fakes.seed_discogs_test_release(
            bae_catalogs::test_releases::discogs_test_release(
                &format!("done-{name}"),
                name,
                &[("Track Title", "3:00")],
            ),
        );
        let import_id = format!("import-{name}");
        import
            .send_command(bae_import::test_imports::folder_import(
                &import_id,
                folder.clone(),
                bae_import::test_imports::discogs_release(release),
            ))
            .await
            .unwrap();
        let (release_id, _) = bae_import::test_imports::wait_for_import_complete(
            &mut import.subscribe_import(import_id),
        )
        .await;
        if name == "C" || name == "B" {
            manager.library.manager().upload_release(&release_id).await.unwrap();
        }
        keys.push((name.to_string(), bae_import::CandidateKey::folder(&folder)));
    }
    keys
}

/// The release names one read of a list shows, in order.
fn done_names(window: &bae_import::ImportWindow) -> Vec<String> {
    window
        .rows
        .iter()
        .filter_map(|(item, _key)| match item {
            bae_import::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

/// `limit` items of the list `list` shows from the item at `offset`: the key
/// a jump to the offset reads, and the window from it.
async fn read_list(
    db: &bae_store::Store,
    list: &bae_import::ImportListRequest,
    offset: u64,
    limit: u64,
) -> bae_import::ImportWindow {
    let tables = bae_import::tables::ImportTables::new(db.clone());
    let key =
        tables.import_key_at(list.clone(), offset).await.unwrap().expect("the list has items");
    let window = bae_store::ListWindow { from: Some(key), limit };
    tables.read_import_window(list.clone(), window).await.unwrap()
}

/// Done runs the releases coven still has uploads queued for first, the first
/// to go first, and then the rest in the view's order: whichever order the
/// view asks for, what is still uploading leads.
#[tokio::test]
async fn done_runs_the_releases_still_uploading_first() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let tables = bae_import::tables::ImportTables::new(manager.store.clone());
    let keys = done_with_two_uploading(&test, &temp_dir).await;
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
        let list = bae_import::ImportListRequest::of(done(order));
        assert_eq!(
            done_names(&read_list(&manager.store, &list, 0, 50).await),
            expected,
            "{order:?}"
        );
    }

    let list = bae_import::ImportListRequest::of(done(bae_import::ImportListOrder::NewestFirst));
    let across = read_list(&manager.store, &list, 1, 2).await;
    assert_eq!(done_names(&across), ["B", "D"], "a window across the two parts");
    assert_eq!(
        tables.read_import_list_for_test(list.clone(), Vec::new()).await.unwrap().count.rows,
        4
    );
    // A key leads with the number of the part its row is in: C and B are
    // still uploading, A and D are not.
    let part = |at: usize| across.rows[at].1.values()[0].clone();
    assert_eq!(part(0), bae_store::list_windows::SortValue::Integer(0), "B is in the first part");
    assert_eq!(part(1), bae_store::list_windows::SortValue::Integer(1), "D is in the second");
    assert_eq!(done_names(&read_list(&manager.store, &list, 3, 5).await), ["A"]);
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    assert_eq!(tables.import_candidate_position(list.clone(), &key("A")).await.unwrap(), Some(3));
    let selected = vec![key("A"), key("B")];
    let oldest_first =
        bae_import::ImportListRequest::of(done(bae_import::ImportListOrder::OldestFirst));
    let opening = tables
        .import_list_view_change(
            bae_import::ImportListRequest::of(bae_import::ImportListView::default()),
            oldest_first.clone(),
            selected.clone(),
        )
        .await
        .unwrap()
        .opening;
    assert!(
        matches!(opening, bae_import::ImportListOpening::At { position: 1, .. }),
        "the view opens at B, still uploading, behind C: {opening:?}"
    );
    assert_eq!(
        tables
            .load_import_list_selected(oldest_first, selected, None)
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
fn done_uploads(window: &bae_import::ImportWindow) -> Vec<(String, Option<TriageStateLine>)> {
    window
        .rows
        .iter()
        .filter_map(|(item, _key)| match item {
            bae_import::ImportListItem::Candidate { row, .. } => {
                Some((row.folder_name.clone(), row.state_line.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The list's live read tells a Done row what to show while its release
/// waits for upload, while it uploads and after it finishes.
#[tokio::test]
async fn a_done_row_carries_its_release_upload() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    done_with_two_uploading(&test, &temp_dir).await;
    let import = test.library.start_import().await.unwrap();
    let subscription = import.subscribe_list_window(
        bae_import::ImportListRequest::of(bae_import::ImportListView {
            tab: bae_import::TriageTab::Done,
            order: bae_import::ImportListOrder::PathAscending,
            ..bae_import::ImportListView::default()
        }),
        bae_store::ListWindow::top(50),
    );
    assert_eq!(
        done_uploads(&subscription.next().await.unwrap().result.unwrap()),
        [
            ("C".to_string(), Some(TriageStateLine::QueuedToUpload)),
            ("B".to_string(), Some(TriageStateLine::QueuedToUpload)),
            ("A".to_string(), None),
            ("D".to_string(), None),
        ]
    );

    // Held in flight until its start is read.
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    test.storage
        .hold_next_creation(
            coven::ObjectPrefix::files(manager.store.store_id_for_test()),
            started,
            resumed,
        )
        .await;
    let drain = bae_util::test_spawn::task({
        let manager = test.library.manager().clone();
        async move { manager.drain_uploads_for_test().await }
    });
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while !done_uploads(&subscription.next().await.unwrap().result.unwrap())
            .iter()
            .any(|(_, line)| matches!(line, Some(TriageStateLine::Uploading { .. })))
        {}
    })
    .await
    .expect("a row shows its release uploading");
    created.await.unwrap();
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while done_uploads(&subscription.next().await.unwrap().result.unwrap())
            .iter()
            .any(|(_, upload)| upload.is_some())
        {}
    })
    .await
    .expect("every row's files are stored");
}

/// A run of rows read on Done takes the rows between its ends across the part
/// still uploading and the rest, both ends included.
#[tokio::test]
async fn extending_on_done_selects_the_run_across_its_two_parts() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let tables = bae_import::tables::ImportTables::new(manager.store.clone());
    let keys = done_with_two_uploading(&test, &temp_dir).await;
    let key = |name: &str| keys.iter().find(|(key, _)| key == name).unwrap().1.clone();
    let list = bae_import::ImportListRequest::of(bae_import::ImportListView {
        tab: bae_import::TriageTab::Done,
        order: bae_import::ImportListOrder::NewestFirst,
        ..bae_import::ImportListView::default()
    });
    let run = tables.import_list_keys_between(list, key("A"), key("B")).await.unwrap();
    assert_eq!(
        run.iter()
            .map(|run_key| keys.iter().find(|(_, key)| key == run_key).unwrap().0.clone())
            .collect::<Vec<_>>(),
        ["B", "D", "A"],
        "C, B, D, A from B to A"
    );
}

/// A window is the query's request: moving it reruns the read as a request
/// change.
#[tokio::test]
async fn moving_a_window_reruns_the_list_as_a_request_change() {
    let (test, _temp_dir) = setup_test_manager().await;
    let import = test.library.start_import().await.unwrap();
    let window = import.subscribe_list_window(
        bae_import::ImportListRequest::of(bae_import::ImportListView::default()),
        bae_store::ListWindow::top(0),
    );
    let initial = window.next().await.expect("the initial value");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert!(initial.result.unwrap().rows.is_empty());

    window.set(bae_store::ListWindow::top(50)).expect("the window is requested");
    let requested = window.next().await.expect("the requested value");
    assert_eq!(requested.cause, coven::LiveQueryCause::Request);
}

/// Closing the library ends a list's waiting read before the store closes:
/// the read answers that it was cancelled, never that the store it read is
/// closed. The app quits with the import tab's read waiting like this.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_cancels_a_waiting_read_before_the_store_closes() {
    let (test, _temp_dir) = setup_test_manager().await;
    let import = test.library.start_import().await.unwrap();
    let subscription = Arc::new(import.subscribe_list_window(
        bae_import::ImportListRequest::of(bae_import::ImportListView::default()),
        bae_store::ListWindow::top(50),
    ));
    subscription.next().await.expect("the initial value");
    let waiting = bae_util::test_spawn::task({
        let subscription = subscription.clone();
        async move { subscription.next().await.map(|_| ()) }
    });

    test.library.manager().close().await.unwrap();

    let read = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, waiting)
        .await
        .expect("the waiting read ends")
        .unwrap();
    assert!(read.is_err(), "the waiting read is cancelled");
    assert!(subscription.next().await.is_err(), "a read after the close is cancelled too");
}

/// Closing the library ends a live read's waiting `next` before the store
/// closes: the read answers that it was cancelled, never that the store it
/// read is closed, and takes no more requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_cancels_a_waiting_live_read() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let read = Arc::new(bae_store::live_query::LiveRead::new(
        manager
            .until_closed(manager.subscribe_upcoming_window(false, bae_store::ListWindow::top(10))),
        |_, window| Some(window),
    ));
    read.next().await.expect("the initial value");
    let waiting = bae_util::test_spawn::task({
        let read = read.clone();
        async move { read.next().await }
    });

    test.library.manager().close().await.unwrap();

    let ended = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, waiting)
        .await
        .expect("the waiting read ends")
        .unwrap();
    assert!(matches!(ended, Err(LiveQueryError::Cancelled)), "{ended:?}");
    assert!(matches!(read.set(bae_store::ListWindow::top(5)), Err(LiveQueryError::Cancelled)));
}

/// A file operation that panics fails as something that went wrong, is not
/// reported again (the panic hook reported it), and leaves the library able
/// to close.
#[tokio::test]
async fn a_file_operation_that_panics_fails_and_lets_the_library_close() {
    let (test, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;
    let manager = test.library.manager();

    let error = manager
        .run_file_operation("test operation", |_closing| async { broken_operation() })
        .await
        .expect_err("a panicked operation fails");

    assert!(matches!(error, LibraryError::Internal(_)), "{error}");
    assert!(bugs.reports().is_empty(), "{:?}", bugs.reports());
    manager.close().await.expect("the library closes");
}

fn broken_operation() -> Result<(), LibraryError> {
    panic!("the operation broke")
}

/// Work asked of a closed library starts nothing, and the caller hears so
/// rather than a log line: starting sync after close is refused.
#[tokio::test]
async fn starting_work_after_close_is_refused_to_the_caller() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    manager.close().await.unwrap();
    let error = manager.start_sync_at_startup().await.expect_err("a closed library starts no work");
    assert!(matches!(error, LibraryError::Closed("startup sync attachment")), "got {error:?}");
    assert!(matches!(manager.start(), Err(LibraryError::Closed(_))));
}

#[tokio::test]
async fn close_waits_for_a_file_operations_rollback_and_refuses_new_work() {
    let (test, _home) = setup_test_manager().await;
    let manager = test.library.manager();
    let (entered, reached) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let db = manager.store.clone();
    let (admitted, admission) = tokio::sync::oneshot::channel();
    let working = manager.clone();
    let operation = bae_util::test_spawn::task(async move {
        working
            .run_file_operation("rollback test", move |closing| async move {
                admitted.send(()).unwrap();
                closing.cancelled().await;
                entered.send(()).unwrap();
                released.await.unwrap();
                db.store_name().await?;
                Ok(())
            })
            .await
    });
    admission.await.unwrap();
    let closing = manager.clone();
    let close = bae_util::test_spawn::task(async move { closing.close().await });
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

/// Only the releases with files being sent have a transfer: files that only
/// wait have none. While a file is sent the total has a bar, and
/// once every file is sent the file and its release leave and the bar goes.
#[tokio::test]
async fn the_transfers_name_only_the_files_being_sent() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release = insert_release_with_queued_uploads(
        manager,
        &temp_dir,
        "sending",
        "Album Title",
        &[("a.flac", b"bytes-a"), ("b.flac", b"bytes-b")],
    )
    .await;
    manager.store.set_transfer_limits(coven::TransferLimits {
        uploads: std::num::NonZeroUsize::new(1).unwrap(),
        ..manager.store.transfer_limits()
    });
    let mut transfers = manager.watch_outbox_transfers();
    assert_eq!(
        transfers.next().await.unwrap().unwrap(),
        crate::OutboxTransfers {
            releases: Default::default(),
            total: crate::UploadTransfer { bar: None, throughput_bps: 0 },
        },
        "files that only wait have no transfer"
    );

    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.store.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = bae_util::test_spawn::task({
        let manager = test.library.manager().clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let current = transfers.next().await.unwrap().unwrap();
            if current.total.bar.is_some() {
                break current;
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
            let current = transfers.next().await.unwrap().unwrap();
            if current.releases.is_empty() && current.total.bar.is_none() {
                break;
            }
        }
    })
    .await
    .expect("every file sent, nothing has a transfer");
}

/// A release being sent reads its bytes against its whole size from the
/// moment its send starts: its stored files count as done, and a file sent in
/// one request counts its share as its object goes, before the provider
/// answers. Its file's own share is told by file.
#[tokio::test]
async fn a_file_sent_in_one_request_moves_its_release_as_it_goes() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release = insert_release_with_queued_uploads(
        manager,
        &temp_dir,
        "sending",
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut transfers = manager.watch_outbox_transfers();
    let mut files = manager.watch_upload_file_transfers();
    transfers.next().await.unwrap().unwrap();

    // The memory provider takes a request's bytes as it starts, then holds
    // the answer.
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.store.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = bae_util::test_spawn::task({
        let manager = test.library.manager().clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let bar = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let current = transfers.next().await.unwrap().unwrap();
            let bar = current.releases.get(&release.id).map(|transfer| transfer.bar);
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
) -> (TestManager, TestFolder, Arc<bae_diagnostics::RecordingBugReporter>) {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let (manager, temp_dir) = setup_test_manager_with(
        Arc::new(coven::SystemClock),
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    )
    .await;
    (manager, temp_dir, bugs)
}
