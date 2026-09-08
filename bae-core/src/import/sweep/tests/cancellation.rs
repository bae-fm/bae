// ── 9. A decision ends the candidate's identification ───────────────────────
//
// The command that decides a candidate — a pick, a clear, a skip, an import —
// cancels its run inside its own write, so no answer the run would have
// reached can land after the decision. The sweep drops the decided candidate
// from the pass it is running and carries every other candidate on to its
// verdict.

/// A pick names one candidate, so it ends one candidate's run. The pass keeps
/// answering everything else it had going.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let mut events = fixture.import.subscribe_events();

    fixture
        .import
        .select_candidate_metadata_provenance(
            picked_key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .expect("the pick lands");

    assert!(
        !fixture.import.is_identifying(&picked_key),
        "the pick ends the picked candidate's run before it returns"
    );
    tokio::time::timeout(Duration::from_secs(30), pass)
        .await
        .expect("the pass answers the candidate nobody picked")
        .unwrap();
    fixture.provider.release();

    let picked_row = fixture
        .stored_for(&picked)
        .await
        .expect("the pick is stored");
    assert_eq!(
        picked_row.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileTags)
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
}

/// Skipping is a decision about the candidate, so it ends its run. Unskipping
/// makes it the sweep's again, and the next pass answers it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn skipping_a_candidate_ends_its_run_and_unskipping_plans_it_again() {
    let fixture = Fixture::new("skip-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
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
        .set_candidate_skipped(key, false)
        .await
        .expect("the candidate is unskipped");
    fixture.sweep_once().await;

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "an unskipped candidate is planned again"
    );
}

/// Clearing a candidate's metadata is a decision too: the run answering the
/// candidate as it was ends, and the change is announced so the pane and the
/// queue sweep both read the candidate afresh.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn clearing_a_candidates_metadata_ends_its_run_and_announces_the_change() {
    let fixture = Fixture::new("clear-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
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

/// A clear leaves the candidate holding neither a pick nor a verdict, so the
/// pass it was running in takes it back: the run the clear ended is replaced,
/// in the same pass, by one reading the candidate as it now is.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn clearing_a_candidates_metadata_mid_pass_puts_it_back_in_the_queue() {
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

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
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
    // A second disc-ID lookup is the pass asking the candidate again. Nothing
    // else can produce one: the pass owns the only queue, and the candidate it
    // dropped would otherwise wait for a scan that never comes.
    wait_for_request(&fixture.provider, "/discid/", 2).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(30), pass)
        .await
        .expect("the pass answers the candidate it took back")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the run that replaced the cleared one stored its verdict"
    );
}

/// An import claims the candidate, so nothing is left for identification to
/// answer. The run ends at the command that claimed it, and the import goes on.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn starting_an_import_ends_the_candidates_run() {
    let fixture = Fixture::new("import-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    let import_id = fixture
        .import
        .start_import(&key, crate::import::StorageMode::Local, false)
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

/// Switching automatic identification off cancels the runs the sweep has
/// going, and then waits: a candidate whose answer is already being written
/// keeps its write, so the row lands and nothing is left saying a commit is
/// still pending.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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

    let context = fixture.context();
    let token = CancellationToken::new();
    let mut pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/release/mb-settling?", 1).await;

    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();

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

/// A person's own run is ended by their decision the same way a sweep's is:
/// the run stops at `Idle` and the watcher hanging off it stores nothing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_during_an_explicit_lookup_stores_no_verdict() {
    let fixture = Fixture::new("pick-ends-explicit-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .import
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .expect("the pick lands");

    assert!(
        !fixture.import.is_identifying(&key),
        "the pick ends the person's own run before it returns"
    );
    await_run_state(&mut events, &key, |_, state| {
        matches!(state, IdentifyState::Idle)
    })
    .await;
    fixture.provider.release();

    let stored = fixture
        .stored_for(&dir)
        .await
        .expect("the pick is stored");
    assert_eq!(
        stored.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileTags)
    );
    assert!(
        stored.identify.is_none(),
        "the watcher on a cancelled run writes nothing"
    );
}
