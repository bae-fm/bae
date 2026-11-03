//! What each run published, and the signals it read, stored beside the
//! standings while the run stands.

use super::*;
use crate::candidate_runtime::tests::*;
use crate::candidates::{Admission, IdentificationRuntime};
use bae_catalogs::catalog::LookupCatalog;

// What each run published, as `import_identify_run` stores it.

/// What `key`'s runs published, as stored.
async fn stored_runs(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
) -> std::collections::BTreeMap<crate::IdentifyRunId, bae_identify::PublishedRun> {
    runtime.tables.load_candidate_runtime(key).await.expect("a test reads the runs table").runs
}

/// A folder candidate's run has its row only while the standing names it:
/// the write that moves the standing off the run deletes it, and a publish
/// after that stores nothing.
#[tokio::test]
async fn a_run_s_published_state_goes_in_the_write_that_moves_its_standing_off_it() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    assert_eq!(
        stored_runs(&runtime, key).await,
        std::collections::BTreeMap::from([(run(1), published(manual_only()))]),
        "the answer being written keeps what its run published"
    );

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    assert!(
        stored_runs(&runtime, key).await.is_empty(),
        "waiting for the next run, the standing names none"
    );
    assert!(!runtime.publish_candidate_run(key, run(1), &triangulating(), None).await.unwrap());
    assert!(stored_runs(&runtime, key).await.is_empty());

    report(&runtime, key, &files, run(2), triangulating()).await;
    assert_eq!(
        stored_runs(&runtime, key).await,
        std::collections::BTreeMap::from([(run(2), published(triangulating()))])
    );
    runtime.run_ended(key, run(2)).await.unwrap();
    assert!(stored_runs(&runtime, key).await.is_empty());
}

/// No run outlives the process: opening the library empties the runs table,
/// a library release's runs included.
#[tokio::test]
async fn opening_the_library_empties_every_run_s_published_state() {
    let runtime = CandidateRuntime::for_test().await;
    let release = &crate::CandidateKey::reidentify("release-1");
    runtime.publish_release_run(release, run(1), &triangulating(), None).await.unwrap();
    assert!(!stored_runs(&runtime, release).await.is_empty());

    runtime.tables.clear_candidate_standings().await.unwrap();
    assert!(stored_runs(&runtime, release).await.is_empty());
}

/// A library release's run that ends goes; another run of it stays.
#[tokio::test]
async fn a_release_run_that_ends_goes_alone() {
    let runtime = CandidateRuntime::for_test().await;
    let release = &crate::CandidateKey::reidentify("release-1");
    runtime.publish_release_run(release, run(1), &manual_only(), None).await.unwrap();
    runtime.publish_release_run(release, run(2), &triangulating(), None).await.unwrap();
    runtime
        .publish_release_run(release, run(1), &bae_identify::IdentifyState::Idle, None)
        .await
        .unwrap();
    assert_eq!(
        stored_runs(&runtime, release).await,
        std::collections::BTreeMap::from([(run(2), published(triangulating()))])
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
    let runtime = CandidateRuntime::for_test().await;
    let release = &crate::CandidateKey::reidentify("release-1");
    let signals = scanning_signals();
    let state = barcode_half_answered();
    runtime.publish_release_run(release, run(1), &state, Some(&signals)).await.unwrap();
    let expected = bae_identify::PublishedRun {
        outcome: bae_identify::RunOutcome::of(&state).unwrap(),
        signals: Some(signals.clone()),
    };
    let bae_identify::RunOutcome::Running { findings, .. } = &expected.outcome else {
        panic!("a run partway through is running");
    };
    assert_eq!(findings.matches.len(), 1, "the half-answered barcode found one release");
    assert_eq!(expected.rate_ruling_out_cd(), Some(96_000));
    assert_eq!(
        stored_runs(&runtime, release).await,
        std::collections::BTreeMap::from([(run(1), expected.clone())])
    );

    // A later state without new signals keeps the ones the run read.
    runtime.publish_release_run(release, run(1), &search_waiting(), None).await.unwrap();
    assert_eq!(
        stored_runs(&runtime, release).await,
        std::collections::BTreeMap::from([(
            run(1),
            bae_identify::PublishedRun {
                outcome: bae_identify::RunOutcome::of(&search_waiting()).unwrap(),
                signals: Some(signals.clone()),
            }
        )])
    );
}

/// A folder candidate's run shows the signals it last read, and its settle
/// reads that run's own: another run of the key reads none, and once the
/// standing moves off the run its signals are gone with it.
#[tokio::test]
async fn a_run_s_signals_are_its_own_and_go_with_it() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(runtime.run_signals(key, run(1)).await.unwrap(), None);

    let scanning = scanning_signals();
    assert!(runtime
        .publish_candidate_run(key, run(1), &triangulating(), Some(&scanning))
        .await
        .unwrap());
    assert_eq!(runtime.run_signals(key, run(1)).await.unwrap(), Some(scanning.clone()));
    assert_eq!(runtime.run_signals(key, run(2)).await.unwrap(), None);
    let shown = |snapshot: Option<crate::CandidateRuntimeSnapshot>| match snapshot
        .and_then(|snapshot| snapshot.identification)
    {
        Some(IdentificationRuntime::Running(published)) => published.signals,
        Some(IdentificationRuntime::Saving(answer)) => answer.state.signals,
        other => panic!("the key is identifying, got {other:?}"),
    };
    assert_eq!(shown(runtime.snapshot(key).await.unwrap()), Some(scanning));

    let settled = extracted_signals();
    assert!(runtime
        .publish_candidate_run(key, run(1), &manual_only(), Some(&settled))
        .await
        .unwrap());
    assert!(runtime.run_answered(key, run(1), false).await.unwrap());
    assert_eq!(
        runtime.run_signals(key, run(1)).await.unwrap(),
        Some(settled.clone()),
        "the answer being written reads the signals it was judged against"
    );
    assert_eq!(shown(runtime.snapshot(key).await.unwrap()), Some(settled));

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    assert_eq!(runtime.run_signals(key, run(1)).await.unwrap(), None);
}

/// Every row a run owns goes with the run: its verdict-shaped rows, its
/// matches, its ledger and its signals.
#[tokio::test]
async fn a_run_s_rows_go_with_it() {
    let runtime = CandidateRuntime::for_test().await;
    let release = &crate::CandidateKey::reidentify("release-1");
    runtime
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
            runtime.tables.count_rows_for_test(table, "identify_run", "1").await.unwrap() > 0,
            "the run holds rows of {table}"
        );
    }
    runtime
        .publish_release_run(release, run(1), &bae_identify::IdentifyState::Idle, None)
        .await
        .unwrap();
    for table in owned {
        assert_eq!(
            runtime.tables.count_rows_for_test(table, "identify_run", "1").await.unwrap(),
            0,
            "{table} keeps rows of a run that ended"
        );
    }
}
