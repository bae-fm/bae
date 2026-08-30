#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn automatic_lookup_off_runs_none_of_the_identification_pipeline() {
    let fixture = Fixture::new("automatic-off").await;
    let calls = Arc::new(AtomicUsize::new(0));
    fixture
        .extraction
        .register_analyzer(Arc::new(CountingAnalyzer {
            calls: Arc::clone(&calls),
        }));
    let dir = fixture.barcode_candidate("Candidate");
    fixture.scan(1).await;
    fixture
        .manager
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::SearchManually)
        .unwrap();

    fixture.sweep_once().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn captured_non_online_sources_run_none_of_the_identification_pipeline() {
    use crate::config::DefaultImportMetadataSource;

    let cases = [
        ("file-tags-default", DefaultImportMetadataSource::FileTags),
        ("none-default", DefaultImportMetadataSource::None),
    ];

    for (name, default_mode) in cases {
        let fixture = Fixture::new(name).await;
        let calls = Arc::new(AtomicUsize::new(0));
        fixture
            .extraction
            .register_analyzer(Arc::new(CountingAnalyzer {
                calls: Arc::clone(&calls),
            }));
        let dir = fixture.barcode_candidate("Candidate");
        fixture
            .manager
            .set_default_import_metadata_source(default_mode)
            .unwrap();
        fixture.scan(1).await;

        fixture.sweep_once().await;

        assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR ran for {name}");
        assert!(
            fixture.provider.requests().is_empty(),
            "provider request ran for {name}"
        );
        assert!(
            fixture.identified_for(&dir).await.is_none(),
            "identification produced an answer for {name}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_seeded_candidate_runs_none_of_the_automatic_identification_pipeline() {
    let fixture = Fixture::new("seeded-candidate").await;
    let calls = Arc::new(AtomicUsize::new(0));
    fixture
        .extraction
        .register_analyzer(Arc::new(CountingAnalyzer {
            calls: Arc::clone(&calls),
        }));
    let dir = fixture.barcode_candidate("Candidate");
    fixture.scan(1).await;
    fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();

    fixture.sweep_once().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    let stored = fixture.stored_for(&dir).await.expect("the seed remains stored");
    assert_eq!(stored.metadata_provenance, Some(crate::import::MetadataProvenance::FileTags));
    assert!(stored.identify.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn choosing_a_metadata_provenance_cancels_background_identification() {
    let fixture = Fixture::new("seed-cancels-background").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let context = fixture.context();
    let token = CancellationToken::new();
    let mut pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture
        .import
        .select_candidate_metadata_provenance(key.clone(), crate::import::MetadataProvenance::FileTags)
        .await
        .unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(2), &mut pass)
        .await
        .is_ok();
    fixture.provider.release();
    if !stopped {
        pass.await.unwrap();
    }

    assert!(stopped, "the seed must stop the background pass");
    assert!(!fixture.identify.is_running(&key));
    let stored = fixture.stored_for(&dir).await.expect("the seed remains stored");
    assert_eq!(stored.metadata_provenance, Some(crate::import::MetadataProvenance::FileTags));
    assert!(stored.identify.is_none());
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
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::SearchManually)
        .unwrap();

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("interactive lookup stores its verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
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
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::SearchManually)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("disabling automatic lookup stops the pass")
        .unwrap();
    fixture.provider.release();

    assert!(!fixture.identify.is_running(&key));
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
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::SearchManually)
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
async fn changing_the_default_does_not_change_an_existing_candidates_captured_source() {
    let fixture = Fixture::new("change-default-keeps-background").await;
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
        .set_default_import_metadata_source(crate::config::DefaultImportMetadataSource::FileTags)
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the existing Find Online candidate completes")
        .unwrap();

    assert!(!fixture.identify.is_running(&key));
    assert!(fixture.context.ours.lock().unwrap().is_empty());
    assert!(fixture.identified_for(&dir).await.is_some());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn enabling_automatic_lookup_schedules_unresolved_candidates() {
    let fixture = Fixture::new("enable-schedules-unresolved").await;
    fixture
        .manager
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::SearchManually)
        .unwrap();
    let sweep = start(
        fixture.import.clone(),
        fixture.identify.clone(),
        fixture.extraction.clone(),
        fixture.manager.clone(),
    );
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
        .set_default_find_online_mode(crate::config::DefaultFindOnlineMode::Automatic)
        .unwrap();

    tokio::time::timeout(Duration::from_secs(20), fixture.await_identified_row(&dir))
        .await
        .expect("enabling automatic Lookup stores a verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
    sweep.stop();
}
