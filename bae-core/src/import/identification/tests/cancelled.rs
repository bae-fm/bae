// ── A person cancelling identification ──────────────────────────────────────
//
// Cancelling takes candidates off the queue whatever they are doing — waiting,
// running, or having their answer written — and leaves them as they were
// before identification reached them: no verdict, no failure. The automatic
// admission does not put them back; a person asking for one again does.

/// Wait until nothing is identifying `key`: its queue mark, its run and its
/// answer are all gone.
async fn await_not_identifying(fixture: &Fixture, key: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.identification_status(key).is_some() || fixture.import.is_identifying(key)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key} is still identifying after being cancelled"));
}

/// Folders over the cap, each asking the provider a distinct question, with
/// the lookups held so the cap's worth run and the rest wait.
async fn flooded_queue(fixture: &Fixture) -> Vec<PathBuf> {
    fixture
        .import
        .register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let mut dirs = Vec::new();
    for index in 0..MAX_IN_FLIGHT + 1 {
        let dir = fixture.barcode_candidate(&format!("Album {index}"));
        std::fs::write(
            dir.join(format!("playlist-{index}.m3u")),
            format!("{index}"),
        )
        .unwrap();
        dirs.push(dir);
    }
    fixture.provider.route("/release?", 200, "{}");
    fixture.provider.hold("/release?");
    fixture.scan(MAX_IN_FLIGHT + 1).await;
    dirs
}

/// A waiting candidate that is cancelled never reaches the provider, and the
/// candidates around it are answered as before.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_waiting_candidate_is_never_looked_up() {
    let fixture = Fixture::new("cancel-waiting").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT).await;
    let waiting = dirs
        .iter()
        .find(|dir| {
            fixture.identification_status(&dir.to_string_lossy())
                == Some(crate::import::IdentificationStatus::Queued)
        })
        .expect("one candidate is over the cap and waiting")
        .clone();
    let waiting_key = waiting.to_string_lossy().into_owned();

    fixture.identification().cancel(vec![waiting_key.clone()]);
    await_not_identifying(&fixture, &waiting_key).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(30), sweep)
        .await
        .expect("the pass ends")
        .unwrap();

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "the cancelled candidate took no slot: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&waiting).await.is_none());
    for dir in dirs.iter().filter(|dir| **dir != waiting) {
        assert!(
            fixture.identified_for(dir).await.is_some(),
            "{} is answered as it would have been",
            dir.display()
        );
    }
}

/// A running identification that is cancelled writes nothing — no verdict, no
/// failure — stays off the queue through the next automatic pass, and is
/// identified again when a person asks.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_run_leaves_the_candidate_unidentified_until_asked_again() {
    let fixture = Fixture::new("cancel-running").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    fixture.identification().cancel(vec![key.clone()]);
    tokio::time::timeout(Duration::from_secs(10), sweep)
        .await
        .expect("cancelling the only job drains the pass")
        .unwrap();
    await_not_identifying(&fixture, &key).await;
    fixture.provider.release();

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate keeps its state row");
    assert!(row.identify.is_none(), "a cancelled run stores no verdict");
    assert!(
        fixture.import.candidate_runtime(&key).is_none(),
        "nothing is left running or failed for it"
    );

    fixture.sweep_once().await;
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "the automatic admission does not take a cancelled candidate back up"
    );
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture.identification().rerun_identify(key.clone());
    tokio::time::timeout(Duration::from_secs(15), fixture.await_identified_row(&dir))
        .await
        .expect("a person asking identifies it again");
}

/// Cancelling everything empties the queue — what was running and what was
/// waiting — and nothing of it is answered afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_everything_empties_the_queue() {
    let fixture = Fixture::new("cancel-all").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT).await;

    fixture.identification().cancel_all();
    tokio::time::timeout(Duration::from_secs(10), sweep)
        .await
        .expect("cancelling everything drains the pass")
        .unwrap();
    for dir in &dirs {
        await_not_identifying(&fixture, &dir.to_string_lossy()).await;
    }
    fixture.provider.release();
    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "nothing cancelled went out again: {:?}",
        fixture.provider.requests()
    );
    for dir in &dirs {
        assert!(
            fixture.identified_for(dir).await.is_none(),
            "{} stores nothing",
            dir.display()
        );
    }
}

/// An answer that is being written when it is cancelled gives itself up
/// before the write: the candidate stores nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_answer_is_not_written() {
    let fixture = Fixture::new("cancel-settling").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .route("/discid/", 200, discid_json("mb-1", "rg-1", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-1?",
        200,
        release_json("mb-1", "rg-1", &[probed, 0]),
    );
    fixture.provider.hold("/release/mb-1?");
    fixture.scan(1).await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/release/mb-1?", 1).await;
    assert_eq!(
        fixture.identification_status(&key),
        Some(crate::import::IdentificationStatus::Finalizing),
        "the run answered and its answer is being written"
    );

    fixture.identification().cancel(vec![key.clone()]);
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), sweep)
        .await
        .expect("cancelling the only job drains the pass")
        .unwrap();
    await_not_identifying(&fixture, &key).await;

    assert!(fixture.identified_for(&dir).await.is_none());
    assert!(fixture.import.candidate_runtime(&key).is_none());
}

/// A run the queue never started — a re-identify sheet's — ends through the
/// same cancel: one cancel for a candidate's identification, however it began.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_the_queue_did_not_start_ends_through_the_same_cancel() {
    let fixture = Fixture::new("cancel-outside-queue").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let candidate = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");
    let run = fixture.import.new_identification_run();
    assert!(fixture.import.start_identification(
        run,
        key.clone(),
        ExtractionSource::Candidate { candidate },
        CallPriority::Interactive,
        LookupChoices::default(),
        None,
    ));
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    assert!(fixture.import.is_identifying(&key));

    fixture.identification().cancel(vec![key.clone()]);
    await_not_identifying(&fixture, &key).await;
    fixture.provider.release();

    assert!(fixture.identified_for(&dir).await.is_none());
}
