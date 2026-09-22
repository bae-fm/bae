// ── Two admissions, one queue ───────────────────────────────────────────────
//
// What a request and the automatic admission do to each other: the front of
// the queue, the cap, the identity group, and the count.

/// A request on a candidate the automatic admission has waiting starts it now,
/// at interactive priority: the person is waiting on it, so it does not sit
/// behind the cap, and it is the same entry rather than a second one.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_request_on_a_waiting_candidate_starts_it_ahead_of_the_cap() {
    let fixture = Fixture::new("request-upgrades-waiting").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let mut dirs = Vec::new();
    for index in 0..MAX_IN_FLIGHT + 1 {
        let dir = fixture.barcode_candidate(&format!("Album {index}"));
        std::fs::write(dir.join(format!("playlist-{index}.m3u")), format!("{index}")).unwrap();
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

    let priority = await_run_priority(&mut events, &waiting_key).await;
    assert_eq!(
        priority,
        CallPriority::Interactive,
        "the requested candidate starts at the priority a person waiting on it \
         deserves, without waiting for the cap to free"
    );
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT + 1).await;
    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT + 1,
        "one lookup for it, not a second entry's as well: {:?}",
        fixture.provider.requests()
    );

    fixture.identification().shut_down();
    fixture.provider.release();
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
#[serial(musicbrainz)]
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

/// Switching automatic identification off takes back what the automatic
/// admission put on the queue, and leaves a candidate a person asked for
/// running: the setting is about what nobody asked for.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn automatic_off_leaves_a_requested_candidate_running() {
    let fixture = Fixture::new("automatic-off-keeps-request").await;
    let requested = fixture.disc_id_candidate("Requested");
    let automatic = fixture.disc_id_candidate("Automatic");
    std::fs::write(automatic.join("notes.txt"), "distinct candidate").unwrap();
    let requested_key = requested.to_string_lossy().into_owned();
    let automatic_key = automatic.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    fixture.start_explicit_lookup_and_await_run(&requested).await;
    let sweep = fixture.sweep();
    wait_for_request(&fixture.provider, "/discid/", 2).await;

    fixture.manager.set_identify_automatically(false).unwrap();

    tokio::time::timeout(Duration::from_secs(20), sweep)
        .await
        .expect("the automatic admission gives up what it admitted")
        .unwrap();
    assert_eq!(
        fixture.identification_status(&automatic_key),
        None,
        "the candidate nobody asked for is off the queue"
    );
    assert!(
        !fixture.import.is_identifying(&automatic_key),
        "and its run is cancelled"
    );
    assert!(
        fixture.import.is_identifying(&requested_key),
        "the candidate a person asked for is still being answered"
    );
    fixture.provider.release();
}

/// A run a person asked for is counted like any other: it opens a batch of its
/// own, and the batch is over when its answer lands.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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
