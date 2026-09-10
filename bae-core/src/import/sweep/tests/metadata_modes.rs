#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn automatic_lookup_off_runs_none_of_the_identification_pipeline() {
    let fixture = Fixture::new("automatic-off").await;
    let calls = Arc::new(AtomicUsize::new(0));
    fixture
        .import
        .register_artwork_analyzer(Arc::new(CountingAnalyzer {
            calls: Arc::clone(&calls),
        }));
    let dir = fixture.barcode_candidate("Candidate");
    fixture.scan(1).await;
    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();

    fixture.sweep_once().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// A draft read off the folder's own files is a starting point, not an
/// answer: the sweep runs the candidate whether the pre-fill wrote that draft
/// at discovery or a person asked for it afterwards.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_file_tags_draft_is_still_run() {
    for (name, reset_by_hand) in [("prefilled", false), ("reset-to-tags", true)] {
        let fixture = Fixture::new(name).await;
        let dir = fixture.disc_id_candidate("Candidate");
        let probed = fixture.probed_total_ms(&dir);
        fixture.provider.route(
            "/discid/",
            200,
            discid_json("mb-file-tags", "rg-file-tags", &[probed, 0]),
        );
        fixture.provider.route(
            "/release/mb-file-tags?",
            200,
            release_json("mb-file-tags", "rg-file-tags", &[probed, 0]),
        );
        fixture.scan(1).await;
        if reset_by_hand {
            fixture
                .import
                .select_candidate_metadata_provenance(
                    dir.to_string_lossy().into_owned(),
                    crate::import::MetadataProvenance::FileTags,
                )
                .await
                .unwrap();
        }
        assert_eq!(
            fixture
                .stored_for(&dir)
                .await
                .expect("the candidate is stored")
                .metadata_provenance,
            Some(crate::import::MetadataProvenance::FileTags),
            "{name} starts from its file tags"
        );

        fixture.sweep_once().await;

        assert!(
            fixture.identified_for(&dir).await.is_some(),
            "the sweep ran {name} and stored its result"
        );
    }
}

/// A release a person chose answers the candidate, so it is stored as the
/// result for the files it has right now — and the sweep, which reads results
/// and nothing about who reached them, leaves it alone.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_stores_the_result_and_the_sweep_leaves_it_alone() {
    let fixture = Fixture::new("pick-is-a-result").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    // Nothing is routed: the pick reads the archived document, and a sweep
    // that decided to run this candidate would have to look the disc ID up.
    fixture.archive("mb-chosen", "rg-chosen", &[probed, 0]).await;
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture
        .import
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-chosen".to_string(),
                partners: Vec::new(),
            },
        )
        .await
        .expect("the pick lands");

    let picked = fixture
        .stored_for(&dir)
        .await
        .expect("the pick is stored");
    let result = picked.identify.expect("the choice is the candidate's result");
    assert!(
        matches!(
            &result.verdict,
            crate::identify::TerminalVerdict::Found { matches, .. }
                if matches.len() == 1 && matches[0].release_id == "mb-chosen"
        ),
        "the result names the release they chose: {:?}",
        result.verdict
    );

    // Everything the pick itself fetched, before the pass: what the sweep adds
    // to this is what it asked about a candidate already answered.
    let asked = fixture.provider.requests();

    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests(),
        asked,
        "the sweep asked about a candidate a person had answered"
    );
    assert_eq!(
        fixture
            .stored_for(&dir)
            .await
            .expect("the candidate is still stored")
            .metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-chosen".to_string(),
            partners: Vec::new(),
        })
    );
}

/// A result for the files a candidate has right now is the whole reason not to
/// run it again: a second pass over the same queue asks nothing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_with_a_result_for_its_files_is_not_run_again() {
    let fixture = Fixture::new("result-stops-the-sweep").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-settled-once", "rg-settled-once", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-settled-once?",
        200,
        release_json("mb-settled-once", "rg-settled-once", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.sweep_once().await;
    let first = fixture
        .stored_for(&dir)
        .await
        .expect("the first pass stores a result");
    assert!(first.identify.is_some());
    let asked = fixture.provider.count_containing("/discid/");

    fixture.sweep_once().await;

    assert_eq!(fixture.provider.count_containing("/discid/"), asked);
    assert_eq!(fixture.stored_for(&dir).await, Some(first));
}

/// Clearing the draft, and reading the folder's tags back into it, say
/// nothing about what identification concluded: the result stands, the file
/// revision it is keyed on does not move, and no new run starts.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_draft_write_leaves_the_result_and_starts_no_run() {
    for name in ["clear-metadata", "reset-to-tags"] {
        let fixture = Fixture::new(name).await;
        let dir = fixture.disc_id_candidate("Candidate");
        let key = dir.to_string_lossy().into_owned();
        let probed = fixture.probed_total_ms(&dir);
        fixture.provider.route(
            "/discid/",
            200,
            discid_json("mb-draft-write", "rg-draft-write", &[probed, 0]),
        );
        fixture.provider.route(
            "/release/mb-draft-write?",
            200,
            release_json("mb-draft-write", "rg-draft-write", &[probed, 0]),
        );
        fixture.scan(1).await;
        fixture.sweep_once().await;
        let settled = fixture
            .stored_for(&dir)
            .await
            .expect("the sweep stores a result");
        let asked = fixture.provider.count_containing("/discid/");

        if name == "clear-metadata" {
            fixture.import.clear_candidate_metadata(key).await.unwrap();
        }
        else {
            fixture
                .import
                .select_candidate_metadata_provenance(
                    key,
                    crate::import::MetadataProvenance::FileTags,
                )
                .await
                .unwrap();
        }

        let after = fixture
            .stored_for(&dir)
            .await
            .expect("the candidate is still stored");
        assert_eq!(
            after.identify, settled.identify,
            "{name} left the result alone"
        );
        assert_eq!(
            after.file_edits.revision, settled.file_edits.revision,
            "{name} did not move the file revision the result is keyed on"
        );

        fixture.sweep_once().await;

        assert_eq!(
            fixture.provider.count_containing("/discid/"),
            asked,
            "{name} started no run"
        );
    }
}

/// Files that changed retire the result they were read from, so the sweep
/// asks again for the candidate as it now is.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn changed_files_retire_the_result_and_the_sweep_runs_again() {
    let fixture = Fixture::new("changed-files-run-again").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-changed", "rg-changed", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-changed?",
        200,
        release_json("mb-changed", "rg-changed", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.sweep_once().await;
    assert!(fixture.identified_for(&dir).await.is_some());

    std::fs::write(dir.join("notes.txt"), "the folder changed").unwrap();
    fixture.import.scan_watched_folders().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.stored_for(&dir).await.is_none_or(|row| row.identify.is_some()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the changed folder is stored with no result for the files it now has");

    fixture.sweep_once().await;

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the sweep answered the candidate its changed files made of it"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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
    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("interactive lookup stores its verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
}

/// A run a person asked for stores its verdict for a candidate whose draft is
/// already filled, and leaves nothing pending on the key once it has.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn disabling_automatic_lookup_cancels_running_background_identification() {
    let fixture = Fixture::new("disable-running-background").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("disabling automatic lookup stops the pass")
        .unwrap();
    fixture.provider.release();

    assert!(!fixture.import.is_identifying(&key));
    assert!(fixture.context.ours.lock().unwrap().is_empty());
    assert!(fixture.identified_for(&dir).await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn disabling_automatic_lookup_preserves_a_settled_result() {
    let fixture = Fixture::new("disable-preserves-settled").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-settled", "rg-settled", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-settled?",
        200,
        release_json("mb-settled", "rg-settled", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.sweep_once().await;
    let before = fixture
        .stored_for(&dir)
        .await
        .expect("identification stores its settled result");

    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();
    fixture.sweep_once().await;

    let after = fixture
        .stored_for(&dir)
        .await
        .expect("disabling background work retains settled identification");
    assert_eq!(after, before);
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn enabling_automatic_lookup_schedules_unresolved_candidates() {
    let fixture = Fixture::new("enable-schedules-unresolved").await;
    fixture
        .manager
        .set_identify_automatically(false)
        .unwrap();
    let sweep = start(fixture.import.clone(), fixture.manager.clone());
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-enabled", "rg-enabled", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-enabled?",
        200,
        release_json("mb-enabled", "rg-enabled", &[probed, 0]),
    );
    fixture.scan(1).await;
    assert!(fixture.provider.requests().is_empty());

    fixture
        .manager
        .set_identify_automatically(true)
        .unwrap();

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("enabling automatic Lookup stores a verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
    sweep.stop();
}
