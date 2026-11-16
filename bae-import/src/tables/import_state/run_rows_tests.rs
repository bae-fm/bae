//! What each run published, and the signals it read, stored beside the
//! standings while the run stands.

use super::*;
use crate::candidate_runtime::tests::{
    extracted_signals, manual_only, published, run, signals_context, triangulating,
};
use crate::tables::import_state::tests::{pane_candidate_key, stored_pane_candidate};
use crate::Admission;
use bae_catalogs::catalog::LookupCatalog;
use bae_store::test_fixtures::empty_db;
use std::collections::BTreeMap;

/// Tables over a library whose scan holds one folder candidate, its key and
/// the files it is identified as.
async fn scanned() -> (ImportTables, CandidateKey, CandidateFiles, tempfile::TempDir) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, content_hash) = stored_pane_candidate(&db).await;
    let files = CandidateFiles { content_hash, file_edit_revision: 0 };
    (ImportTables::new(db), pane_candidate_key(), files, tmp)
}

/// Tables over an empty library.
async fn library() -> (ImportTables, tempfile::TempDir) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    (ImportTables::new(db), tmp)
}

/// What the run `key`'s runtime shows published, as stored, by its run.
async fn stored_runs(
    tables: &ImportTables,
    key: &CandidateKey,
) -> BTreeMap<IdentifyRunId, PublishedRun> {
    tables
        .load_candidate_runtime(key)
        .await
        .expect("a test reads the runs table")
        .run
        .into_iter()
        .collect()
}

async fn change(tables: &ImportTables, key: &CandidateKey, change: IdentificationChange) {
    tables.change_candidate_identifications(vec![(key.clone(), change)]).await.unwrap();
}

async fn admit(
    tables: &ImportTables,
    key: &CandidateKey,
    files: &CandidateFiles,
    admission: Admission,
) {
    change(tables, key, IdentificationChange::Admit { files: files.clone(), admission }).await;
}

/// Start `run` of `key` with `state`, and where `state` is its answer, move
/// the standing to it being written, as the identify driver reports.
async fn report(
    tables: &ImportTables,
    key: &CandidateKey,
    files: &CandidateFiles,
    run: IdentifyRunId,
    state: IdentifyState,
) {
    tables.start_candidate_run(key, files, run, &state).await.unwrap();
    if state.is_terminal() {
        let reads_pick = state.picks_unattended();
        change(tables, key, IdentificationChange::RunAnswered { run, reads_pick }).await;
    }
}

/// A folder candidate's run has its row only while the standing names it:
/// the write that moves the standing off the run deletes it, and a publish
/// after that stores nothing.
#[tokio::test]
async fn a_run_s_published_state_goes_in_the_write_that_moves_its_standing_off_it() {
    let (tables, key, files, _tmp) = scanned().await;
    let key = &key;
    admit(&tables, key, &files, Admission::Automatic).await;
    report(&tables, key, &files, run(1), manual_only()).await;
    assert_eq!(
        stored_runs(&tables, key).await,
        BTreeMap::from([(run(1), published(manual_only()))]),
        "the answer being written keeps what its run published"
    );

    admit(&tables, key, &files, Admission::Requested).await;
    assert!(
        stored_runs(&tables, key).await.is_empty(),
        "waiting for the next run, the standing names none"
    );
    assert!(!tables.publish_candidate_run(key, run(1), &triangulating(), None).await.unwrap());
    assert!(stored_runs(&tables, key).await.is_empty());

    report(&tables, key, &files, run(2), triangulating()).await;
    assert_eq!(
        stored_runs(&tables, key).await,
        BTreeMap::from([(run(2), published(triangulating()))])
    );
    change(&tables, key, IdentificationChange::RunEnded { run: run(2) }).await;
    assert!(stored_runs(&tables, key).await.is_empty());
}

/// A run publishes no `Idle` while it runs, so a folder candidate's run
/// publishing one is a bug of bae's own: the write fails with the bug the
/// store reports.
#[tokio::test]
async fn a_folder_candidate_s_run_publishing_idle_is_a_bug() {
    for starting in [false, true] {
        let (db, _tmp, heard) =
            bae_store::test_fixtures::temp_db_hearing_bugs(crate::TEST_SCHEMA).await;
        let (_, content_hash) = stored_pane_candidate(&db).await;
        let files = CandidateFiles { content_hash, file_edit_revision: 0 };
        let tables = ImportTables::new(db);
        let key = pane_candidate_key();
        admit(&tables, &key, &files, Admission::Automatic).await;
        report(&tables, &key, &files, run(1), triangulating()).await;

        let failed = match starting {
            true => tables
                .start_candidate_run(&key, &files, run(2), &IdentifyState::Idle)
                .await
                .map(|_| ()),
            false => tables
                .publish_candidate_run(&key, run(1), &IdentifyState::Idle, None)
                .await
                .map(|_| ()),
        }
        .unwrap_err();

        let bug = crate::tables::tests::the_reported_bug(&heard, &failed);
        assert!(bug.contains("published no state"), "{bug}");
    }
}

/// No run outlives the process: opening the library empties the runs table,
/// a library release's runs included.
#[tokio::test]
async fn opening_the_library_empties_every_run_s_published_state() {
    let (tables, _tmp) = library().await;
    let release = &CandidateKey::reidentify("release-1");
    tables.publish_release_run(release, run(1), &triangulating(), None).await.unwrap();
    assert!(!stored_runs(&tables, release).await.is_empty());

    tables.clear_candidate_standings().await.unwrap();
    assert!(stored_runs(&tables, release).await.is_empty());
}

/// A library release's run that ends, publishing `Idle`, goes; another run
/// of it stays.
#[tokio::test]
async fn a_release_run_that_ends_goes_alone() {
    let (tables, _tmp) = library().await;
    let release = &CandidateKey::reidentify("release-1");
    tables.publish_release_run(release, run(1), &manual_only(), None).await.unwrap();
    tables.publish_release_run(release, run(2), &triangulating(), None).await.unwrap();
    tables.publish_release_run(release, run(1), &IdentifyState::Idle, None).await.unwrap();
    assert_eq!(
        stored_runs(&tables, release).await,
        BTreeMap::from([(run(2), published(triangulating()))])
    );
}

/// A run partway through: its disc ID and ISRCs still being read, the
/// artwork still being scanned, and its one barcode answered by one catalog
/// with a release while the other is still looking.
fn barcode_half_answered() -> bae_identify::IdentifyState {
    use bae_identify::{LookupState, ProviderLookup, ValueLookup};

    let mut context = signals_context(9);
    context.providers = vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs];
    context.artwork = bae_identify::signals::ArtworkScan::Reading {
        current: Some("back.jpg".to_string()),
        position: 1,
        total: 2,
    };
    context.barcode.codes = vec![bae_identify::signals::SourcedValue::in_file(
        "0123456789012".to_string(),
        "back.jpg".to_string(),
    )];
    context.barcode.had_source = true;
    bae_identify::IdentifyState::Triangulating {
        discid: bae_identify::DiscidProgress::Computing,
        barcode: bae_identify::BarcodeProgress::Lookups {
            codes: vec![ValueLookup {
                value: "0123456789012".to_string(),
                providers: vec![
                    ProviderLookup {
                        source: LookupCatalog::MusicBrainz,
                        state: LookupState::Done {
                            results: vec![bae_catalogs::search::MetadataResult::for_test(
                                LookupCatalog::MusicBrainz,
                                "mb-release",
                                Some("mb-group"),
                            )],
                        },
                    },
                    ProviderLookup {
                        source: LookupCatalog::Discogs,
                        state: LookupState::LookingUp,
                    },
                ],
            }],
        },
        catalog: bae_identify::CatalogProgress::Skipped,
        isrc: bae_identify::IsrcProgress::Reading,
        search: bae_identify::SearchProgress::Pending,
        context,
    }
}

/// A run whose clues named nothing, its title search waiting to go out.
fn search_waiting() -> bae_identify::IdentifyState {
    let mut context = signals_context(9);
    context.providers = vec![bae_catalogs::catalog::LookupCatalog::MusicBrainz];
    context.search.query = bae_identify::TitleSearch::of("Kind of Blue", "Miles Davis");
    bae_identify::IdentifyState::Triangulating {
        discid: bae_identify::DiscidProgress::Skipped,
        barcode: bae_identify::BarcodeProgress::Skipped,
        catalog: bae_identify::CatalogProgress::Skipped,
        isrc: bae_identify::IsrcProgress::Skipped,
        search: bae_identify::SearchProgress::Pending,
        context,
    }
}

/// What extraction read partway through: its artwork still being scanned,
/// with one barcode and one line of text so far.
fn scanning_signals() -> bae_identify::signals::Signals {
    bae_identify::signals::Signals {
        origin: bae_catalogs::AudioOrigin { source: None, not_cd_rate: Some(96_000) },
        disc_id: bae_identify::signals::DiscIdSignal::Absent,
        barcode: bae_identify::signals::BarcodeSignal::Scanning {
            codes: vec![bae_identify::signals::SourcedValue::in_file(
                "0123456789012".to_string(),
                "back.jpg".to_string(),
            )],
        },
        text: bae_identify::signals::TextSignal::Scanning {
            catalogs: vec!["CK 40579".to_string()],
            free_text: Vec::new(),
        },
        text_pool: vec![bae_identify::signals::TextLine {
            text: "Kind of Blue CK 40579".to_string(),
            origin: bae_identify::signals::TextOrigin::Artwork,
        }],
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// A run in flight is stored in the verdict's shape — its steps still being
/// read, its cells looking up, the release its answers so far found, its
/// search waiting — with the signals it last read, and reads back as it
/// published, judged against those signals.
#[tokio::test]
async fn a_run_in_flight_reads_back_as_it_published() {
    let (tables, _tmp) = library().await;
    let release = &CandidateKey::reidentify("release-1");
    let signals = scanning_signals();
    let state = barcode_half_answered();
    tables.publish_release_run(release, run(1), &state, Some(&signals)).await.unwrap();
    let outcome = bae_identify::RunOutcome::of(&state).unwrap();
    // The run's rows hold the cards that `reference_cards_of` builds from the
    // run's outcome and the text in its signals.
    let text = bae_identify::CandidateText::of(&signals.text_pool, &[], signals.barcode.codes());
    let expected = bae_identify::PublishedRun {
        cards: bae_identify::view::reference_cards_of(&outcome, &text),
        outcome,
        signals: Some(signals.clone()),
    };
    let bae_identify::RunOutcome::Running { findings, .. } = &expected.outcome else {
        panic!("a run partway through is running");
    };
    assert_eq!(findings.matches.len(), 1, "the half-answered barcode found one release");
    assert_eq!(expected.rate_ruling_out_cd(), Some(96_000));
    assert_eq!(stored_runs(&tables, release).await, BTreeMap::from([(run(1), expected.clone())]));

    // A later state without new signals keeps the ones the run read.
    tables.publish_release_run(release, run(1), &search_waiting(), None).await.unwrap();
    assert_eq!(
        stored_runs(&tables, release).await,
        BTreeMap::from([(
            run(1),
            bae_identify::PublishedRun {
                outcome: bae_identify::RunOutcome::of(&search_waiting()).unwrap(),
                signals: Some(signals.clone()),
                cards: Default::default(),
            }
        )])
    );
}

/// A folder candidate's run holds the signals it last read, and its settle
/// reads that run's own: another run of the key reads none, and once the
/// standing moves off the run its signals are gone with it.
#[tokio::test]
async fn a_run_s_signals_are_its_own_and_go_with_it() {
    let (tables, key, files, _tmp) = scanned().await;
    let key = &key;
    admit(&tables, key, &files, Admission::Automatic).await;
    report(&tables, key, &files, run(1), triangulating()).await;
    assert_eq!(tables.load_candidate_run_signals(key, run(1)).await.unwrap(), None);

    let scanning = scanning_signals();
    assert!(tables
        .publish_candidate_run(key, run(1), &triangulating(), Some(&scanning))
        .await
        .unwrap());
    assert_eq!(
        tables.load_candidate_run_signals(key, run(1)).await.unwrap(),
        Some(scanning.clone())
    );
    assert_eq!(tables.load_candidate_run_signals(key, run(2)).await.unwrap(), None);
    assert_eq!(stored_runs(&tables, key).await[&run(1)].signals, Some(scanning));

    let settled = extracted_signals();
    assert!(tables
        .publish_candidate_run(key, run(1), &manual_only(), Some(&settled))
        .await
        .unwrap());
    change(&tables, key, IdentificationChange::RunAnswered { run: run(1), reads_pick: false })
        .await;
    assert_eq!(
        tables.load_candidate_run_signals(key, run(1)).await.unwrap(),
        Some(settled.clone()),
        "the answer being written reads the signals it was judged against"
    );
    assert_eq!(stored_runs(&tables, key).await[&run(1)].signals, Some(settled));

    admit(&tables, key, &files, Admission::Requested).await;
    assert_eq!(tables.load_candidate_run_signals(key, run(1)).await.unwrap(), None);
}

/// Every row a run owns goes with the run: its verdict-shaped rows, its
/// matches, its ledger and its signals.
#[tokio::test]
async fn a_run_s_rows_go_with_it() {
    let (tables, _tmp) = library().await;
    let release = &CandidateKey::reidentify("release-1");
    tables
        .publish_release_run(release, run(1), &barcode_half_answered(), Some(&scanning_signals()))
        .await
        .unwrap();
    let owned = [
        "import_candidate_verdict",
        "import_candidate_match",
        "import_candidate_lookup_value",
        "import_candidate_lookup",
        "import_candidate_lookup_release",
        "import_candidate_lookup_provider",
        "import_candidate_signals",
        "import_candidate_signal_value",
        "import_candidate_text_line",
    ];
    for table in owned {
        assert!(
            tables.count_rows_for_test(table, "identify_run", "1").await.unwrap() > 0,
            "the run holds rows of {table}"
        );
    }
    tables.publish_release_run(release, run(1), &IdentifyState::Idle, None).await.unwrap();
    for table in owned {
        assert_eq!(
            tables.count_rows_for_test(table, "identify_run", "1").await.unwrap(),
            0,
            "{table} keeps rows of a run that ended"
        );
    }
}

/// A search of one query with every catalog asked, as a person types it.
fn typed_search() -> crate::candidate_search::CandidateSearch {
    let availability: Vec<_> = LookupCatalog::ALL
        .into_iter()
        .map(|catalog| bae_preferences::CatalogAvailability {
            catalog,
            state: bae_preferences::SourceAvailability::On,
        })
        .collect();
    crate::candidate_search::CandidateSearch::started(
        bae_catalogs::search::SearchQuery::General {
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
        },
        &availability,
    )
}

/// One query loads the runtime of every key at once: a folder candidate
/// holding only its standing and a search, a release holding two runs in
/// flight, which shows the later one, a release holding one, and a release
/// holding only a search; each under its own key, and the keys in key order.
#[tokio::test]
async fn every_key_read_together_holds_what_it_holds_alone() {
    let (tables, key, files, _tmp) = scanned().await;
    admit(&tables, &key, &files, Admission::Automatic).await;
    let first = CandidateKey::reidentify("release-1");
    let second = CandidateKey::reidentify("release-2");
    let third = CandidateKey::reidentify("release-3");
    tables.publish_release_run(&first, run(2), &search_waiting(), None).await.unwrap();
    tables
        .publish_release_run(&first, run(1), &barcode_half_answered(), Some(&scanning_signals()))
        .await
        .unwrap();
    tables.publish_release_run(&second, run(3), &search_waiting(), None).await.unwrap();
    tables.start_candidate_search(&key, &typed_search()).await.unwrap();
    tables.start_candidate_search(&third, &typed_search()).await.unwrap();

    let together = tables.load_candidate_runtimes().await.unwrap();
    let keys: Vec<_> = together.iter().map(|(held, _)| held.clone()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "the keys come in key order");
    assert_eq!(keys.len(), 4);
    let of = |key: &CandidateKey| together.iter().find(|(held, _)| held == key).map(|(_, r)| r);

    let folder = of(&key).unwrap();
    assert!(folder.run.is_none());
    assert!(folder.standing.identification.is_some());
    assert_eq!(folder.search.as_ref(), Some(&typed_search()));

    let two = of(&first).unwrap();
    assert_eq!(two.run, Some((run(2), published(search_waiting()))));
    assert!(two.search.is_none());

    let one = of(&second).unwrap();
    assert_eq!(one.run, Some((run(3), published(search_waiting()))));

    let search_only = of(&third).unwrap();
    assert!(search_only.run.is_none());
    assert!(search_only.standing.identification.is_none());
    assert_eq!(search_only.search.as_ref(), Some(&typed_search()));

    for key in &keys {
        assert_eq!(of(key).unwrap(), &tables.load_candidate_runtime(key).await.unwrap());
    }
    assert_eq!(
        tables.load_candidate_runtime(&CandidateKey::reidentify("nothing")).await.unwrap(),
        StoredRuntime::default(),
        "loading a key with nothing running returns an empty runtime"
    );
}
