/// A candidate that appears after the pass has started and already holds a
/// verdict joins it as answered: the pass counts it and finishes, and nothing
/// re-buys the answer it has.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_late_candidate_with_a_stored_verdict_joins_the_pass_answered() {
    let fixture = Fixture::new("answered-late-row").await;
    let running = fixture.disc_id_candidate("Running");
    let late = fixture.disc_id_candidate("Late");
    std::fs::write(late.join("late-playlist.m3u"), "late identity").unwrap();
    fixture.scan(2).await;
    let late_key = late.to_string_lossy().into_owned();
    fixture
        .import
        .set_candidate_skipped(late_key.clone(), true)
        .await
        .unwrap();

    let probed = fixture.probed_total_ms(&running);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-malformed-running?",
        200,
        release_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    assert!(
        fixture
            .manager
            .save_import_candidate_verdict(&NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&late),
                folder_path: late.to_string_lossy().into_owned(),
                verdict: TerminalVerdict::NotFoundAnywhere,
                signals: settled_signals(Default::default()),
                expected_edit_revision: 0,
                expected_metadata_revision: 0,
                metadata: blank_metadata_for_dir(&late),
            })
            .await
            .unwrap()
    );
    fixture
        .import
        .set_candidate_skipped(late_key, false)
        .await
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the pass finishes with the late candidate counted")
        .expect("the late candidate is handled without panic");
    assert!(
        !fixture
            .identify
            .is_running(running.to_string_lossy().as_ref())
    );
    let late_row = fixture
        .stored_for(&late)
        .await
        .expect("the late candidate keeps its row");
    assert_eq!(
        late_row.identify.map(|identify| identify.verdict),
        Some(TerminalVerdict::NotFoundAnywhere),
        "an answered candidate is not identified again"
    );
}
