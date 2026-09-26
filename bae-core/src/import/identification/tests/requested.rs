// ── Identification a person asked for ───────────────────────────────────────
//
// Identify, Retry, a lookup choice changed, a source switched off under a run.
// One queue, so a request is an admission over it rather than a scheduler of
// its own: it goes to the front, runs at interactive priority, and settles
// through the same step the automatic admission does.

/// The failing response is a 400 rather than a 5xx so the client's own retry
/// policy stays out of it; what is under test is what identification does with
/// a failure, not how many times the client repeats one.
/// A verdict is where its run ends, and a re-run afterwards is a run of its
/// own: its own id, its own inputs, its own answer. The run it replaces says
/// nothing further — no driver lingers to re-broadcast the terminal state it
/// already reached, so nothing a later watcher hears can be mistaken for the
/// re-run's answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_rerun_after_a_verdict_is_a_run_of_its_own() {
    let fixture = Fixture::new("rerun-is-its-own-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));
    assert!(
        !fixture.import.is_identifying(&key),
        "the failed run ended at its verdict rather than parking on its inbox"
    );
    let failed_run = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged {
                candidate_key, run, ..
            } if candidate_key == key => Some(run),
            _ => None,
        })
        .next_back()
        .expect("the failed run broadcast its states");

    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-retry-2", "rg-retry-2", &[probed, 0]),
        ),
        (
            "/release/mb-retry-2?",
            200,
            release_json("mb-retry-2", "rg-retry-2", &[probed, 0]),
        ),
    ]);
    fixture.identification().rerun_identify(key.clone());
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if matches!(
                fixture.identified_for(&dir).await.map(|row| row.verdict),
                Some(TerminalVerdict::Found { .. })
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the explicit re-run stores its own answer, not the previous run's");
    let runs: Vec<IdentifyRunId> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged {
                candidate_key, run, ..
            } if candidate_key == key => Some(run),
            _ => None,
        })
        .collect();
    assert!(
        !runs.is_empty() && runs.iter().all(|run| *run != failed_run),
        "the run that already answered broadcast nothing further: {runs:?}"
    );
}

/// A run reads the sources this library asks once, at its start. So the run
/// that replaces it is what a switched-off source reaches — the replacement
/// asks what is left, and the source nobody asks any more is not asked again.
/// This is the restart `AppServices::set_metadata_source_enabled` starts for
/// every key `IdentifyServiceHandle::running_keys` names.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_restarted_over_a_shorter_provider_list_asks_only_what_is_left() {
    let fixture = Fixture::new("restart-drops-a-source").await;
    fixture.use_discogs().await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[("mb-drop-1", "rg-drop-1", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-drop-1?",
        200,
        release_json("mb-drop-1", "rg-drop-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000201", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture
        .provider
        .route("/releases/70000201", 200, discogs_release_json("70000201"));
    fixture
        .manager
        .providers()
        .musicbrainz()
        .seed_discogs_url_lookup("70000201", None);
    // Hold MusicBrainz's answer, so the run is genuinely still asking when the
    // source is switched off rather than racing a run that already settled.
    fixture.provider.hold("/release?");
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    let asked_both = await_run_state(&mut events, &key, |_, _| true).await;
    wait_for_request(&fixture.provider, "/database/search", 1).await;

    fixture
        .manager
        .set_metadata_source_enabled(crate::import::Catalog::Discogs, false).await
        .expect("MusicBrainz is still asked, so Discogs can be switched off");
    fixture.identification().rerun_identify(key.clone());
    await_run_state(&mut events, &key, |run, _| run != asked_both).await;
    fixture.provider.release();

    let row = fixture.await_identified_row(&dir).await;
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.source)
            .collect::<Vec<_>>(),
        vec![crate::import::Catalog::MusicBrainz],
        "the run that stored the answer asked only the source still switched on"
    );
    assert_eq!(
        fixture.provider.count_containing("/database/search"),
        1,
        "and Discogs was asked once, by the run that started while it was on: {:?}",
        fixture.provider.requests()
    );
}

/// Explicit Lookup settles a candidate too. A person's own run answers the
/// candidate for good, and "answered" means the next launch opens it with no
/// network — so the same step runs here, before the verdict is written.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_settles_its_lead_before_storing_the_verdict() {
    let fixture = Fixture::new("interactive-settles").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        release_json("mb-interactive-1", "rg-interactive-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    // Exactly what a person asking to identify the candidate does.
    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("the explicit Lookup recorder stores the verdict");

    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert!(
        matches[0].source_tracks.is_some(),
        "the lead was settled before the verdict was written"
    );
    assert!(
        fixture.stored_release("mb-interactive-1").await.is_some(),
        "and its documents are archived under the release they describe"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "so the row is admitted on evidence that was actually checked"
    );

    // A later sweep pass finds nothing left to buy.
    let after_lookup = fixture.provider.requests().len();
    fixture.sweep_once().await;
    assert_eq!(
        fixture.provider.requests().len(),
        after_lookup,
        "a settled row is finished: {:?}",
        fixture.provider.requests()
    );
}

/// A release document can carry a readable tracklist yet still be impossible
/// to project into candidate metadata. An explicit run stores that terminal
/// release-details failure instead of waiting for another state event that
/// will never arrive.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_stores_a_metadata_projection_failure() {
    let fixture = Fixture::new("interactive-projection-failure").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-projection-1", "rg-projection-1", &[probed, 0]),
    );
    let mut incomplete: serde_json::Value = serde_json::from_str(&release_json(
        "mb-projection-1",
        "rg-projection-1",
        &[probed, 0],
    ))
    .unwrap();
    // No artist credits: the tracklist still reads, and the mapper refuses a
    // release with nobody credited. The release group goes too — not because
    // its absence fails anything, but because a document that names one sends
    // the fetch after it, and this run has no group route to answer with.
    let incomplete = incomplete.as_object_mut().unwrap();
    incomplete.remove("artist-credit");
    incomplete.remove("release-group");
    fixture.provider.route(
        "/release/mb-projection-1?",
        200,
        serde_json::Value::Object(incomplete.clone()).to_string(),
    );
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(Duration::from_secs(2), fixture.await_identified_row(&dir))
        .await
        .expect("the explicit recorder stores the projection failure");

    assert!(matches!(
        identify_result(&row).verdict,
        TerminalVerdict::Failed {
            ref failures,
            ..
        } if matches!(failures.as_slice(), [crate::identify::IdentifyFailure::ReleaseDetails(_)])
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn interactive_lookup_runs_while_automatic_lookup_is_off() {
    let fixture = Fixture::new("interactive-with-automatic-off").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-interactive-off", "rg-interactive-off", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-interactive-off?",
        200,
        release_json("mb-interactive-off", "rg-interactive-off", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.manager.set_identify_automatically(false).await.unwrap();

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("interactive lookup stores its verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
}

/// A run a person asked for stores its verdict for a candidate whose draft is
/// already filled, and leaves nothing pending on the key once it has.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_stores_its_verdict_for_a_pre_filled_candidate() {
    let fixture = Fixture::new("explicit-with-pre-filled-draft").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-default-none", "rg-default-none", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-default-none?",
        200,
        release_json("mb-default-none", "rg-default-none", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("an explicit lookup stores its verdict over a pre-filled draft");
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture
            .import
            .candidate_runtimes()
            .get(&key)
            .is_some_and(|runtime| {
                crate::import::triage::TriageRuntimeFacts::of(runtime)
                    .identification
                    .is_some()
            })
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a stored verdict leaves nothing pending on the key");
}

/// Asking to identify an answered candidate runs it again: the person asked
/// for a run, and a stored result is what they are asking to replace. The
/// sweep is the only reader that treats a result as a reason not to run.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_for_an_answered_candidate_runs_it_again() {
    let fixture = Fixture::new("resume-answered").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-asked-again", "rg-asked-again", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-asked-again?",
        200,
        release_json("mb-asked-again", "rg-asked-again", &[probed, 0]),
    );
    fixture.scan(1).await;

    let verdict = multi_match_verdict(&["mb-resume-1", "mb-resume-2"], "rg-resume-1");
    let wrote = fixture
        .import
        .save_candidate_verdict_if_current(
            &dir.to_string_lossy(),
            IdentifyRunId::for_test(1),
            &NewImportCandidateVerdict {
                candidate: crate::import::CandidateAsRead {
                    content_hash: fixture.content_hash(&dir),
                    file_edit_revision: 0,
                    metadata_revision: 0,
                },
                folder_path: dir.to_string_lossy().into_owned(),
                verdict,
                signals: settled_signals(fixture.probed_durations(&dir)),
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert!(wrote, "the seeded verdict lands");

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(Duration::from_secs(20), async {
        while !fixture.identified_for(&dir).await.is_some_and(|result| {
            matches!(
                &result.verdict,
                TerminalVerdict::Found { matches, .. }
                    if matches.iter().any(|m| m.release_id == "mb-asked-again")
            )
        }) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the run the person asked for replaces the stored result");
    assert!(fixture.provider.count_containing("/discid/") > 0);
}

/// Asking again while a run is going supersedes it: the person asked for a
/// run that reads the candidate as it is now, so the one in flight is
/// cancelled before the new one starts and its result can never land.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_during_an_active_run_supersedes_it() {
    let fixture = Fixture::new("explicit-supersedes-active-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let first_run = loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the active run broadcasts its state")
            .expect("the import event bus remains open");
        if let ImportEvent::IdentifyStateChanged {
            candidate_key, run, ..
        } = event
        {
            if candidate_key == key {
                break run;
            }
        }
    };

    // The person presses again while the first run is still at the provider.
    fixture.start_explicit_lookup(&dir);

    let replacement = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ImportEvent::IdentifyStateChanged {
                candidate_key, run, ..
            } = events
                .recv()
                .await
                .expect("the import event bus remains open")
            {
                if candidate_key == key && run != first_run {
                    return run;
                }
            }
        }
    })
    .await;
    fixture.provider.release();

    let replacement = replacement.expect("pressing again starts a run of its own");
    assert_ne!(
        replacement, first_run,
        "the run in flight was superseded rather than joined"
    );
    assert!(
        fixture.import.is_identifying(&key),
        "and the candidate is identifying under the new run"
    );
    // The superseded run reached the provider and was cancelled there; nothing
    // it would have concluded is stored.
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// Re-run on a candidate whose driver is gone starts a fresh interactive run
/// instead of no-op'ing — the stored answer is what a re-run exists to
/// replace, so it is not consulted.
#[tokio::test(flavor = "multi_thread")]
async fn a_rerun_with_no_driver_runs_identification_again() {
    let fixture = Fixture::new("rerun-no-driver").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.archive("mb-rerun-1", "rg-rerun-1", &[probed, 0]).await;
    fixture
        .store_settled_verdict(&dir, "mb-rerun-1", "rg-rerun-1", probed)
        .await;
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-rerun-2?",
        200,
        release_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );

    fixture
        .identification()
        .rerun_identify(dir.to_string_lossy().into_owned());

    wait_for_request(&fixture.provider, "/discid/", 1).await;
}

/// A person's own run is ended by their decision the same way a sweep's is:
/// the run stops at `Idle` and the watcher hanging off it stores nothing.
#[tokio::test(flavor = "multi_thread")]
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
            crate::import::MetadataProvenance::FileMetadata,
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

    let stored = fixture.stored_for(&dir).await.expect("the pick is stored");
    assert_eq!(
        stored.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileMetadata)
    );
    assert!(
        stored.identify.is_none(),
        "the watcher on a cancelled run writes nothing"
    );
}

/// Changing what a candidate's identification asks about supersedes the run
/// that was going: the automatic run ends on `Idle`, a new run answers under a
/// new id, and the queue gives up the slot instead of waiting on a run that is
/// never coming back.
#[tokio::test(flavor = "multi_thread")]
async fn changing_the_choices_supersedes_the_run_and_frees_its_slot() {
    let fixture = Fixture::new("choices-supersede-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
            "mb-super-1",
            "rg-super-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-super-1?",
        200,
        release_json(
            "mb-super-1",
            "rg-super-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();
    let mut restart = fixture.import.subscribe_events();

    let pass = fixture.sweep();
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
                excluded_barcodes: vec!["0123456789012".to_string()],
                chosen_catalogs: Vec::new(),
                search_words: None,
                discounted_catalogs: Vec::new(),
            },
        )
        .await
        .unwrap();
    fixture.identification().rerun_identify(key.clone());
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
        "the queue holds nothing for a candidate whose answer has landed"
    );
}

/// The case the entry's admission exists for, end to end: the queue stores a
/// failed candidate, the person explicitly reruns it, and the automatic
/// admission must not take it back.
///
/// `identify.start` supersedes, so taking it would cancel their Interactive run
/// and restart it in the background. Nothing keeps a second set of keys to
/// remember whose run it is: the entry on the queue says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_the_queue_failed_then_the_user_reran_is_left_alone() {
    let fixture = Fixture::new("failed-then-looked-up").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(SlowAnalyzer {
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
    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));

    // The user explicitly reruns the stored failure.
    fixture.identification().rerun_identify(key.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !fixture.import.is_identifying(&key) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("identify registers the explicit rerun");
    assert!(
        fixture.import.is_identifying(&key),
        "their run is in flight"
    );

    let lookups_before = fixture.provider.count_containing("/discid/");
    fixture.sweep_once().await;
    assert_eq!(
        fixture.identification_status(&key),
        Some(crate::import::IdentificationStatus::Running),
        "the queue holds it as the run the person asked for, not as one of its own"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        lookups_before,
        "nor spent a background lookup on it"
    );
    assert!(
        fixture.import.is_identifying(&key),
        "their run is still the one registered — it was not cancelled and \
         restarted underneath them"
    );
}

/// The guard the priority exists for. A candidate someone is looking up is
/// left alone — `identify.start` supersedes, so taking it would cancel their
/// Interactive run and restart it in the background. Under one queue it is
/// left alone by construction: the key is already an entry, and admitting is
/// idempotent.
///
/// The held lookup is what makes the run in flight rather than merely recent:
/// a driver is registered for exactly as long as its run is working, so the
/// admission has to be timed against a lookup that has not come back yet.
#[tokio::test(flavor = "multi_thread")]
async fn the_automatic_admission_leaves_a_candidate_being_looked_up_alone() {
    let fixture = Fixture::new("user-owns-it").await;
    let dir = fixture.disc_id_candidate("Opened");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-opened-1", "rg-opened-1", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    // The request registers the person's driver before the admission runs.
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    assert!(
        fixture.import.is_identifying(&key),
        "the user's run is in flight"
    );

    fixture.sweep_once().await;

    assert_eq!(
        fixture.identification_status(&key),
        Some(crate::import::IdentificationStatus::Running),
        "the queue holds only the run the person asked for"
    );
    assert!(
        fixture.import.is_identifying(&key),
        "and it did not cancel the run out from under them"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "nor asked the provider a second time for the same candidate"
    );
    fixture.provider.release();
}
