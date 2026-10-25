// ── An answer whose pick is being read ──────────────────────────────────────

/// A found answer over `pressings` (each release's row) whose lead lists two
/// tracks, for a folder of two.
fn found_over(pressings: Vec<u32>) -> crate::identify::IdentifyState {
    let matches = (0..pressings.len())
        .map(|index| crate::import::search::MetadataResult {
            source_tracks: Some(crate::import::search::SourceTracks::Listed { count: 2 }),
            ..crate::import::search::MetadataResult::for_test(
                Catalog::MusicBrainz,
                &format!("mb-{index}"),
                Some("rg-1"),
            )
        })
        .collect();
    crate::identify::IdentifyState::Found {
        findings: crate::identify::Findings {
            matches,
            pressings,
            ..Default::default()
        },
        library_statuses: Default::default(),
        track_count: 2,
        ledger: None,
        context: signals_context(2),
    }
}

/// An answer that picks its one pressing unattended reads as its pick being
/// read, and an Import asked of it rides on it until the answer is disposed
/// of, which hands the ask back.
#[test]
fn an_answer_that_picks_reads_as_its_pick_being_read_and_carries_an_import_ask() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, found_over(vec![0])));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::ReadingPick {
            import_asked: false
        })
    );

    assert!(runtime.ask_import_once_stored(key));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::ReadingPick { import_asked: true })
    );
    assert!(
        runtime.ask_import_once_stored(key),
        "asking again asks the same thing"
    );

    assert!(
        runtime.end_identification_answer(key, run(1)),
        "the write that ends the answer is handed the ask"
    );
    assert_eq!(identification(&runtime, key), None);
    assert!(
        !runtime.ask_import_once_stored(key),
        "with no answer held there is nothing to ask of"
    );
}

/// An answer that picks nothing is written as today: it reads as finalizing,
/// and an Import is not asked of it.
#[test]
fn an_answer_that_picks_nothing_reads_as_finalizing_and_takes_no_ask() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, found_over(vec![0, 1])));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Finalizing),
        "two pressings are the person's to pick from"
    );

    assert!(!runtime.ask_import_once_stored(key));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Finalizing)
    );
    assert!(!runtime.end_identification_answer(key, run(1)));
}

/// The ask is the answer's: a newer run's answer starts without it, an older
/// run's ending hands back nothing, and a failed write drops it.
#[test]
fn an_import_ask_goes_with_the_answer_it_was_asked_of() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, found_over(vec![0])));
    assert!(runtime.ask_import_once_stored(key));

    runtime.record_event(&identify(key, 2, found_over(vec![0])));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::ReadingPick {
            import_asked: false
        }),
        "the newer run's answer was asked nothing"
    );
    assert!(
        !runtime.end_identification_answer(key, run(1)),
        "the older run's answer is not the one held"
    );

    assert!(runtime.ask_import_once_stored(key));
    runtime.fail_identification(key, run(2), not_written("database write failed"));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            failure: not_written("database write failed"),
        })
    );
    assert!(!runtime.end_identification_answer(key, run(2)));
}
