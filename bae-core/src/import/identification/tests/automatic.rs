// ── Identification on its own starts where a release is found ───────────────
//
// A release is queued on its own only when a scan first finds it while the
// setting is on.

/// A release found while identification runs on its own is identified; one
/// found after it was turned off is not, even once the queue is running.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_release_found_while_automatic_is_on_is_identified_on_its_own() {
    let fixture = Fixture::new("found-while-on").await;
    let on = fixture.disc_id_candidate("Found While On");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.scan(1).await;

    fixture.manager.set_identify_automatically(false).await.unwrap();
    let off = fixture.disc_id_candidate("Found While Off");
    std::fs::write(off.join("notes.txt"), "a candidate of its own").unwrap();
    fixture.rescan(&fixture.import, 2).await;
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&on).await.is_some(),
        "the release found while identification ran on its own is identified"
    );
    assert!(
        fixture.identified_for(&off).await.is_none(),
        "the release found after it was turned off waits for a person"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "one release was asked about: {:?}",
        fixture.provider.requests()
    );
}

/// Turning identification on reaches back to nothing: a candidate found while
/// it was off stays unidentified.
#[tokio::test(flavor = "multi_thread")]
async fn turning_automatic_identification_on_queues_no_candidate_already_found() {
    let fixture = Fixture::new("on-reaches-back-to-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.scan(1).await;
    fixture.drain_automatic().await;

    fixture.manager.set_identify_automatically(true).await.unwrap();
    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "nothing was asked about the candidate found while it was off: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// A rescan finds nothing it had found: a candidate it reads again is queued
/// no more than it was, whatever became of its files — new files under a key
/// the library holds are the same candidate.
#[tokio::test(flavor = "multi_thread")]
async fn a_rescan_queues_no_candidate_it_had_found() {
    let fixture = Fixture::new("rescan-queues-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.scan(1).await;
    let files_before = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable")
        .files
        .content_hash();
    fixture.manager.set_identify_automatically(true).await.unwrap();
    fixture.drain_automatic().await;

    fixture.rescan(&fixture.import, 1).await;
    std::fs::write(dir.join("notes.txt"), "a file added under the same folder").unwrap();
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    let files_after = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the rescanned candidate is answerable")
        .files
        .content_hash();
    assert_ne!(
        files_before, files_after,
        "the rescan stored the folder's new files under the same candidate"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "no rescan queued the candidate: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// A relaunch finds nothing new: the candidates the library stored are
/// already found, so the launch scan queues none of them — even one found
/// while identification ran on its own whose run the last session never got
/// to.
#[tokio::test(flavor = "multi_thread")]
async fn a_relaunch_queues_no_candidate_the_library_stored() {
    let fixture = Fixture::new("relaunch-queues-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.route("/discid/", 200, "{}");
    // Found while identification runs on its own, and the session ends before
    // any queue takes it.
    fixture.scan(1).await;
    fixture.import.stop_and_join();

    let import = fixture
        .manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let identification = super::start(import.clone(), fixture.manager.clone());
    fixture.rescan(&import, 1).await;
    tokio::time::timeout(
        Duration::from_secs(30),
        identification.automatic_drained_for_test(),
    )
    .await
    .expect("the relaunched queue drains");

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "the relaunch queued nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
    assert!(
        import.candidate_runtime(&dir.to_string_lossy()).is_none(),
        "nothing is waiting either"
    );
    identification.stop();
    import.stop_and_join();
}

/// Turning identification off stops nothing: the runs it started and the
/// candidate still waiting for a slot are all answered.
#[tokio::test(flavor = "multi_thread")]
async fn turning_automatic_identification_off_finishes_what_is_queued() {
    let fixture = Fixture::new("off-finishes-queue").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.drain_automatic_task();
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT).await;
    let waiting = dirs
        .iter()
        .find(|dir| {
            fixture.identification_status(&dir.to_string_lossy())
                == Some(crate::import::IdentificationStatus::Queued)
        })
        .expect("one candidate is over the cap and waiting")
        .clone();

    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(30), sweep)
        .await
        .expect("the queue finishes what it held")
        .unwrap();

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT + 1,
        "the waiting candidate was still looked up: {:?}",
        fixture.provider.requests()
    );
    for dir in &dirs {
        assert!(
            fixture.identified_for(dir).await.is_some(),
            "{} is answered{}",
            dir.display(),
            if *dir == waiting {
                ", though it was still waiting when identification was turned off"
            } else {
                ""
            }
        );
    }
}
