/// A run reads what the person decided it asks about. With the barcode left
/// out, no provider is asked about the codes the artwork carries — the disc ID
/// is asked about and answers alone.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_run_leaves_out_the_signals_the_candidate_says_to_leave_out() {
    let fixture = Fixture::new("choices-leave-out-barcode").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    // A rip log for the disc ID and an image for the barcode, so both signals
    // are there and only the choice decides which is asked about.
    let dir = fixture.disc_id_candidate("Album");
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-choice-1", "rg-choice-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-choice-1?",
        200,
        release_json("mb-choice-1", "rg-choice-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.scan(1).await;
    fixture.use_discogs();
    fixture
        .import
        .set_candidate_lookup_choices(
            &dir.to_string_lossy(),
            crate::import::LookupChoices {
                disc_id_excluded: false,
                barcode_excluded: true,
                chosen_catalogs: Vec::new(),
            },
        )
        .await
        .unwrap();

    fixture.sweep_once().await;

    fixture
        .await_identified_row(&dir)
        .await
        .identify
        .expect("the run still answers from the disc ID");
    let requests = fixture.provider.requests();
    assert!(
        requests.iter().any(|target| target.contains("/discid/")),
        "the disc ID is still asked about: {requests:?}"
    );
    assert!(
        !requests.iter().any(|target| target.contains("barcode")),
        "nothing asks about a barcode the candidate says to leave out: {requests:?}"
    );
    assert!(
        !requests.iter().any(|target| target.contains("/database/search")),
        "and Discogs is not asked about it either: {requests:?}"
    );
}

/// Changing what a candidate's identification asks about supersedes the run
/// that was going: the sweep's run ends on `Idle`, a new run answers under a
/// new id, and the sweep's pass gives up the slot instead of waiting on a run
/// that is never coming back.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn changing_the_choices_supersedes_the_run_and_frees_the_sweep_s_slot() {
    let fixture = Fixture::new("choices-supersede-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-super-1", "rg-super-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-super-1?",
        200,
        release_json("mb-super-1", "rg-super-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();
    let mut restart = fixture.import.subscribe_events();

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let sweeps_run = await_run_state(&mut restart, &key, |_, _| true).await;

    // What a person changing a choice does: the whole value is stored, and the
    // run that reads it is started.
    fixture
        .import
        .set_candidate_lookup_choices(
            &key,
            crate::import::LookupChoices {
                disc_id_excluded: false,
                barcode_excluded: true,
                chosen_catalogs: Vec::new(),
            },
        )
        .await
        .unwrap();
    fixture.sweep.rerun_for_explicit_lookup(key.clone());
    await_run_state(&mut restart, &key, |run, _| run != sweeps_run).await;
    fixture.provider.release();

    // The pass returns rather than waiting forever on the run it lost.
    tokio::time::timeout(Duration::from_secs(30), pass)
        .await
        .expect("the pass frees the slot its superseded run held")
        .unwrap();

    fixture.await_identified_row(&dir).await;
    let runs: Vec<(IdentifyRunId, IdentifyState)> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                run,
                state,
                ..
            } if candidate_key == key => Some((run, state)),
            _ => None,
        })
        .collect();
    let superseded = sweeps_run;
    assert!(
        runs.iter()
            .any(|(run, state)| *run == superseded && matches!(state, IdentifyState::Idle)),
        "the superseded run ends on Idle: {runs:?}"
    );
    assert!(
        runs.iter()
            .any(|(run, state)| *run != superseded && state.is_terminal()),
        "a new run answers under a new id: {runs:?}"
    );
    assert_eq!(
        fixture
            .import
            .candidate_runtimes()
            .remove(&key)
            .as_ref()
            .and_then(|runtime| crate::import::TriageRuntimeFacts::of(runtime).identification),
        None,
        "the sweep does not queue the candidate again for a run it gave away"
    );
}
