/// A run reads what the person decided it asks about. With the barcode left
/// out, no provider is asked about the codes the artwork carries — the disc ID
/// is asked about and answers alone.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_run_leaves_out_the_signals_the_candidate_says_to_leave_out() {
    let fixture = Fixture::new("choices-leave-out-barcode").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
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
        discid_json(
            "mb-choice-1",
            "rg-choice-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-choice-1?",
        200,
        release_json(
            "mb-choice-1",
            "rg-choice-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;
    fixture.use_discogs();
    fixture
        .import
        .set_candidate_lookup_choices(
            &dir.to_string_lossy(),
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
        !requests
            .iter()
            .any(|target| target.contains("/database/search")),
        "and Discogs is not asked about it either: {requests:?}"
    );
}
