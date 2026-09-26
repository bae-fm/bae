#[tokio::test(flavor = "multi_thread")]
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
    fixture.manager.set_identify_automatically(false).await.unwrap();

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
                    crate::import::MetadataProvenance::FileMetadata,
                )
                .await
                .unwrap();
        }
        let stored = fixture
            .stored_for(&dir)
            .await
            .expect("the candidate is stored");
        assert_eq!(
            stored.metadata_provenance,
            Some(crate::import::MetadataProvenance::FileMetadata),
            "{name} starts from its file tags"
        );
        assert_eq!(
            stored.metadata_author,
            if reset_by_hand {
                crate::import::MetadataAuthor::Person
            } else {
                crate::import::MetadataAuthor::Prefill
            },
            "{name}: the draft says who read the tags into it"
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
async fn a_pick_stores_the_result_and_the_sweep_leaves_it_alone() {
    let fixture = Fixture::new("pick-is-a-result").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    // Nothing is routed: the pick reads the stored release, and a sweep
    // that decided to run this candidate would have to look the disc ID up.
    fixture
        .archive("mb-chosen", "rg-chosen", &[probed, 0])
        .await;
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture
        .import
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    "mb-chosen".to_string(),
                ),
                partners: Vec::new(),
            },
        )
        .await
        .expect("the pick lands");

    let picked = fixture.stored_for(&dir).await.expect("the pick is stored");
    let result = picked
        .identify
        .expect("the choice is the candidate's result");
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
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-chosen".to_string()
            ),
            partners: Vec::new(),
        })
    );
}

/// A result for the files a candidate has right now is the whole reason not to
/// run it again: a second pass over the same queue asks nothing.
#[tokio::test(flavor = "multi_thread")]
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
        } else {
            fixture
                .import
                .select_candidate_metadata_provenance(
                    key,
                    crate::import::MetadataProvenance::FileMetadata,
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

/// Files that changed retire the result they were read from, so the queue asks
/// again for the candidate as it now is.
#[tokio::test(flavor = "multi_thread")]
async fn changed_files_retire_the_result_and_identification_runs_again() {
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

    let answered = fixture.content_hash(&dir);

    std::fs::write(dir.join("notes.txt"), "the folder changed").unwrap();
    fixture.import.scan_watched_folders().unwrap();

    // The scan writes the changed folder with no result for the files it now
    // has, and the queue hears the same scan and answers it. Read for the
    // answer rather than for the gap between them: the queue closes that gap
    // on its own, which is the whole of what it is for.
    let reshaped = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let hash = fixture.content_hash(&dir);
            if hash != answered && fixture.identified_for(&dir).await.is_some() {
                return hash;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the queue answered the candidate its changed files made of it");
    assert_ne!(
        reshaped, answered,
        "the answer is keyed on the files the candidate has now"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn disabling_automatic_lookup_lets_what_it_queued_finish() {
    let fixture = Fixture::new("disable-lets-queued-finish").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-finishes", "rg-finishes", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-finishes?",
        200,
        release_json("mb-finishes", "rg-finishes", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let pass = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    // A preference is not a cancel: the run the setting admitted is still
    // running after it turns off, and answers.
    fixture.manager.set_identify_automatically(false).await.unwrap();
    assert!(fixture.import.is_identifying(&key));
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("the queued run finishes after the setting turns off")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the run the setting admitted stores its answer"
    );
    assert_eq!(fixture.identification_status(&key), None);
}

#[tokio::test(flavor = "multi_thread")]
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

    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.sweep_once().await;

    let after = fixture
        .stored_for(&dir)
        .await
        .expect("disabling background work retains settled identification");
    assert_eq!(after, before);
}

#[tokio::test(flavor = "multi_thread")]
async fn enabling_automatic_lookup_schedules_unresolved_candidates() {
    let fixture = Fixture::new("enable-schedules-unresolved").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    // Started while automatic identification is off, so it admits nothing
    // until the setting turns on.
    fixture.identification();
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

    fixture.manager.set_identify_automatically(true).await.unwrap();

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("enabling automatic Lookup stores a verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
}
