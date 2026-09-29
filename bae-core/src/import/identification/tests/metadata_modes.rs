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
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// A draft read off the folder's own files is not an answer, whether the
/// pre-fill wrote it or a person asked for it.
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
                .select_candidate_file_tags(dir.to_string_lossy().into_owned())
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

        if reset_by_hand {
            // The reset is a person's decision, which ends the found release's
            // identification; asking again runs it over the reset draft.
            fixture.drain_automatic().await;
            assert!(fixture.identified_for(&dir).await.is_none());
            fixture.start_explicit_lookup(&dir);
            fixture.await_identified_row(&dir).await;
        } else {
            fixture.drain_automatic().await;
        }

        assert!(
            fixture.identified_for(&dir).await.is_some(),
            "{name} was identified and stored its result"
        );
    }
}

/// A release a person chose is stored as the candidate's result, so the found
/// release is not identified on its own afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_stores_the_result_and_automatic_identification_leaves_it_alone() {
    let fixture = Fixture::new("pick-is-a-result").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    // Nothing is routed: a run would have to look the disc ID up.
    fixture
        .archive("mb-chosen", "rg-chosen", &[probed, 0])
        .await;
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture
        .import
        .select_candidate_release(key.clone(), crate::import::PressingLink {
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
            crate::identify::TerminalVerdict::Found { findings: crate::identify::Findings { matches, .. }, .. }
                if matches.len() == 1 && matches[0].release_id == "mb-chosen"
        ),
        "the result names the release they chose: {:?}",
        result.verdict
    );

    let asked = fixture.provider.requests();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.requests(),
        asked,
        "a candidate a person had answered was asked about"
    );
    assert_eq!(
        fixture
            .stored_for(&dir)
            .await
            .expect("the candidate is still stored")
            .release_link,
        Some(crate::import::ReleaseLink::Pressing(crate::import::PressingLink {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-chosen".to_string()
            ),
            partners: Vec::new(),
        }))
    );
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
        fixture.drain_automatic().await;
        let settled = fixture
            .stored_for(&dir)
            .await
            .expect("identification stores a result");
        let asked = fixture.provider.count_containing("/discid/");

        if name == "clear-metadata" {
            fixture.import.clear_candidate_metadata(key).await.unwrap();
        } else {
            fixture
                .import
                .select_candidate_file_tags(key)
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

        fixture.drain_automatic().await;

        assert_eq!(
            fixture.provider.count_containing("/discid/"),
            asked,
            "{name} started no run"
        );
    }
}

/// Files that changed retire the result they were read from. The candidate is
/// the one the library already found, so nothing identifies it again on its
/// own: it waits, unanswered, for a person to ask.
#[tokio::test(flavor = "multi_thread")]
async fn changed_files_retire_the_result_and_wait_for_a_person() {
    let fixture = Fixture::new("changed-files-wait").await;
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
    fixture.drain_automatic().await;
    assert!(fixture.identified_for(&dir).await.is_some());

    let asked = fixture.provider.count_containing("/discid/");

    std::fs::write(dir.join("notes.txt"), "the folder changed").unwrap();
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "the files it has now have no result"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        asked,
        "nothing asked about the changed candidate on its own: {:?}",
        fixture.provider.requests()
    );

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
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

    let pass = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    // A preference is not a cancel: the run the setting admitted is still
    // running after it turns off, and answers.
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
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
    fixture.drain_automatic().await;
    let before = fixture
        .stored_for(&dir)
        .await
        .expect("identification stores its settled result");

    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.drain_automatic().await;

    let after = fixture
        .stored_for(&dir)
        .await
        .expect("disabling background work retains settled identification");
    assert_eq!(after, before);
}
