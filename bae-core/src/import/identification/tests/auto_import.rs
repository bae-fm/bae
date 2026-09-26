// ── Importing when identified ───────────────────────────────────────────────
//
// An automatic run that settles on a verdict needing nothing, while "Import
// automatically when identified" is on, owes an import; the queue pays it
// once, from the stored row, and nothing else owes one.

/// Every import the worker reports on for `key` until one ends: the ids it
/// reported under, and the error the ending one failed with, if it failed.
async fn await_import_ending(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    key: &str,
) -> (std::collections::BTreeSet<String>, Option<String>) {
    let mut ids = std::collections::BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(error) => panic!("the import event bus closed: {error}"),
            };
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

/// No import of `key` is reported on for a second after `after`.
async fn assert_no_import(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    key: &str,
    after: &str,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Err(_) => return,
            Ok(Ok(ImportEvent::ImportProgress {
                candidate_key,
                progress,
            })) if candidate_key == key => {
                panic!("nothing imports {key} after {after}, got {progress:?}")
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => return,
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

    /// Read the watched root again, so the queue reads everything afresh the
    /// way it does at launch.
    async fn rescan(&self) {
        self.import
            .refresh_watched_folder(self.root.to_string_lossy().into_owned())
            .await
            .unwrap();
    }

    async fn owed_import(&self, dir: &Path) -> Option<u64> {
        self.manager
            .load_owed_import(&self.content_hash(dir))
            .await
            .unwrap()
    }

    /// Record that `dir`'s stored verdict owes its import, for the draft it
    /// holds now — the state the app leaves when it closes between storing a
    /// verdict and starting its import.
    async fn owe_import(&self, dir: &Path) {
        let hash = self.content_hash(dir);
        let revision = self
            .manager
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the candidate has a state row")
            .metadata_revision;
        self.manager.owe_import_for_test(&hash, revision).await.unwrap();
    }

    /// An automatic run's Ready verdict for `dir`, stored while importing when
    /// identified was off, so it owes nothing yet.
    async fn identify_ready(&self, dir: &Path, release_id: &str, group_id: &str) {
        self.route_disc_id_match(dir, release_id, group_id, 2);
        self.scan(1).await;
        self.sweep_once().await;
        assert_eq!(
            self.classification_for(dir).await,
            QueueClassification::Ready
        );
        assert!(self.owed_import(dir).await.is_none());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_run_that_settles_ready_imports_its_candidate_once() {
    let fixture = Fixture::importing("auto-import-ready").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-auto", "rg-auto", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();

    fixture.sweep_once().await;
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready
    );

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
    assert!(
        fixture.owed_import(&dir).await.is_none(),
        "the import that ran answered what was owed"
    );

    fixture.rescan().await;
    fixture.sweep_once().await;
    assert_no_import(&mut events, &key, "the queue read everything again").await;
}

/// Two pressings are a question for the person, so the verdict owes nothing.
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
    let mut events = fixture.import.subscribe_events();

    fixture.sweep_once().await;

    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
    assert!(
        fixture.owed_import(&dir).await.is_none(),
        "a verdict that asks something owes nothing"
    );
    assert_no_import(&mut events, &key, "the run settled needing a person").await;
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
    fixture.provider.route("/discid/", 200, answer.to_string());
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();

    fixture.sweep_once().await;

    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::NeedsYou(NeedsYou::MediumDisagrees {
            folder: crate::identify::MediumConflict::CdRip
        })
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
    assert_eq!(fixture.count_release_lookups("mb-vinyl"), 0);
    assert!(fixture.owed_import(&dir).await.is_none());
    assert_no_import(&mut events, &key, "the folder ruled the release out").await;
}

/// Turning the setting on imports only what settles from then on: a candidate
/// already identified and ready stays where it is for the person.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_ready_before_the_setting_was_on_is_not_imported() {
    let fixture = Fixture::new("auto-import-earlier").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-earlier", "rg-earlier", 2);
    fixture.scan(1).await;
    fixture.sweep_once().await;
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready
    );
    let mut events = fixture.import.subscribe_events();

    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture.rescan().await;

    assert!(fixture.owed_import(&dir).await.is_none());
    assert_no_import(&mut events, &key, "the setting was turned on").await;
}

/// A run a person asked for is theirs to act on: its verdict owes nothing,
/// whatever the setting says.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_imports_nothing() {
    let fixture = Fixture::new("auto-import-requested").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    // The queue starts with the person's request, after the scan has
    // finished, so the one run this candidate gets is the one they asked for.
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-requested", "rg-requested", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;

    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready
    );
    assert!(fixture.owed_import(&dir).await.is_none());
    assert_no_import(&mut events, &key, "the person's run settled").await;
}

/// The app closed after the verdict was stored and before its import started:
/// the next launch finds what was owed and imports it, once.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_owed_when_the_app_closed_is_imported_at_the_next_launch() {
    let fixture = Fixture::importing("auto-import-restart").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-restart", "rg-restart").await;
    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture.owe_import(&dir).await;
    let mut events = fixture.import.subscribe_events();

    // What the launch does: the first scan finishes, and the queue reads
    // everything afresh.
    fixture.rescan().await;

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
    assert!(fixture.owed_import(&dir).await.is_none());

    fixture.rescan().await;
    assert_no_import(&mut events, &key, "the owed import was paid").await;
}

/// What was owed goes with the setting: found while it is off, it is
/// withdrawn, and turning the setting on later does not bring it back.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_owed_while_the_setting_is_off_is_withdrawn() {
    let fixture = Fixture::new("auto-import-off").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-off", "rg-off").await;
    fixture.owe_import(&dir).await;
    let mut events = fixture.import.subscribe_events();

    fixture.rescan().await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.owed_import(&dir).await.is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the owed import is withdrawn while the setting is off");

    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture.rescan().await;
    assert_no_import(&mut events, &key, "the setting came back on").await;
}

/// A person's edit after the verdict is theirs: the draft the import was owed
/// for is gone, and so is what was owed.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_after_the_verdict_leaves_nothing_owed() {
    let fixture = Fixture::new("auto-import-edited").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-edited", "rg-edited").await;
    fixture.owe_import(&dir).await;
    let mut events = fixture.import.subscribe_events();

    fixture
        .import
        .set_candidate_edit_field(
            &key,
            crate::import::DraftFieldEdit::Text {
                field: crate::import::CandidateEditField::AlbumTitle,
                value: "Album (Edited)".into(),
            },
        )
        .await
        .unwrap();
    assert!(fixture.owed_import(&dir).await.is_none());

    fixture.manager.set_import_when_identified(true).await.unwrap();
    fixture.rescan().await;
    assert_no_import(&mut events, &key, "the person edited the draft").await;
}
