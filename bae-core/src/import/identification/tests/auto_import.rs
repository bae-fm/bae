// ── Importing when identified ───────────────────────────────────────────────
//
// A run that stores an auto-importable verdict, while "Import automatically
// when identified" is on, starts its candidate's import right then — the same
// start a person's Import press makes — whether the run started on its own or
// a person asked for it. A release a person picks themselves is theirs to
// import. Nothing else imports on its own.

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

    /// An automatic run's auto-importable verdict for `dir`, stored while
    /// importing when identified is off.
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
    fixture.provider.route("/discid/", 200, answer.to_string());
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
        0,
        "the offered row was read from its Disc ID answer, and nothing picked it"
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

/// Identify or a retry is a run like an automatic one: what it picks
/// unattended is imported, whether or not identification also runs on its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_imports_its_candidate_with_identifying_automatically_off() {
    let fixture = Fixture::importing("auto-import-requested").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-requested", "rg-requested", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);

    let (imports, failure) = await_import_ending(&mut events, &key).await;
    assert_eq!(imports.len(), 1, "one import of the candidate: {imports:?}");
    assert_eq!(failure, None, "the import completes");
}

/// With the setting off, what a person's run picks unattended waits for them.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_a_person_asked_for_with_the_setting_off_imports_nothing() {
    let fixture = Fixture::new("auto-import-requested-setting-off").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-requested-off", "rg-requested-off", 2);
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&dir);
    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.identification().drained_for_test(),
    )
    .await
    .expect("the person's run ends");

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None)
    );
    assert_no_import(&fixture, &mut events, &key, "the person's run settled with the setting off").await;
}

/// A release a person picks from their typed search is their choice, and
/// theirs to import, whatever the setting says.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_picked_from_a_typed_search_is_not_imported() {
    let fixture = Fixture::new("auto-import-typed-search").await;
    fixture.manager.set_identify_automatically(false).await.unwrap();
    fixture.manager.set_import_when_identified(true).await.unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .route("/release?", 200, search_json("mb-typed", "rg-typed"));
    fixture.provider.route(
        "/release/mb-typed?",
        200,
        release_json("mb-typed", "rg-typed", &[probed, 0]),
    );
    fixture.scan(1).await;
    // The queue is running while the person searches and picks.
    fixture.identification();
    let mut events = fixture.import.every_event();

    fixture.import.start_candidate_search(
        key.clone(),
        crate::import::search::SearchQuery::General {
            artist: "Artist".to_string(),
            album: "Album".to_string(),
        },
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let landed = fixture
                .import
                .candidate_runtime(&key).await.unwrap()
                .and_then(|runtime| runtime.search)
                .is_some_and(|search| {
                    matches!(
                        search.source(bae_catalogs::catalog::Catalog::MusicBrainz),
                        Some(crate::import::SourceSearch::Done { .. })
                    )
                });
            if landed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the typed search lands its MusicBrainz results");
    fixture
        .import
        .select_candidate_release(key.clone(), crate::import::PressingLink {
            record: crate::import::MetadataRef::new(
                bae_catalogs::catalog::Catalog::MusicBrainz,
                "mb-typed".to_string(),
            ),
            partners: vec![],
        })
        .await
        .expect("the person's pick applies");
    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.identification().drained_for_test(),
    )
    .await
    .expect("the queue has nothing left to do");

    assert_no_import(&fixture, &mut events, &key, "the person picked from their typed search").await;
}

/// An automatic start the import path refuses is the candidate's failed
/// import, shown where one that failed while running is, with nothing claimed.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_that_cannot_start_is_recorded_as_its_failed_import() {
    let fixture = Fixture::new("auto-import-refused").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.identify_ready(&dir, "mb-refused", "rg-refused").await;
    // A remote cover whose stored bytes are gone is a broken candidate no
    // import starts from.
    let hash = fixture.content_hash(&dir);
    fixture
        .preparations
        .set_cover(
            &hash,
            &crate::import::CoverSelection::Remote(
                bae_catalogs::remote_image::RemoteImageSet::original(
                    "https://example.invalid/cover".to_string(),
                ),
                bae_catalogs::catalog::Catalog::MusicBrainz,
            ),
            Some(&bae_catalogs::remote_image::RemoteImage {
                bytes: vec![1, 2, 3],
                content_type: bae_util::content_type::ContentType::Jpeg,
            }),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute_for_test(
            "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
            vec![hash],
        )
        .await
        .unwrap();

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a remote cover without its bytes does not start an import");

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
            .candidate_runtime(&key).await.unwrap()
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
