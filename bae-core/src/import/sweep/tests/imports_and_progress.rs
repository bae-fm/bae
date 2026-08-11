/// What starting an import does to a candidate, in the order the import
/// service does it: [`ImportServiceHandle::claim_candidate_for_import`] before
/// the command is queued, and the worker's first `ImportProgress` some time
/// after that — after the folder re-walk, and behind every import already
/// queued ahead of it. Tests that model the start as the event alone are
/// modelling the second half of it only.
async fn start_import_for(fixture: &Fixture, candidate: &Path) {
    let candidate_key = candidate.to_string_lossy().into_owned();
    fixture
        .import
        .claim_candidate_for_import(&candidate_key)
        .await;
    fixture
        .import
        .emit_event_for_test(ImportEvent::ImportProgress {
            candidate_key,
            progress: crate::import::ImportProgress::Started {
                id: "release-importing".to_string(),
                import_id: "import-running".to_string(),
            },
        });
}

/// An import started mid-pass takes its candidate away from the sweep: no
/// verdict is stored for it, and it stops counting towards the queue's total.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_import_start_mid_pass_removes_the_candidate_from_work_and_progress() {
    let fixture = Fixture::new("import-mid-pass").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let importing_hash = fixture.content_hash(&importing);
    let probed = fixture.probed_total_ms(&remaining);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-progress?",
        200,
        release_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    // Starting an import, in the order the import service really does it: the
    // candidate is claimed before the command is queued, and the worker's
    // first progress event comes back some time after that.
    start_import_for(&fixture, &importing).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after import ownership changes")
        .unwrap();

    assert!(!fixture.stored().await.contains_key(&importing_hash));
    assert!(fixture.stored_for(&remaining).await.is_some());
    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert_eq!(progress.last(), Some(&(1, 1)), "{progress:?}");
}

/// A re-scan lands while an import owns a candidate. The scan announces every
/// candidate it walks, import or no import, and the pass must not count one
/// back in that an import has taken away — the queue's total would climb back
/// past what the sweep is responsible for and never come down, because nothing
/// announces the candidate again once the import finishes with it.
///
/// This is the same sequence CI hits on every non-macOS runner: the OS watcher
/// delivers the folder's own change events late enough that the re-scan they
/// trigger arrives inside the pass rather than after it. Driven here from the
/// bus instead of the filesystem, so the ordering is the test's and not the
/// watcher backend's.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_rescan_does_not_count_back_a_candidate_an_import_owns() {
    let fixture = Fixture::new("import-rescan").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&remaining);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-rescan?",
        200,
        release_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    // …and then the scan re-announces it, exactly as a watcher-triggered pass
    // over the same folder does.
    let claimed = match fixture.import.get_candidate(&importing.to_string_lossy()) {
        Some(ImportCandidateSnapshot::Folder { candidate, .. }) => candidate,
        other => panic!("the claimed candidate is still a folder candidate: {other:?}"),
    };
    fixture
        .import
        .emit_event_for_test(ImportEvent::Scan(ScanEvent::FolderCandidate {
            candidate: claimed,
            skipped: false,
            is_added: false,
        }));
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after the re-scan")
        .unwrap();

    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert_eq!(progress.last(), Some(&(1, 1)), "{progress:?}");
}

/// The same import start, one step later in the candidate's life — and the
/// step where the pass's own bookkeeping can no longer help.
///
/// The verdict has settled and the pass is buying its tracklist, so the
/// candidate is in neither `in_flight` nor `pending`: the `ImportProgress` the
/// worker sends finds nothing to detach and cancels nothing, and the write is
/// already on its way. What stops the row is the claim — the write takes the
/// folder-state commit lock the claim was taken under, re-reads the candidate,
/// and finds an import owns it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_import_started_while_a_verdict_is_in_flight_stores_nothing() {
    let fixture = Fixture::new("import-mid-write").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let hash = fixture.content_hash(&dir);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-mid-write", "rg-mid-write"),
    );
    fixture.provider.route(
        "/release/mb-mid-write?",
        200,
        release_json("mb-mid-write", "rg-mid-write", &[1, 1]),
    );
    fixture.scan(1).await;
    // A search result carries no tracklist, so the pass buys one before it can
    // store anything. Holding that lookup puts the import start exactly inside
    // the window between a settled verdict and its row.
    fixture.provider.hold("/release/mb-mid-write?");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/release/mb-mid-write?", 1).await;
    start_import_for(&fixture, &dir).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("pass finishes after the import claims the candidate")
        .unwrap();

    assert!(
        !fixture.stored().await.contains_key(&hash),
        "the verdict was already bought and paid for, and is still not stored: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );
}

/// Progress crosses as an event carrying both numbers. The total is the sweep's
/// own count of what it is responsible for, so a view renders "n of m" without
/// counting the rows it happens to be holding — and the second pass opens at the
/// full count rather than starting over at zero.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn progress_carries_both_counts() {
    let fixture = Fixture::new("progress").await;
    let first = fixture.disc_id_candidate("Album One");
    // A second folder with a differing file makes a second content hash; the
    // two would otherwise share one row.
    let second = fixture.disc_id_candidate("Album Two");
    std::fs::write(second.join("notes.txt"), "different bytes").unwrap();
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-prog-1?",
        200,
        release_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let mut progress = Vec::new();
    for event in drain_events(&mut events) {
        if let ImportEvent::QueueIdentifyProgress { identified, total } = event {
            progress.push((identified, total));
        }
    }
    assert_eq!(
        progress.first(),
        Some(&(0, 2)),
        "planning announces the whole queue before any of it is answered"
    );
    assert_eq!(
        progress.last(),
        Some(&(2, 2)),
        "and every verdict advances the count: {progress:?}"
    );

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let replanned = loop {
        match events.try_recv() {
            Ok(ImportEvent::QueueIdentifyProgress { identified, total }) => {
                break (identified, total)
            }
            Ok(_) => continue,
            Err(e) => panic!("the second pass must announce progress too: {e}"),
        }
    };
    assert_eq!(
        replanned,
        (2, 2),
        "a pass over an answered queue opens at the full count"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn identified_progress_is_emitted_after_the_verdict_is_committed() {
    let fixture = Fixture::new("progress-after-commit").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-progress-commit?",
        200,
        release_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });

    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("identified progress arrives")
            .expect("event bus remains open");
        if matches!(
            event,
            ImportEvent::QueueIdentifyProgress {
                identified: 1,
                total: 1
            }
        ) {
            assert!(
                fixture.stored_for(&dir).await.is_some(),
                "the DB row must be readable before progress exposes the verdict"
            );
            break;
        }
    }

    pass.await.expect("sweep pass joins");
}

/// The sweep drives the whole queue through `IdentifyStateChanged` and
/// `SignalsUpdated`, and every one of those would otherwise invalidate a
/// candidate in both UIs. A run a person started still does.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn only_a_watched_run_invalidates_the_ui() {
    let fixture = Fixture::new("ui-invalidation").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-ui-1", "rg-ui-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-ui-1?",
        200,
        release_json("mb-ui-1", "rg-ui-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let (swept_candidate_events, swept_watched) = count_candidate_events(&mut events);
    assert!(
        swept_candidate_events > 0,
        "the sweep does emit per-candidate events — otherwise this proves nothing"
    );
    assert_eq!(
        swept_watched, 0,
        "but none of them claims a person is watching, so the UI bus renders nothing"
    );

    // The same pipeline, opened by a person.
    let mut events = fixture.import.subscribe_events();
    fixture.select(&dir);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, selected_watched) = count_candidate_events(&mut events);
    assert!(
        selected_watched > 0,
        "an opened candidate keeps re-rendering exactly as it does today"
    );
}

/// `(per-candidate events, of which watched)` — the two the UI bus branches on.
fn count_candidate_events(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
) -> (usize, usize) {
    let mut total = 0;
    let mut watched = 0;
    for event in drain_events(events) {
        let priority = match event {
            ImportEvent::IdentifyStateChanged { priority, .. }
            | ImportEvent::SignalsUpdated { priority, .. } => priority,
            _ => continue,
        };
        total += 1;
        if priority == CallPriority::Interactive {
            watched += 1;
        }
    }
    (total, watched)
}

fn drain_events(events: &mut tokio::sync::broadcast::Receiver<ImportEvent>) -> Vec<ImportEvent> {
    let mut drained = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => drained.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return drained,
            Err(error) => panic!("import event bus failed while draining ready events: {error}"),
        }
    }
}

/// A candidate that vanishes while it is being identified must not wedge the
/// pass. The signals service cancels extraction on `CandidateRemoved` and
/// nothing cancels identify, so the driver would sit in `Triangulating`
/// forever holding a slot — and because the outer loop only takes another
/// `ScanEvent::Finished` between passes, a stalled pass silently ends sweeping
/// for the whole session.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_removed_mid_flight_does_not_wedge_the_sweep() {
    let fixture = Fixture::new("removed-mid-flight").await;
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(400),
    }));
    let dir = fixture.barcode_candidate("Vanishing");
    let hash = fixture.content_hash(&dir);
    fixture.scan(1).await;

    // Start the pass and let extraction get as far as its (slow) OCR pass, so
    // the candidate is genuinely mid-flight when the folder goes.
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    std::fs::remove_dir_all(&dir).unwrap();
    // What the folder watcher does when a candidate's directory goes: re-scan
    // the root and reconcile, which emits `CandidateRemoved` for the one that
    // is no longer there.
    fixture.import.scan_watched_folders().unwrap();

    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the pass must finish rather than wait on a candidate that is gone")
        .unwrap();

    assert!(
        !fixture.stored().await.contains_key(&hash),
        "a candidate that vanished mid-identification learned nothing"
    );
    // And the sweep is still alive to the queue: a later pass runs.
    fixture.sweep_once().await;
}

/// A candidate the sweep is done with leaves nothing of the sweep's behind.
///
/// `run_driver` only ends via `Cancelled`, so a settled driver the sweep does
/// not cancel parks a task, a bus-relay task, and a live broadcast receiver that
/// every later `IdentifyStateChanged` — a whole `IdentifyState`, result vectors
/// and all — is deep-cloned into. Over a queue swept unattended on every launch
/// that fan-out is quadratic in its size. The sweep never toggles a signal or
/// re-runs, so it has no use for the driver once the verdict is written.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_finished_candidate_leaves_no_driver_behind() {
    let fixture = Fixture::new("no-driver-left").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-drv-1?",
        200,
        release_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture.sweep_once().await;

    assert!(
        fixture.stored_for(&dir).await.is_some(),
        "the candidate really was identified"
    );
    assert!(
        !fixture.identify.is_running(&key),
        "and its driver is gone rather than parked for a toggle the sweep will never send"
    );
    assert!(
        fixture.context().ours.lock().unwrap().is_empty(),
        "the sweep holds no ownership of a candidate it has finished with"
    );
}

/// The case the ownership guard exists for, end to end: the sweep fails a
/// candidate, the user then opens it, and the next pass must not take it back.
///
/// `identify.start` supersedes, so taking it would cancel their Interactive run
/// and restart it in the background. This only holds because the sweep gives up
/// ownership when it finishes with a candidate — a set that only ever grows
/// would claim this one forever.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_the_sweep_failed_then_the_user_opened_is_left_alone() {
    let fixture = Fixture::new("failed-then-opened").await;
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(2_000),
    }));
    let dir = fixture.disc_id_candidate("Album");
    // One image, so the user's run stays in flight on a slow OCR pass while the
    // second sweep pass runs.
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture.sweep_once().await;
    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "the first pass learned nothing"
    );

    // The user opens it.
    fixture.select(&dir);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(fixture.identify.is_running(&key), "their run is in flight");

    let lookups_before = fixture.provider.count_containing("/discid/");
    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let (candidate_events, watched) = count_candidate_events(&mut events);

    assert_eq!(
        candidate_events - watched,
        0,
        "the sweep started no background run for a candidate the user has open"
    );
    assert!(
        fixture.context().ours.lock().unwrap().is_empty(),
        "and claimed no ownership of it: it did not take the candidate back"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        lookups_before,
        "nor spent a background lookup on it"
    );
    assert!(
        fixture.identify.is_running(&key),
        "their run is still the one registered — it was not cancelled and \
         restarted underneath them"
    );
}

/// The guard the priority exists for. A candidate someone has open is left
/// alone — `identify.start` supersedes, so taking it would cancel their
/// Interactive run and restart it in the background.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_sweep_leaves_a_candidate_the_user_has_open_alone() {
    let fixture = Fixture::new("user-owns-it").await;
    // A slow OCR pass keeps the user's run in flight across the sweep.
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(1_500),
    }));
    let dir = fixture.barcode_candidate("Opened");
    fixture.scan(1).await;

    fixture.select(&dir);
    // Let identify register the user's driver before the sweep plans.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        fixture.identify.is_running(&dir.to_string_lossy()),
        "the user's run is in flight"
    );

    fixture.sweep_once().await;

    assert!(
        !fixture
            .context()
            .ours
            .lock()
            .unwrap()
            .contains(dir.to_string_lossy().as_ref()),
        "the sweep never took ownership of a candidate it does not own"
    );
    assert!(
        fixture.identify.is_running(&dir.to_string_lossy()),
        "and it did not cancel the run out from under them"
    );
}

/// Teardown writes nothing. The token is re-checked immediately before the
/// write, so a cancellation landing during the settle lookup that precedes it
/// cannot leave a row behind.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_cancelled_candidate_writes_no_row() {
    let fixture = Fixture::new("cancelled-writes-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cancel-1?",
        200,
        release_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    // Hold the disc-ID response, so the cancel lands while the candidate is
    // genuinely mid-identification rather than racing a pass that already
    // finished.
    fixture.provider.hold("/discid/");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass_token = token.clone();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &pass_token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    token.cancel();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("a cancelled pass returns")
        .unwrap();

    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "a cancelled candidate writes no row: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );

    // `save` itself refuses under a cancelled token, whatever reached it.
    let verdict = TerminalVerdict::NotFoundAnywhere;
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(
        !save(
            &fixture.context(),
            &cancelled,
            "/x",
            "hash-x",
            "/x",
            &verdict,
            0,
            0,
        )
        .await,
        "the write is gated on the token, not only the lookup before it"
    );
    assert!(fixture.stored().await.is_empty());
}
