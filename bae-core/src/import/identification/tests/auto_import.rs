// ── Importing when identified ───────────────────────────────────────────────
//
// An automatic run that stores an auto-importable verdict, while "Import automatically
// when identified" is on, starts its candidate's import right then — the same
// start a person's Import press makes. Nothing else imports on its own.

/// Every import the worker reports on for `key` until one ends: the ids it
/// reported under, and the error the ending one failed with, if it failed.
async fn await_import_ending(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ImportEvent>,
    key: &str,
) -> (std::collections::BTreeSet<String>, Option<String>) {
    let mut ids = std::collections::BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = events.recv().await.expect("the import event bus stays open");
            let ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } = event
            else {
                continue;
            };
            if candidate_key != key {
                continue;
            }
            match progress {
                crate::import::ImportProgress::Preparing { import_id, .. }
                | crate::import::ImportProgress::Progress { import_id, .. }
                | crate::import::ImportProgress::RemoteUploadQueued { import_id, .. } => {
                    ids.insert(import_id);
                }
                crate::import::ImportProgress::Complete { import_id, .. } => {
                    ids.insert(import_id);
                    return None;
                }
                crate::import::ImportProgress::Failed { import_id, error } => {
                    ids.insert(import_id);
                    return Some(error);
                }
                crate::import::ImportProgress::Cancelled { import_id } => {
                    ids.insert(import_id);
                    return Some("cancelled".to_string());
                }
            }
        }
    })
    .await
    .map(|failure| (ids, failure))
    .expect("an import of the candidate ends")
}

/// Nothing imported `key`, asked once the queue is done with it: the worker
/// runs every import it was handed before it stops, so `events` then holds
/// every report there will be.
async fn assert_no_import(
    fixture: &Fixture,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ImportEvent>,
    key: &str,
    after: &str,
) {
    let import = fixture.import.clone();
    tokio::task::spawn_blocking(move || import.stop_and_join())
        .await
        .expect("the import worker stops");
    for event in drain_events(events) {
        if let ImportEvent::ImportProgress {
            candidate_key,
            progress,
        } = event
        {
            assert_ne!(
                candidate_key, key,
                "nothing imports {key} after {after}, got {progress:?}"
            );
        }
    }
}

impl Fixture {
    /// Route a disc-ID match for `dir` to one release listing `tracks` tracks
    /// (the fixture folder holds two).
    fn route_disc_id_match(&self, dir: &Path, release_id: &str, group_id: &str, tracks: usize) {
        let probed = self.probed_total_ms(dir);
        let mut lengths = vec![0; tracks];
        lengths[0] = probed;
        self.provider
            .route("/discid/", 200, discid_json(release_id, group_id, &lengths));
        self.provider.route(
            &format!("/release/{release_id}?"),
            200,
            release_json(release_id, group_id, &lengths),
        );
    }

    /// An automatic run's auto-importable verdict for `dir`, stored while importing when
    /// identified is off.
    async fn identify_ready(&self, dir: &Path, release_id: &str, group_id: &str) {
        self.route_disc_id_match(dir, release_id, group_id, 2);
        self.scan(1).await;
        self.drain_automatic().await;
        assert_eq!(
            self.judgement_for(dir).await,
            (true, None)
        );
    }

    /// The failed import `dir`'s pane shows, if any.
    async fn import_failure(&self, dir: &Path) -> Option<crate::import::ImportFailure> {
        self.manager
            .load_import_candidate_pane_rows(&self.content_hash(dir))
            .await
            .unwrap()
            .failure
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_auto_importable_imports_its_candidate_once() {
    let fixture = Fixture::importing("auto-importable").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-auto", "rg-auto", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None)
    );

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");

    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;
    assert_no_import(&fixture, &mut events, &key, "the watched folder was read again").await;
}

/// With the setting off, an auto-importable verdict waits for a person.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_auto_importable_with_the_setting_off_imports_nothing() {
    let fixture = Fixture::new("auto-import-setting-off").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-setting-off", "rg-setting-off", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None)
    );
    assert_no_import(&fixture, &mut events, &key, "the run settled with the setting off").await;
}

/// Two pressings are a question for the person, so nothing is imported.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_needing_you_imports_nothing() {
    let fixture = Fixture::new("auto-import-needs-you").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[
            ("mb-two-1", "rg-two-1", PAIRED_BARCODE),
            ("mb-two-2", "rg-two-1", "9876543210987"),
        ]),
    );
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, None)
    );
    assert_no_import(&fixture, &mut events, &key, "the run settled needing a person").await;
}

/// A folder its rip log proves is a CD rip, and a disc ID answered by a
/// release whose record states vinyl: the one pressing is offered, but the
/// folder's own files rule it out, so nothing applies it or imports it.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_the_folder_rules_out_is_neither_applied_nor_imported() {
    let fixture = Fixture::importing("auto-import-medium").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json("mb-vinyl", "rg-vinyl", &[probed, 0]))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["media"][0]["format"] = serde_json::json!("12\" Vinyl");
    let mut document: serde_json::Value =
        serde_json::from_str(&release_json("mb-vinyl", "rg-vinyl", &[probed, 0]))
            .expect("the release fixture parses");
    document["media"][0]["format"] = serde_json::json!("12\" Vinyl");
    fixture.provider.route("/discid/", 200, answer.to_string());
    fixture
        .provider
        .route("/release/mb-vinyl?", 200, document.to_string());
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, Some(FolderCheck::MediumDisagrees {
            folder: crate::identify::MediumConflict::CdRip
        }))
    );
    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "the release the folder rules out was not applied to the draft"
    );
    assert_eq!(
        fixture.count_release_lookups("mb-vinyl"),
        1,
        "the offered row was read in full once, like any other"
    );
    assert_no_import(&fixture, &mut events, &key, "the folder ruled the release out").await;
}

/// Turning the setting on imports only what settles from then on: a candidate
/// already identified and ready stays where it is for the person.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_auto_importable_before_the_setting_was_on_is_not_imported() {
    let fixture = Fixture::new("auto-import-earlier").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-earlier", "rg-earlier").await;
    let mut events = fixture.import.every_event();

    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    assert_no_import(&fixture, &mut events, &key, "the setting was turned on").await;
}

/// A run a person asked for is theirs to act on, whatever the setting says.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_imports_nothing() {
    let fixture = Fixture::new("auto-import-requested").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-requested", "rg-requested", 2);
    // Found while identification waits to be asked, so the one run it gets is
    // the person's; importing when identified is read only while it runs on
    // its own, so it goes back on before they ask.
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.scan(1).await;
    fixture.manager.set_identify_automatically(true).await.unwrap();
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
    // The queue takes the job off before it would start an import, and
    // answers an empty cancel only once it is past that.
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.identification_status(&key).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the person's run leaves the queue");
    fixture.identification().cancel(Vec::new()).await.unwrap();

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None)
    );
    assert_no_import(&fixture, &mut events, &key, "the person's run settled").await;
}

/// An automatic start the import path refuses is the candidate's failed
/// import, shown where one that failed while running is, with nothing claimed.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_that_cannot_start_is_recorded_as_its_failed_import() {
    let fixture = Fixture::new("auto-import-refused").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-refused", "rg-refused").await;
    // A draft whose assets are not prepared is one no import starts from.
    fixture
        .preparations
        .set_album_artists(
            &fixture.content_hash(&dir),
            &[crate::import::ArtistAssignment::named("Changed Artist")],
        )
        .await
        .unwrap();

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("an unprepared draft does not start an import");

    let failure = fixture
        .import_failure(&dir)
        .await
        .expect("the refused start is the candidate's failed import");
    assert_eq!(
        failure.reason,
        crate::import::ImportFailureReason::error(error.to_string())
    );
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .is_none_or(|runtime| runtime.import.is_none()),
        "nothing claims the candidate"
    );
}

/// An import that already owns the candidate is not a failure of the
/// automatic one: it says how it goes itself.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_behind_another_import_records_no_failure() {
    let fixture = Fixture::new("auto-import-behind").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-behind", "rg-behind").await;
    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a candidate an import owns is not imported again");

    assert!(
        matches!(error, crate::import::ImportError::CandidateImportInProgress),
        "{error}"
    );
    assert_eq!(fixture.import_failure(&dir).await, None);
}
