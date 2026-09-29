// ── 9. A decision ends the candidate's identification ───────────────────────
//
// A decision about a candidate — a pick, a clear, a skip, an import — ends its
// run inside the decision's own write and takes it off the queue.

/// A pick names one candidate, so it ends one candidate's run. The pass keeps
/// answering everything else it had going.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_ends_only_the_picked_candidates_run() {
    let fixture = Fixture::new("pick-ends-one-run").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let picked = fixture.disc_id_candidate("Picked");
    let other = fixture.barcode_candidate("Other");
    let probed = fixture.probed_total_ms(&other);
    fixture.provider.route("/discid/", 200, "{}");
    fixture
        .provider
        .route("/release?", 200, search_json("mb-other-1", "rg-other-1"));
    fixture.provider.route(
        "/release/mb-other-1?",
        200,
        release_json("mb-other-1", "rg-other-1", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let picked_key = picked.to_string_lossy().into_owned();
    let other_key = other.to_string_lossy().into_owned();
    let pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let mut events = fixture.import.every_event();

    fixture
        .import
        .select_candidate_file_tags(picked_key.clone())
        .await
        .expect("the pick lands");

    assert!(
        !fixture.import.is_identifying(&picked_key),
        "the pick ends the picked candidate's run before it returns"
    );
    // The other candidate is carried to its verdict. The picked one left the
    // queue with the run the pick ended.
    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.await_identified_row(&other),
    )
    .await
    .expect("the pass answers the candidate nobody picked");

    let picked_row = fixture
        .stored_for(&picked)
        .await
        .expect("the pick is stored");
    assert_eq!(
        picked_row.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileMetadata)
    );
    assert!(
        picked_row.identify.is_none(),
        "a cancelled run writes no verdict"
    );
    assert!(
        fixture.identified_for(&other).await.is_some(),
        "the candidate nobody picked reached its verdict"
    );
    let cancelled_other = drain_events(&mut events).into_iter().any(|event| {
        matches!(
            event,
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state: IdentifyState::Idle,
                ..
            } if candidate_key == other_key
        )
    });
    assert!(
        !cancelled_other,
        "the pick must not tear down the run of a candidate it does not name"
    );

    fixture.identification().shut_down();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the pass ends")
        .unwrap();
}

/// Skipping is a decision about the candidate, so it ends its run. Unskipping
/// brings it back as it was — unqueued — and a person asking answers it.
#[tokio::test(flavor = "multi_thread")]
async fn skipping_a_candidate_ends_its_run_and_unskipping_queues_nothing() {
    let fixture = Fixture::new("skip-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(1).await;

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .import
        .set_candidate_skipped(key.clone(), true)
        .await
        .expect("the candidate is skipped");

    assert!(
        !fixture.import.is_identifying(&key),
        "skipping ends the run before it returns"
    );
    fixture.provider.release();
    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "a cancelled run writes no verdict"
    );

    fixture
        .import
        .set_candidate_skipped(key.clone(), false)
        .await
        .expect("the candidate is unskipped");
    fixture.drain_automatic().await;
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "unskipping queues nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
}

/// Clearing a candidate's metadata is a decision too: the run answering the
/// candidate as it was ends, and the change is announced so the pane reads
/// the candidate afresh.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_a_candidates_metadata_ends_its_run_and_announces_the_change() {
    let fixture = Fixture::new("clear-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(1).await;

    let mut events = fixture.import.every_event();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .import
        .clear_candidate_metadata(key.clone())
        .await
        .expect("the metadata is cleared");

    assert!(
        !fixture.import.is_identifying(&key),
        "clearing ends the run before it returns"
    );
    // The announce is sent inside the same write, so it is on the bus by the
    // time the command has returned.
    let announced = drain_events(&mut events).into_iter().any(|event| {
        matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key })
                if candidate_key == key
        )
    });
    assert!(announced, "the clear announces the candidate's change");
    fixture.provider.release();

    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "a cancelled run writes no verdict"
    );
}

/// A clear ends the identification the automatic admission had going, and
/// the candidate leaves the queue with it: nothing puts it back on its own,
/// and a person asking answers it.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_a_candidates_metadata_takes_it_off_the_queue() {
    let fixture = Fixture::new("clear-requeues").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cleared", "rg-cleared", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cleared?",
        200,
        release_json("mb-cleared", "rg-cleared", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .import
        .clear_candidate_metadata(key.clone())
        .await
        .expect("the metadata is cleared");

    assert!(
        !fixture.import.is_identifying(&key),
        "the clear ends the run it found"
    );
    tokio::time::timeout(Duration::from_secs(30), pass)
        .await
        .expect("the candidate left the queue, so the automatic admission holds nothing")
        .unwrap();
    fixture.provider.release();
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "nothing asked about the cleared candidate again: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
}

/// A decision already announced when a person asks is heard before the ask:
/// the clear's announcement takes nothing off the queue the request then puts
/// on it, and the run asked for stores its answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_announced_before_a_request_leaves_the_requested_run_going() {
    let fixture = Fixture::new("decision-then-request").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-asked", "rg-asked", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-asked?",
        200,
        release_json("mb-asked", "rg-asked", &[probed, 0]),
    );
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(1).await;

    // The loop is handed both before it first looks: the announcement, then
    // the request, with every later event behind them.
    let (events, mut bus) = tokio::sync::mpsc::unbounded_channel();
    events
        .send(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged {
            candidate_key: key.clone(),
        }))
        .unwrap();
    let mut later = fixture.import.every_event();
    tokio::spawn(async move {
        while let Some(event) = later.recv().await {
            if events.send(event).is_err() {
                return;
            }
        }
    });
    let (commands, mut asked) = tokio::sync::mpsc::unbounded_channel();
    commands
        .send(Command::Request {
            candidate_key: key.clone(),
        })
        .unwrap();
    let (drained, answered) = tokio::sync::oneshot::channel();
    commands
        .send(Command::AwaitDrained {
            drain: Drain::Every,
            drained,
        })
        .unwrap();

    let context = fixture.context();
    let token = CancellationToken::new();
    let loop_token = token.clone();
    let queue = tokio::spawn(async move {
        let config = context.library_manager.subscribe_config_changes();
        super::queue::run(&context, &loop_token, &mut bus, &mut asked, &config).await;
    });
    tokio::time::timeout(Duration::from_secs(30), answered)
        .await
        .expect("the requested job ends")
        .expect("the queue says so");

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the run asked for after the clear was announced stored its answer; requests: {:?}",
        fixture.provider.requests()
    );
    token.cancel();
    queue.await.unwrap();
}

/// An import claims the candidate, so nothing is left for identification to
/// answer. The run ends at the command that claimed it, and the import goes on.
#[tokio::test(flavor = "multi_thread")]
async fn starting_an_import_ends_the_candidates_run() {
    let fixture = Fixture::new("import-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(1).await;

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    let import_id = fixture
        .import
        .start_import(&key)
        .await
        .expect("the prepared candidate enters its import");

    assert!(
        !fixture.import.is_identifying(&key),
        "starting the import ends the run before it returns"
    );
    assert!(!import_id.is_empty(), "the import was queued");
    fixture.provider.release();
    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "a cancelled run writes no verdict"
    );
}

/// Switching automatic identification off takes nothing off the queue: a
/// candidate whose answer is already being written keeps its write, so the
/// row lands and nothing is left saying a commit is still pending.
#[tokio::test(flavor = "multi_thread")]
async fn switching_automatic_identification_off_lets_a_settling_write_land() {
    let fixture = Fixture::new("disable-during-settle").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-settling", "rg-settling", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-settling?",
        200,
        release_json("mb-settling", "rg-settling", &[probed, 0]),
    );
    fixture.provider.hold("/release/mb-settling?");
    fixture.scan(1).await;

    let mut pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/release/mb-settling?", 1).await;

    fixture.manager.set_identify_automatically(false).await.unwrap();

    assert!(
        tokio::time::timeout(Duration::from_secs(1), &mut pass)
            .await
            .is_err(),
        "the pass waits for the write it already has in flight"
    );
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("the pass ends once the write it waited for has landed")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the answer being written when automatic identification went off still landed"
    );
    let runtime = fixture.import.candidate_runtime(&key);
    assert!(
        runtime
            .as_ref()
            .is_none_or(|runtime| runtime.saving.is_none()),
        "nothing is left saying the commit is still pending: {runtime:?}"
    );
    assert!(
        runtime
            .as_ref()
            .is_none_or(|runtime| runtime.save_failed.is_none()),
        "and the write that landed reports no failure: {runtime:?}"
    );
}
