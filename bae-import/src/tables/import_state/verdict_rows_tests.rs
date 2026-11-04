//! A terminal verdict as stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;
use bae_identify::{Findings, NarrowedOut, TerminalVerdict};

/// A stored verdict reads back exactly, its ledger included.
#[tokio::test]
async fn round_trip_preserves_the_verdict_and_its_ledger() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = sample_verdict();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let loaded_row = loaded.get(&hash).expect("row present under its content hash");
    assert_eq!(
        loaded_row.candidate_key,
        crate::CandidateKey::folder(&host_root("/music/Some Album"))
    );
    let identify =
        loaded_row.identify.as_ref().expect("a stored verdict reads back as an identify result");
    assert_eq!(
        identify.verdict, verdict,
        "the verdict must round-trip exactly, its ledger included"
    );
}

/// A failed verdict reads back with what its answering lookups found.
#[tokio::test]
async fn a_failed_verdict_round_trips_what_its_answering_lookups_found() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = TerminalVerdict::Failed {
        failures: vec![bae_identify::IdentifyFailure::Search(
            bae_catalogs::search::SourceFailure {
                source: bae_catalogs::catalog::Catalog::MusicBrainz,
                failure: bae_catalogs::LookupFailure::Provider { status: Some(503) },
            },
        )],
        findings: sample_findings(),
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// A run that ended because bae broke reads back with why.
#[tokio::test]
async fn an_error_verdict_round_trips_its_failure() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let failure = bae_catalogs::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    let verdict = TerminalVerdict::Error { failure };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// A verdict recorded with no ledger reads back with none.
#[tokio::test]
async fn a_verdict_with_no_ledger_reads_back_without_one() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict =
        TerminalVerdict::Found { findings: sample_findings(), track_count: 11, ledger: None };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// Narrowed-out releases read back apart from the matches.
#[tokio::test]
async fn a_verdict_round_trips_its_narrowed_out_releases_apart_from_its_matches() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut left_out = sample_match();
    left_out.release_id = "rel-narrowed".to_string();
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            narrowed_out: NarrowedOut { matches: vec![left_out], pressings: vec![0] },
            ..sample_findings()
        },
        track_count: 11,
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
    let TerminalVerdict::Found { findings: Findings { matches, narrowed_out, .. }, .. } =
        &identify.verdict
    else {
        panic!("a found verdict reads back as one");
    };
    assert_eq!(
        matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["rel-1"]
    );
    assert_eq!(
        narrowed_out.matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["rel-narrowed"]
    );
}

/// A transport failure is stored as a failed verdict.
#[tokio::test]
async fn a_transport_failure_round_trips_as_a_failed_verdict() {
    use bae_catalogs::LookupFailure;
    use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
    use bae_identify::state::step as identify_step;
    use bae_identify::{IdentifyEvent, IdentifyState};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    store_candidate_state(&db, &candidate, &host_root("/music/Some Album")).await;

    let (state, _) = identify_step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers: vec![bae_catalogs::catalog::LookupCatalog::MusicBrainz],
            choices: bae_identify::LookupChoices::default(),
            title_search: None,
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: DiscIdSignal::Computed {
                    discs: vec![bae_identify::signals::ComputedDiscId {
                        disc_id: "disc-hash".to_string(),
                        source_file: None,
                    }],
                },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled { catalogs: vec![], free_text: vec![] },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            audio: bae_identify::signals::AudioFacts::default(),
            artwork: bae_identify::signals::ArtworkScan::Absent,
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-hash".into(),
            outcome: Err(LookupFailure::Provider { status: Some(503) }),
        },
    );

    let verdict = TerminalVerdict::try_from(state).expect("the failure is terminal");
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let loaded = loaded
        .get(&hash)
        .and_then(|state| state.identify.as_ref())
        .expect("the failed verdict is stored");
    assert_eq!(loaded.verdict, verdict, "the provider failure survives the database boundary");
}
