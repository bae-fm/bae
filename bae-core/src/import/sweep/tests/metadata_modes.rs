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
        .set_automatic_import_metadata_lookup(false)
        .unwrap();

    fixture.sweep_once().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    assert!(fixture.stored_for(&dir).await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_file_tags_default_runs_none_of_the_identification_pipeline() {
    let fixture = Fixture::new("file-tags-default").await;
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
        .set_default_import_metadata_mode(crate::config::DefaultImportMetadataMode::FileTags)
        .unwrap();

    fixture.sweep_once().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(
        fixture.provider.requests().is_empty(),
        "no provider request may run"
    );
    assert!(fixture.stored_for(&dir).await.is_none());
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
        .manager
        .save_candidate_metadata_seed(
            &fixture.content_hash(&dir),
            &dir.to_string_lossy(),
            &crate::import::MetadataSeed::FileTags,
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
    assert_eq!(stored.metadata_seed, Some(crate::import::MetadataSeed::FileTags));
    assert!(stored.identify.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn choosing_a_metadata_seed_cancels_background_identification() {
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
        .select_candidate_metadata_seed(key.clone(), crate::import::MetadataSeed::FileTags)
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
    assert_eq!(stored.metadata_seed, Some(crate::import::MetadataSeed::FileTags));
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
        .set_automatic_import_metadata_lookup(false)
        .unwrap();

    fixture.select(&dir);

    tokio::time::timeout(Duration::from_secs(20), fixture.await_row(&dir))
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
        .set_automatic_import_metadata_lookup(false)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("disabling automatic lookup stops the pass")
        .unwrap();
    fixture.provider.release();

    assert!(!fixture.identify.is_running(&key));
    assert!(fixture.context.ours.lock().unwrap().is_empty());
    assert!(fixture.stored_for(&dir).await.is_none());
}
