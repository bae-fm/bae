/// What starting an import does to a candidate, in the order the import
/// service does it: [`ImportServiceHandle::claim_candidate_for_import`] before
/// the command is queued, and the worker's first `ImportProgress` after it
/// dequeues the command.
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
            progress: crate::import::ImportProgress::Preparing {
                import_id: "import-running".to_string(),
                step: crate::import::PrepareStep::ValidatingSourceFiles,
                album_title: String::new(),
                artist_name: String::new(),
            },
        });
}

#[tokio::test(flavor = "multi_thread")]
async fn claiming_an_import_publishes_queued_status_immediately() {
    let fixture = Fixture::new("import-queued-status").await;
    let candidate = fixture.disc_id_candidate("Album Title");
    fixture.scan(1).await;
    let mut changes = fixture.import.subscribe_candidate_runtime().1;

    fixture
        .import
        .claim_candidate_for_import(&candidate.to_string_lossy())
        .await;

    let change = tokio::time::timeout(Duration::from_secs(1), changes.recv())
        .await
        .expect("the queued status is published")
        .expect("runtime changes remain open");
    assert!(matches!(
        change,
        crate::import::CandidateRuntimeChange::Updated { key, runtime }
            if key == candidate.to_string_lossy()
                && runtime.import
                    == Some(crate::import::ImportInFlight {
                        progress_percent: None,
                        step: Some(crate::import::ImportStep::Preparing(
                            crate::import::PrepareStep::Queued
                        )),
                    })
    ));
}

/// An import started mid-pass takes its candidate away from the sweep: its
/// draft gains no identification result, and it stops counting towards the
/// queue's total.
#[tokio::test(flavor = "multi_thread")]
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
    let pass = fixture.sweep();
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

    let importing_state = fixture
        .stored()
        .await
        .remove(&importing_hash)
        .expect("the discovered draft remains available to the import");
    assert!(importing_state.identify.is_none());
    assert!(fixture.identified_for(&remaining).await.is_some());
    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentificationProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert!(
        progress.contains(&(1, 2)),
        "the candidate the import took is no longer waited on: {progress:?}"
    );
    assert_eq!(
        progress.last(),
        Some(&(0, 0)),
        "and the batch is over once the other one has its answer: {progress:?}"
    );
}

/// A re-scan lands while an import owns a candidate. The scan announces every
/// candidate it walks, import or no import, and the pass must not queue one
/// again that an import has taken away — the batch's total would climb past
/// the identifications it holds and never come down, because nothing ends an
/// identification that never started.
///
/// This is the same sequence CI hits on every non-macOS runner: the OS watcher
/// delivers the folder's own change events late enough that the re-scan they
/// trigger arrives inside the pass rather than after it. Driven here from the
/// bus instead of the filesystem, so the ordering is the test's and not the
/// watcher backend's.
#[tokio::test(flavor = "multi_thread")]
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
    let pass = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    // …and then the scan re-announces it, exactly as a watcher-triggered pass
    // over the same folder does.
    let claimed = match fixture
        .import
        .get_candidate(&importing.to_string_lossy())
        .await
    {
        Ok(Some(ImportCandidateSnapshot::Folder { candidate, .. })) => candidate,
        other => panic!(
            "the claimed candidate is still a folder candidate: {:?}",
            other.map(|snapshot| snapshot.map(|_| "a candidate"))
        ),
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
            ImportEvent::IdentificationProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert!(
        progress.iter().all(|(_, total)| *total <= 2),
        "the re-scan does not put the importing candidate back in the batch: {progress:?}"
    );
    assert_eq!(progress.last(), Some(&(0, 0)), "{progress:?}");
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
async fn an_import_started_while_a_verdict_is_in_flight_stores_nothing() {
    let fixture = Fixture::new("import-mid-write").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
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

    let pass = fixture.sweep();
    wait_for_request(&fixture.provider, "/release/mb-mid-write?", 1).await;
    start_import_for(&fixture, &dir).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("pass finishes after the import claims the candidate")
        .unwrap();

    let state = fixture
        .stored()
        .await
        .remove(&hash)
        .expect("the discovered draft remains available to the import");
    assert!(
        state.identify.is_none(),
        "the in-flight verdict does not replace the importing candidate's draft"
    );
}

/// Progress crosses as an event carrying both numbers, so a view renders
/// "n of m" without counting the rows it happens to be holding. The batch is
/// what the pass identifies: it opens at the whole of it, and ends at `(0, 0)`
/// — which is what a surface draws nothing for — once every identification is
/// over. A pass with nothing to identify says nothing at all.
#[tokio::test(flavor = "multi_thread")]
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
        if let ImportEvent::IdentificationProgress { identified, total } = event {
            progress.push((identified, total));
        }
    }
    assert_eq!(
        progress.first(),
        Some(&(0, 2)),
        "planning announces the whole batch before any of it is answered"
    );
    assert!(
        progress.contains(&(1, 2)),
        "and every verdict advances the count: {progress:?}"
    );
    assert_eq!(
        progress.last(),
        Some(&(0, 0)),
        "the batch is over once both have their answers: {progress:?}"
    );

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let replanned: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentificationProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert!(
        replanned.is_empty(),
        "a pass over an answered queue identifies nothing, so there is no batch: {replanned:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
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
    let pass = fixture.sweep();

    let mut opened = false;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("identification progress arrives")
            .expect("event bus remains open");
        match event {
            ImportEvent::IdentificationProgress {
                identified: 0,
                total: 1,
            } => opened = true,
            ImportEvent::IdentificationProgress {
                identified: 0,
                total: 0,
            } if opened => {
                assert!(
                    fixture.identified_for(&dir).await.is_some(),
                    "the identification result must be readable before the batch ends"
                );
                break;
            }
            _ => continue,
        }
    }

    pass.await.expect("sweep pass joins");
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
async fn a_candidate_removed_mid_flight_does_not_wedge_the_sweep() {
    let fixture = Fixture::new("removed-mid-flight").await;
    let analyzer_started = Arc::new(Barrier::new(2));
    let analyzer_release = Arc::new(Barrier::new(2));
    fixture.import.register_artwork_analyzer(Arc::new(GatedAnalyzer {
        started: analyzer_started.clone(),
        release: analyzer_release.clone(),
    }));
    let dir = fixture.barcode_candidate("Vanishing");
    let hash = fixture.content_hash(&dir);
    fixture.scan(1).await;

    // Start the pass and hold extraction inside OCR, so the candidate is
    // genuinely mid-flight when the folder goes.
    let pass = fixture.sweep();
    tokio::task::spawn_blocking(move || {
        analyzer_started.wait();
    })
    .await
    .unwrap();
    let mut events = fixture.import.subscribe_events();
    std::fs::remove_dir_all(&dir).unwrap();
    // What the folder watcher does when a candidate's directory goes: re-scan
    // the root and reconcile, which emits `CandidateRemoved` for the one that
    // is no longer there.
    fixture.import.scan_watched_folders().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if matches!(
                events.recv().await,
                Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }))
                    if candidate_key == dir.to_string_lossy()
            ) {
                break;
            }
        }
    })
    .await
    .expect("the rescan reports the removed candidate");
    tokio::task::spawn_blocking(move || {
        analyzer_release.wait();
    })
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the pass must finish rather than wait on a candidate that is gone")
        .unwrap();

    let state = fixture
        .stored()
        .await
        .remove(&hash)
        .expect("the discovered draft remains keyed by the candidate's content");
    assert!(
        state.identify.is_none(),
        "a candidate that vanished mid-identification learned nothing"
    );
    // And the sweep is still alive to the queue: a later pass runs.
    fixture.sweep_once().await;
}

/// A candidate the queue is done with leaves nothing of the queue's behind.
///
/// The driver ends at its own verdict, so nothing has to cancel it, and the
/// queue gives the key up in the same breath. A driver left registered past its
/// answer would park a task, a bus-relay task, and a live broadcast receiver
/// that every later `IdentifyStateChanged` — a whole `IdentifyState`, result
/// vectors and all — is deep-cloned into; over a queue swept unattended on
/// every launch that fan-out is quadratic in its size.
#[tokio::test(flavor = "multi_thread")]
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
        fixture.identified_for(&dir).await.is_some(),
        "the candidate really was identified"
    );
    assert!(
        !fixture.import.is_identifying(&key),
        "and its driver is gone: the run ended at the verdict it reached"
    );
    assert_eq!(
        fixture.identification_status(&key),
        None,
        "and the queue holds nothing for a candidate it has finished with"
    );
}

/// Teardown writes nothing. The token is re-checked immediately before the
/// write, so a cancellation landing during the settle lookup that precedes it
/// cannot leave a row behind.
#[tokio::test(flavor = "multi_thread")]
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

    let pass = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    fixture.identification().shut_down();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("a cancelled queue returns")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "a cancelled candidate writes no identification result: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );

    // `save` itself asks for no write under a cancelled token, whatever
    // reached it.
    let verdict = TerminalVerdict::NotFoundAnywhere { ledger: None };
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        save(
            &fixture.context(),
            &cancelled,
            "/x",
            IdentifyRunId::for_test(1),
            crate::import::CandidateAsRead {
                content_hash: "hash-x".to_string(),
                file_edit_revision: 0,
                metadata_revision: 0,
            },
            "/x",
            &verdict,
            crate::signals::Signals {
                rip: crate::signals::RipEvidence::Unproven,
                disc_id: crate::signals::DiscIdSignal::Absent { track_count: 0 },
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                durations: crate::import::probe::SourceDurations::default(),
            },
            None,
            false,
        )
        .await,
        Settled::Abandoned
    ));
    let stored = fixture.stored().await;
    assert!(
        stored.values().all(|row| row.identify.is_none()),
        "cancellation preserves the discovered draft without writing an identification result"
    );
    assert!(
        !stored.contains_key("hash-x"),
        "the already-cancelled write creates no candidate state"
    );
}
