/// Stand in for starting an import: the claim, then the worker's first
/// `ImportProgress`, in that order.
async fn start_import_for(fixture: &Fixture, candidate: &Path) {
    let candidate_key = candidate.to_string_lossy().into_owned();
    fixture
        .import
        .claim_candidate_for_import(&candidate_key, "import-running")
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
    let mut changes = fixture.import.every_runtime_change_for_test();

    fixture
        .import
        .claim_candidate_for_import(&candidate.to_string_lossy(), "import-1")
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
                        step: crate::import::ImportStep::Preparing(
                            crate::import::PrepareStep::Queued
                        ),
                    })
    ));
}

/// An import started on a queued candidate takes it off the queue: it gets no
/// result, and the progress count treats its identification as ended.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_start_takes_a_queued_candidate_out_of_work_and_progress() {
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

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
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
    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
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

/// A re-scan announcing a candidate an import owns must not queue it again,
/// or the batch total would never come back down. Driven from the bus rather
/// than the filesystem so the event order does not depend on the watcher.
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

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    // The scan re-announces the claimed candidate, as a watcher re-scan does.
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
            found_while_automatic: false,
        }));
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after the re-scan")
        .unwrap();

    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
    assert!(
        progress.iter().all(|(_, total)| *total <= 2),
        "the re-scan does not put the importing candidate back in the batch: {progress:?}"
    );
    assert_eq!(progress.last(), Some(&(0, 0)), "{progress:?}");
}

/// An import started while a settled verdict's release is being fetched stops
/// its row. The queue does not cancel a settling answer; the write takes the
/// commit lock the claim was taken under, re-reads the candidate, and finds an
/// import owns it.
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
    // Holding the release fetch the settle makes before its write puts the
    // import start between the settled verdict and its row.
    fixture.provider.hold("/release/mb-mid-write?");

    let pass = fixture.drain_automatic_task();
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

/// Progress events carry both counts: a batch admitted together opens at its
/// full size and ends at `(0, 0)`. A pass with nothing to identify sends none.
#[tokio::test(flavor = "multi_thread")]
async fn progress_carries_both_counts() {
    let fixture = Fixture::new("progress").await;
    let first = fixture.disc_id_candidate("Album One");
    // A differing file gives the second folder its own content hash and row.
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

    let mut counts = fixture.import.every_identification_count_for_test();
    fixture.drain_automatic().await;
    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
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

    let mut counts = fixture.import.every_identification_count_for_test();
    fixture.drain_automatic().await;
    let replanned: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
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

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();

    let mut opened = false;
    loop {
        let count = tokio::time::timeout(Duration::from_secs(10), counts.recv())
            .await
            .expect("identification progress arrives")
            .expect("the count stays open");
        match count {
            (0, 1) => opened = true,
            (0, 0) if opened => {
                assert!(
                    fixture.identified_for(&dir).await.is_some(),
                    "the identification result must be readable before the batch ends"
                );
                break;
            }
            _ => continue,
        }
    }

    pass.await.expect("the drain joins");
}

/// A candidate removed while it is being identified gives up its queue slot
/// and stores nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_removed_mid_flight_does_not_wedge_the_queue() {
    let fixture = Fixture::new("removed-mid-flight").await;
    let (gate, held, entered) = crate::test_gate::closed();
    fixture
        .import
        .register_artwork_analyzer(Arc::new(GatedAnalyzer(held)));
    let dir = fixture.barcode_candidate("Vanishing");
    let hash = fixture.content_hash(&dir);
    fixture.scan(1).await;

    // Start the pass and hold extraction inside OCR, so the candidate is
    // genuinely mid-flight when the folder goes.
    let pass = fixture.drain_automatic_task();
    tokio::task::spawn_blocking(move || entered.recv_timeout(Duration::from_secs(30)))
        .await
        .unwrap()
        .expect("extraction reaches the artwork");
    std::fs::remove_dir_all(&dir).unwrap();
    // What the folder watcher does when a candidate's directory goes: read
    // the root again, which removes the candidate that is no longer there.
    fixture.rescan(&fixture.import, 0).await;
    gate.open();

    tokio::time::timeout(Duration::from_secs(30), pass)
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
    // The queue still answers a later drain.
    fixture.drain_automatic().await;
}

/// A finished candidate leaves nothing behind: its driver deregisters at its
/// verdict, and the queue holds no entry for it.
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

    fixture.drain_automatic().await;

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

/// Shutdown writes nothing, and `save` checks the token right before writing
/// so a cancel during the settle's release fetch leaves no row.
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
    // Hold the disc-ID response so the shutdown lands mid-identification.
    fixture.provider.hold("/discid/");

    let pass = fixture.drain_automatic_task();
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

    // `save` itself writes nothing under a cancelled token.
    let verdict = TerminalVerdict::NotFoundAnywhere { ledger: None };
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let candidate = fixture
        .import
        .answerable_candidate(&dir.to_string_lossy())
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");
    assert!(matches!(
        save(
            &fixture.context(),
            &cancelled,
            IdentifyRunId::for_test(1),
            &candidate,
            &verdict,
            crate::signals::Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: crate::signals::DiscIdSignal::Absent,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            None,
        )
        .await,
        Settled::Abandoned
    ));
    assert!(
        fixture.stored().await.values().all(|row| row.identify.is_none()),
        "cancellation preserves the discovered draft without writing an identification result"
    );
}
