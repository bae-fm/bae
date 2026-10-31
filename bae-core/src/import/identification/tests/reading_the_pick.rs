// ── Reading the pick beside the queue ───────────────────────────────────────
//
// A run that picks its release unattended hands its answer to a settle,
// which reads that release in full before anything is written. The settle
// holds none of the queue's slots, the row reads Identified while it reads,
// and an Import asked for meanwhile waits for the pick to be stored and then
// imports, as automatic import does.

impl Fixture {
    /// Wait until the runtime says `status` of `key`.
    async fn await_identification_status(
        &self,
        key: &crate::import::CandidateKey,
        status: crate::import::IdentificationStatus,
    ) {
        let reached = tokio::time::timeout(Duration::from_secs(20), async {
            while self.identification_status(key).await.as_ref() != Some(&status) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        if reached.is_err() {
            panic!(
                "{key} never read as {status:?}, it reads {:?}; requests so far: {:?}",
                self.identification_status(key).await,
                self.provider.requests()
            );
        }
    }

    /// A disc-ID candidate whose one release `release_id` the run picks
    /// unattended, with the settle's read of that release held, and its run
    /// asked for and reading the pick. Returns its folder and key.
    async fn reading_its_pick(&self, release_id: &str, group_id: &str) -> (PathBuf, crate::import::CandidateKey) {
        self.manager.set_identify_automatically(false).await.unwrap();
        let dir = self.disc_id_candidate("Album");
        let key = crate::import::CandidateKey::folder(&dir);
        self.route_disc_id_match(&dir, release_id, group_id, 2);
        self.provider.hold(&format!("/release/{release_id}?"));
        self.scan(1).await;
        self.start_explicit_lookup(&dir);
        self.await_identification_status(
            &key,
            crate::import::IdentificationStatus::ReadingPick {
                import_asked: false,
            },
        )
        .await;
        (dir, key)
    }

    /// Let the held read answer and wait for the queue to be done.
    async fn finish_reading(&self) {
        self.provider.release();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.identification().drained_for_test(),
        )
        .await
        .expect("the settle ends");
    }
}

/// With every slot's run going but one, a run that answers and is reading its
/// pick lets the job waiting behind it start: the read holds no slot.
#[tokio::test(flavor = "multi_thread")]
async fn a_settle_reading_its_pick_holds_no_slot() {
    let fixture = Fixture::new("settle-holds-no-slot").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture
        .import
        .register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let settling = fixture.disc_id_candidate("Album 0");
    let settling_key = crate::import::CandidateKey::folder(&settling);
    fixture.route_disc_id_match(&settling, "mb-slot", "rg-slot", 2);
    let mut running = Vec::new();
    for index in 1..=MAX_IN_FLIGHT {
        let dir = fixture.barcode_candidate(&format!("Album {index}"));
        std::fs::write(
            dir.join(format!("playlist-{index}.m3u")),
            format!("{index}"),
        )
        .unwrap();
        running.push(dir);
    }
    fixture.provider.route("/release?", 200, "{}");
    // Both the barcode searches and the settle's read of the pick wait here.
    fixture.provider.hold_each(&["query=barcode", "/release/mb-slot?"]);
    fixture.scan(MAX_IN_FLIGHT + 1).await;

    fixture.start_explicit_lookup(&settling);
    fixture
        .await_identification_status(
            &settling_key,
            crate::import::IdentificationStatus::ReadingPick {
                import_asked: false,
            },
        )
        .await;
    for dir in &running {
        fixture.start_explicit_lookup(dir);
    }

    // The settle's read and every slot's lookup are out at once.
    wait_for_request(&fixture.provider, "query=barcode", MAX_IN_FLIGHT).await;
    assert_eq!(
        fixture.count_release_lookups("mb-slot"),
        1,
        "the settle is still reading the pick: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.identification_status(&settling_key).await,
        Some(crate::import::IdentificationStatus::ReadingPick {
            import_asked: false,
        })
    );
    for dir in &running {
        assert_eq!(
            fixture.identification_status(&crate::import::CandidateKey::folder(dir)).await,
            Some(crate::import::IdentificationStatus::Running),
            "{} started while the pick was read",
            dir.display()
        );
    }

    fixture.finish_reading().await;
    assert_eq!(fixture.judgement_for(&settling).await, (true, None));
}

/// The row of a run reading its pick is Identified, counted under Identified
/// and not In Progress, shows no lookup at work, and offers importing beside
/// cancelling the run and skipping; its pane shows the pick being read until
/// it is stored.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_whose_pick_is_being_read_is_identified() {
    let fixture = Fixture::new("reading-the-pick-row").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::import::CandidateKey::folder(&dir);
    fixture.route_disc_id_match(&dir, "mb-row", "rg-row", 2);
    fixture.provider.hold("/release/mb-row?");
    fixture.scan(1).await;
    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    assert_eq!(counted(&initial, crate::import::PendingFilter::Identified), 0);

    fixture.start_explicit_lookup(&dir);
    let reading = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = next_page(&list).await;
            if page_live(&snapshot)[&key].facts.identification
                == Some(crate::import::IdentificationStatus::ReadingPick {
                    import_asked: false,
                })
            {
                return snapshot;
            }
        }
    })
    .await
    .expect("the row reads as its pick being read");

    let live = &page_live(&reading)[&key];
    assert_eq!(live.standing, Some(crate::import::PendingStanding::Identified));
    assert_eq!(live.identifying(), None, "an Identified row shows no lookup at work");
    assert_eq!(
        live.actions,
        vec![
            crate::import::CandidateAction::Import,
            crate::import::CandidateAction::CancelIdentification,
            crate::import::CandidateAction::Skip,
            crate::import::CandidateAction::RevealFolder
        ]
    );
    assert_eq!(counted(&reading, crate::import::PendingFilter::Identified), 1);
    assert_eq!(counted(&reading, crate::import::PendingFilter::InProgress), 0);
    let pane_reads_pick = || async {
        fixture
            .import
            .candidate_pane(&key)
            .await
            .unwrap()
            .expect("the pane reads the candidate")
            .live
            .reading_pick()
    };
    assert!(pane_reads_pick().await, "the pane shows the pick being read");

    fixture.finish_reading().await;
    assert!(
        !pane_reads_pick().await,
        "once the pick is stored, the pane shows none being read"
    );
}

/// Import from the pane while the pick is read does not cancel the read nor
/// import the draft stored now: it waits, and the candidate imports once,
/// after the pick is stored, with importing when identified off.
#[tokio::test(flavor = "multi_thread")]
async fn import_from_the_pane_while_the_pick_is_read_imports_it_once_stored() {
    let fixture = Fixture::importing("import-asked-from-the-pane").await;
    let (dir, key) = fixture.reading_its_pick("mb-pane", "rg-pane").await;
    let mut events = fixture.import.every_event();

    let started = fixture
        .import
        .start_import(&key)
        .await
        .expect("the import is asked for");
    assert_eq!(started, crate::import::ImportStart::WhenPickStored);
    assert_eq!(
        fixture.identification_status(&key).await,
        Some(crate::import::IdentificationStatus::ReadingPick { import_asked: true }),
        "the read goes on, asked to import"
    );
    assert!(
        fixture
            .import
            .candidate_runtime(&key).await.unwrap()
            .is_some_and(|runtime| runtime.import.is_none()),
        "nothing is claimed while the pick is read"
    );

    fixture.provider.release();
    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    let row = fixture.stored_for(&dir).await.expect("the candidate's row");
    assert_eq!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "what imported is the pick's draft"
    );

    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.identification().drained_for_test(),
    )
    .await
    .expect("the queue has nothing left to do");
    assert_no_import(&fixture, &mut events, &key, "the asked-for import ended").await;
}

/// Import of a selection reaching a row whose pick is read asks the same,
/// rather than refusing the row as one being identified.
#[tokio::test(flavor = "multi_thread")]
async fn importing_a_selection_while_the_pick_is_read_imports_it_once_stored() {
    let fixture = Fixture::importing("import-asked-from-a-selection").await;
    let (_dir, key) = fixture.reading_its_pick("mb-selection", "rg-selection").await;
    let mut events = fixture.import.every_event();

    let started = fixture
        .import
        .import_selected(&key)
        .await
        .expect("the selected row is not refused");
    assert_eq!(started, crate::import::ImportStart::WhenPickStored);

    fixture.provider.release();
    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");

    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.identification().drained_for_test(),
    )
    .await
    .expect("the queue has nothing left to do");
    assert_no_import(&fixture, &mut events, &key, "the asked-for import ended").await;
}

/// A read that fails stores no pick, so the Import asked for while it ran
/// imports nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_that_fails_after_import_was_asked_imports_nothing() {
    let fixture = Fixture::importing("import-asked-read-fails").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::import::CandidateKey::folder(&dir);
    // The run reads its release from the Disc ID answer; the pick's read of
    // the release itself is refused.
    fixture.provider.route("/release/mb-fails?", 400, "{}");
    fixture.route_disc_id_match(&dir, "mb-fails", "rg-fails", 2);
    fixture.provider.hold("/release/mb-fails?");
    fixture.scan(1).await;
    fixture.start_explicit_lookup(&dir);
    fixture
        .await_identification_status(
            &key,
            crate::import::IdentificationStatus::ReadingPick {
                import_asked: false,
            },
        )
        .await;
    let mut events = fixture.import.every_event();

    assert_eq!(
        fixture.import.start_import(&key).await.expect("the import is asked for"),
        crate::import::ImportStart::WhenPickStored
    );

    fixture.finish_reading().await;
    assert_ne!(
        fixture
            .stored_for(&dir)
            .await
            .map(|row| row.metadata_author),
        Some(crate::import::MetadataAuthor::Identification),
        "no pick was stored"
    );
    assert_no_import(&fixture, &mut events, &key, "the pick's read failed").await;
}
