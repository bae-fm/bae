// A release is queued on its own only when a scan first finds it while the
// setting is on.

/// A release found while the setting is on is identified; one found after it
/// was turned off is not.
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

/// Turning the setting on queues no candidate found while it was off.
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

/// A rescan queues no candidate it had found, even with new files in its
/// folder.
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

/// The launch scan queues no stored candidate, even one found while the
/// setting was on whose run the last session never started.
#[tokio::test(flavor = "multi_thread")]
async fn a_relaunch_queues_no_candidate_the_library_stored() {
    let fixture = Fixture::new("relaunch-queues-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.route("/discid/", 200, "{}");
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

/// Turning the setting off stops nothing already queued.
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

/// A takeover queues only a release new to the parent.
#[tokio::test(flavor = "multi_thread")]
async fn a_takeover_queues_only_a_release_new_to_the_parent() {
    let fixture = Fixture::new("takeover-queues-only-new").await;
    let inner = fixture.root.join("Inner");
    let known = fixture.disc_id_candidate("Inner/Album");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.manager.set_identify_automatically(false).await.unwrap();
    let events = fixture.import.subscribe_events();
    let inner_key = inner.to_string_lossy().into_owned();
    fixture.import.add_watched_folder(inner_key.clone()).await.unwrap();
    fixture.import.refresh_watched_folder(inner_key).await.unwrap();
    fixture.await_scanned(&fixture.import, events, 1).await;
    fixture.manager.set_identify_automatically(true).await.unwrap();

    let new = fixture.disc_id_candidate("New Album");
    std::fs::write(new.join("notes.txt"), "a candidate of its own").unwrap();
    let events = fixture.import.subscribe_events();
    fixture
        .import
        .add_watched_folder(fixture.root.to_string_lossy().into_owned())
        .await
        .unwrap();
    fixture.await_scanned(&fixture.import, events, 2).await;
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&known).await.is_none(),
        "the candidate the inner folder had found waits for a person"
    );
    assert!(
        fixture.identified_for(&new).await.is_some(),
        "the release new to the parent is identified on its own"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "one release was asked about: {:?}",
        fixture.provider.requests()
    );
}
