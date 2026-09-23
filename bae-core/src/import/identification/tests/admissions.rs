// ── Two admissions, one queue ───────────────────────────────────────────────
//
// What a request and the automatic admission do to each other: the front of
// the queue, the cap, the identity group, and the count.

/// A request on a candidate the automatic admission has waiting moves it to
/// the front of the queue as the same entry, and it takes the next slot at
/// interactive priority — it does not open a fifth slot: the cap bounds local
/// work whoever asked for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_on_a_waiting_candidate_takes_the_next_slot() {
    let fixture = Fixture::new("request-upgrades-waiting").await;
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

    let sweep = fixture.sweep();
    // The cap's worth of lookups are out, and one candidate is still waiting.
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
    // Subscribed here, so the only states this reads are the requested run's.
    let mut events = fixture.import.subscribe_events();

    fixture.start_explicit_lookup(&waiting);

    // Still waiting: every slot is held, and a request is not a fifth one.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        fixture.identification_status(&waiting_key),
        Some(crate::import::IdentificationStatus::Queued),
        "the request waits for a slot like any other job"
    );
    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "no lookup went out past the cap: {:?}",
        fixture.provider.requests()
    );

    fixture.provider.release();
    let priority = await_run_priority(&mut events, &waiting_key).await;
    assert_eq!(
        priority,
        CallPriority::Interactive,
        "the requested candidate takes the next slot at the priority a person \
         waiting on it deserves"
    );
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT + 1).await;
    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT + 1,
        "one lookup for it, not a second entry's as well: {:?}",
        fixture.provider.requests()
    );

    fixture.identification().shut_down();
    let _ = tokio::time::timeout(Duration::from_secs(20), sweep).await;
}

/// The priority the next run of `key` reports.
async fn await_run_priority(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    key: &str,
) -> CallPriority {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let ImportEvent::IdentifyStateChanged {
                candidate_key,
                priority,
                ..
            } = events
                .recv()
                .await
                .expect("the import event bus stays open")
            {
                if candidate_key == key {
                    return priority;
                }
            }
        }
    })
    .await
    .expect("a run of the candidate broadcasts a state")
}

/// A request that supersedes an automatic run keeps the rest of the identity
/// group waiting on the answer it will store, rather than dropping them: they
/// are the same bytes, and one answer settles all of them.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_superseding_a_run_keeps_the_rest_of_its_group() {
    let fixture = Fixture::new("request-keeps-group").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    assert_eq!(
        fixture.content_hash(&first),
        fixture.content_hash(&second),
        "the two folders hold the same bytes"
    );
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-group-1", "rg-group-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-group-1?",
        200,
        release_json("mb-group-1", "rg-group-1", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    // Whichever of them the automatic admission is running, the person asks
    // about the other one.
    let running = if fixture.import.is_identifying(&first.to_string_lossy()) {
        first.clone()
    } else {
        second.clone()
    };
    let other = if running == first { second } else { first };

    fixture.start_explicit_lookup(&other);

    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.identification_status(&other.to_string_lossy())
            != Some(crate::import::IdentificationStatus::Running)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the request supersedes the run that was going");
    assert_eq!(
        fixture.identification_status(&running.to_string_lossy()),
        Some(crate::import::IdentificationStatus::Queued),
        "the candidate whose run was superseded waits on the answer this one \
         stores rather than being dropped"
    );

    fixture.provider.release();
    fixture.await_identified_row(&running).await;
    tokio::time::timeout(Duration::from_secs(20), sweep)
        .await
        .expect("the automatic admission has nothing left outstanding")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture
            .identification_status(&running.to_string_lossy())
            .is_some()
            || fixture
                .identification_status(&other.to_string_lossy())
                .is_some()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the stored answer retires every candidate it covers");
}

/// Switching automatic identification off admits nothing further, and takes
/// nothing back: what is on the queue — asked for or admitted on its own —
/// runs to its answer. A preference is not a cancel.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_off_leaves_the_queue_to_finish() {
    let fixture = Fixture::new("automatic-off-keeps-queue").await;
    let requested = fixture.disc_id_candidate("Requested");
    let automatic = fixture.disc_id_candidate("Automatic");
    std::fs::write(automatic.join("notes.txt"), "distinct candidate").unwrap();
    let requested_key = requested.to_string_lossy().into_owned();
    let automatic_key = automatic.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    fixture
        .start_explicit_lookup_and_await_run(&requested)
        .await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 2).await;

    fixture.manager.set_identify_automatically(false).unwrap();

    assert!(
        fixture.import.is_identifying(&automatic_key),
        "the candidate the setting admitted keeps running after it turns off"
    );
    assert!(
        fixture.import.is_identifying(&requested_key),
        "the candidate a person asked for is still being answered"
    );
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), sweep)
        .await
        .expect("the automatic admission's queue finishes what it admitted")
        .unwrap();
    assert!(
        fixture.identified_for(&automatic).await.is_some(),
        "the run the setting admitted stores its answer"
    );
    assert_eq!(fixture.identification_status(&automatic_key), None);
}

/// A run a person asked for is counted like any other: it opens a batch of its
/// own, and the batch is over when its answer lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_requested_run_is_counted() {
    let fixture = Fixture::new("requested-is-counted").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-counted", "rg-counted", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-counted?",
        200,
        release_json("mb-counted", "rg-counted", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.manager.set_identify_automatically(false).unwrap();

    let mut events = fixture.import.subscribe_events();
    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;

    let progress = tokio::time::timeout(Duration::from_secs(10), async {
        let mut progress = Vec::new();
        loop {
            if let ImportEvent::IdentificationProgress { identified, total } = events
                .recv()
                .await
                .expect("the import event bus stays open")
            {
                progress.push((identified, total));
                if progress.last() == Some(&(0, 0)) {
                    return progress;
                }
            }
        }
    })
    .await
    .expect("the batch a request opened drains");
    assert_eq!(
        progress.first(),
        Some(&(0, 1)),
        "the request opens a batch of one: {progress:?}"
    );
}

/// Admitting a candidate opens its pane on Find online — the page its run
/// reports on — whoever admitted it, so a person who clicks into a candidate
/// while it is being identified is on the run rather than on the draft it
/// started from.
#[tokio::test(flavor = "multi_thread")]
async fn an_admitted_candidate_opens_on_find_online() {
    let fixture = Fixture::new("admitted-opens-on-find-online").await;
    let dir = fixture.disc_id_candidate("Candidate");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("scanned")
            .session
            .presentation,
        crate::import::MetadataPresentation::Draft,
        "a scanned candidate nobody has touched opens on its draft"
    );

    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 1).await;

    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("still scanned")
            .session
            .presentation,
        crate::import::MetadataPresentation::FindOnline,
        "the admission moved the pane to the page its run reports on"
    );

    fixture.identification().shut_down();
    fixture.provider.release();
    let _ = tokio::time::timeout(Duration::from_secs(20), sweep).await;
}
