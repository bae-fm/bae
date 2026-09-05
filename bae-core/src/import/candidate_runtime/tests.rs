use super::*;
use crate::import::folder_scanner::{CategorizedFiles, InvalidCandidate, InvalidReason};
use crate::import::types::{ImportPhase, ImportProgress, PrepareStep};
use crate::util::rate_limiter::CallPriority;
use std::path::PathBuf;

fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles { files: Vec::new() }
}

fn folder_candidate(path: &str, watched: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: format!("Candidate {path}"),
        files: empty_categorized(),
        watched_folder_path: watched.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn scanned(candidate: FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate,
        skipped: false,
        is_added: false,
    })
}

fn progress(key: &str, percent: u8) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: Some(percent),
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    }
}

fn signals_context(track_count: u32) -> crate::identify::state::SignalsContext {
    crate::identify::state::SignalsContext {
        providers: Vec::new(),
        disc_id: crate::signals::DiscIdSignal::Absent { track_count },
        barcode_codes: Vec::new(),
        had_barcode_source: false,
        catalogs: Vec::new(),
        chosen_catalog: None,
        disc_excluded: false,
        barcode_excluded: false,
        discid_results: Vec::new(),
        barcode_results: Vec::new(),
        catalog_results: Vec::new(),
        discid_failure: None,
        barcode_failures: Vec::new(),
        barcode_scan_failure: None,
        catalog_failures: Vec::new(),
        matched_barcode: None,
        track_count,
    }
}

fn identify(key: &str, state: crate::identify::IdentifyState) -> ImportEvent {
    ImportEvent::IdentifyStateChanged {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(0),
        state,
        priority: CallPriority::Background,
    }
}

fn drain(changes: &mut broadcast::Receiver<CandidateRuntimeChange>) -> Vec<CandidateRuntimeChange> {
    let mut out = Vec::new();
    while let Ok(change) = changes.try_recv() {
        out.push(change);
    }
    out
}

fn extracted_signals() -> crate::signals::Signals {
    crate::signals::Signals {
        disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
        barcode: crate::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: crate::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        durations: crate::import::probe::SourceDurations::totalling(1_000),
    }
}

#[test]
fn import_progress_is_recorded_per_key_and_published_for_that_key_only() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";

    runtime.record_event(&progress(key, 42));

    let in_flight = runtime
        .get(key)
        .and_then(|runtime| runtime.import)
        .expect("the running import is recorded");
    assert_eq!(
        in_flight,
        ImportInFlight {
            progress_percent: Some(42),
            step: Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
        }
    );
    let published = drain(&mut changes);
    assert_eq!(published.len(), 1);
    assert!(matches!(
        &published[0],
        CandidateRuntimeChange::Updated { key: changed, runtime }
            if changed == key && runtime.import == Some(in_flight.clone())
    ));

    runtime.record_event(&progress("/watch/a/rel2", 3));
    assert!(matches!(
        drain(&mut changes).as_slice(),
        [CandidateRuntimeChange::Updated { key: changed, .. }] if changed == "/watch/a/rel2"
    ));
}

#[test]
fn a_claim_is_the_queued_step_until_the_worker_reports() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";

    runtime.claim_for_import(key);
    assert_eq!(
        runtime.get(key).and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            progress_percent: None,
            step: Some(ImportStep::Preparing(PrepareStep::Queued)),
        })
    );
    drain(&mut changes);

    runtime.release_import_claim(key);
    assert!(runtime.get(key).is_none());
    assert_eq!(
        drain(&mut changes),
        vec![CandidateRuntimeChange::Removed {
            key: key.to_string()
        }],
        "with nothing else running, releasing the claim empties the key"
    );
}

/// Every way an import ends is a row by the time the event reaches here — the
/// release the worker committed, or the failure it wrote — so the key stops
/// being in flight.
#[test]
fn a_finished_import_leaves_the_map() {
    let key = "/watch/a/rel1";
    let endings = [
        ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
        ImportProgress::RemoteUploadQueued {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
            outbox_revision: 7,
        },
        ImportProgress::Failed {
            error: "no space left".to_string(),
            import_id: "imp-1".to_string(),
        },
    ];
    for ending in endings {
        let runtime = CandidateRuntime::default();
        let mut changes = runtime.subscribe();
        runtime.record_event(&progress(key, 42));
        drain(&mut changes);

        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.to_string(),
            progress: ending.clone(),
        });
        assert!(
            runtime.get(key).is_none(),
            "{ending:?} left an entry behind"
        );
        assert_eq!(
            drain(&mut changes),
            vec![CandidateRuntimeChange::Removed {
                key: key.to_string()
            }]
        );
    }
}

/// A held terminal state keeps the key: only the import half ends.
#[test]
fn a_finished_import_keeps_a_key_whose_run_is_still_held() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::ManualOnly {
            track_count: 9,
            context: signals_context(9),
        },
    ));
    runtime.record_event(&progress(key, 42));
    drain(&mut changes);

    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
    });
    let recorded = runtime.get(key).expect("the held run keeps the key");
    assert!(recorded.import.is_none());
    assert!(matches!(
        recorded
            .identify
            .and_then(CandidateIdentifyRuntime::into_state),
        Some(crate::identify::IdentifyState::ManualOnly { .. })
    ));
    assert!(matches!(
        drain(&mut changes).as_slice(),
        [CandidateRuntimeChange::Updated { key: changed, .. }] if changed == key
    ));
}

/// The late subscriber is the whole reason the map exists: a row that appears
/// after an import started reads its state, and every later tick reaches it.
#[test]
fn a_late_subscriber_reads_every_running_key() {
    let runtime = CandidateRuntime::default();
    runtime.record_event(&progress("/watch/a/rel1", 10));
    runtime.claim_for_import("/watch/a/rel2");

    let mut changes = runtime.subscribe();
    let running = runtime.all();
    assert_eq!(running.len(), 2);
    assert_eq!(
        running["/watch/a/rel1"].import,
        Some(ImportInFlight {
            progress_percent: Some(10),
            step: Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
        })
    );
    assert!(running["/watch/a/rel2"].import.is_some());

    runtime.record_event(&progress("/watch/a/rel3", 5));
    assert!(matches!(
        drain(&mut changes).as_slice(),
        [CandidateRuntimeChange::Updated { key, .. }] if key == "/watch/a/rel3"
    ));
}

#[test]
fn a_preparing_step_has_no_progress_fraction() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";

    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Preparing {
            import_id: "imp-1".to_string(),
            step: PrepareStep::ValidatingSourceFiles,
            album_title: String::new(),
            artist_name: String::new(),
        },
    });

    assert_eq!(
        runtime
            .get(key)
            .and_then(|runtime| runtime.import)
            .expect("preparation is in flight")
            .progress_percent,
        None
    );
}

#[test]
fn the_automatic_queue_is_published_as_one_current_runtime_snapshot() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";

    runtime.replace_automatic_identification_queue([first.to_string(), second.to_string()]);

    let queued = runtime.all();
    assert_eq!(queued.len(), 2);
    for key in [first, second] {
        assert_eq!(
            crate::import::triage::TriageRuntimeFacts::of(&queued[key]).identification,
            Some(crate::import::IdentificationStatus::Queued)
        );
    }
    assert!(matches!(
        drain(&mut changes).as_slice(),
        [CandidateRuntimeChange::Reset { runtimes }] if runtimes == &queued
    ));

    runtime.replace_automatic_identification_queue(std::iter::empty());

    assert!(runtime.all().is_empty());
    assert!(matches!(
        drain(&mut changes).as_slice(),
        [CandidateRuntimeChange::Reset { runtimes }] if runtimes.is_empty()
    ));
}

#[test]
fn runtime_recorded_before_the_scan_survives_the_scan_reporting_the_key() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&progress(key, 42));

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));

    assert!(runtime
        .get(key)
        .is_some_and(|runtime| runtime.import.is_some()));
}

#[test]
fn a_rescan_reporting_the_same_shape_keeps_the_runtime_and_a_new_shape_drops_it() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    runtime.record_event(&progress(key, 42));
    drain(&mut changes);

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    assert!(
        runtime.get(key).is_some(),
        "an unchanged re-scan keeps the runtime"
    );
    assert!(drain(&mut changes).is_empty());

    let mut reshaped = folder_candidate(key, "/watch/a");
    reshaped.file_edit_revision = 1;
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
        candidate: reshaped,
    }));
    assert!(
        runtime.get(key).is_none(),
        "a reshaped candidate's runtime is gone"
    );
    assert_eq!(
        drain(&mut changes),
        vec![CandidateRuntimeChange::Removed {
            key: key.to_string()
        }]
    );
}

#[test]
fn removal_and_invalidation_drop_the_runtime() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    runtime.record_event(&progress("/watch/a/rel1", 1));
    runtime.record_event(&progress("/watch/a/rel2", 1));
    drain(&mut changes);

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: "/watch/a/rel1".to_string(),
    }));
    runtime.record_event(&ImportEvent::Scan(ScanEvent::InvalidCandidate(
        InvalidCandidate {
            path: PathBuf::from("/watch/a/rel2"),
            name: "rel2".to_string(),
            watched_folder_path: "/watch/a".to_string(),
            display_path: "rel2".to_string(),
            resolved_boundaries: Vec::new(),
            reason: InvalidReason::NoValidAudio,
        },
    )));

    assert!(runtime.all().is_empty());
    assert_eq!(drain(&mut changes).len(), 2);
    // Removing a key with no runtime publishes nothing.
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: "/watch/a/rel1".to_string(),
    }));
    assert!(drain(&mut changes).is_empty());
}

/// Extraction's snapshots are kept for the one form that reads them, but they
/// are not what is in flight: no entry appears for them and nobody watching a
/// row is woken. The form is fed by the UI bus instead.
#[test]
fn extracted_signals_are_retained_without_publishing_a_runtime() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";

    runtime.record_event(&ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        signals: extracted_signals(),
        priority: CallPriority::Background,
    });

    assert!(
        runtime.signals(key).is_some(),
        "the form can read them back"
    );
    assert!(
        runtime.get(key).is_none(),
        "nothing is in flight for the key"
    );
    assert!(drain(&mut changes).is_empty());

    // They describe this key's current files, so the scan dropping the key
    // drops them with it.
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: key.to_string(),
    }));
    assert!(runtime.signals(key).is_none());
}

/// The queue's own count belongs to the header, not to any one candidate, so it
/// never reaches here.
#[test]
fn queue_progress_never_touches_the_runtime() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    runtime.record_event(&ImportEvent::QueueIdentifyProgress {
        identified: 1,
        total: 9,
    });
    assert!(runtime.all().is_empty());
    assert!(drain(&mut changes).is_empty());
}

#[test]
fn a_stored_verdict_clears_the_recorded_terminal_state() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::ManualOnly {
            track_count: 9,
            context: signals_context(9),
        },
    ));

    // A driver torn down after settling broadcasts `Idle`; the answer stays.
    runtime.record_event(&identify(key, crate::identify::IdentifyState::Idle));
    assert!(matches!(
        runtime
            .get(key)
            .and_then(|runtime| runtime.identify)
            .and_then(CandidateIdentifyRuntime::into_state),
        Some(crate::identify::IdentifyState::ManualOnly { .. })
    ));
    drain(&mut changes);

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
        candidate_key: key.to_string(),
    }));
    assert!(
        runtime.get(key).is_none(),
        "the stored verdict owns the answer now, and nothing else was running"
    );
    assert_eq!(
        drain(&mut changes),
        vec![CandidateRuntimeChange::Removed {
            key: key.to_string()
        }]
    );

    // A newer run is in flight when the previous run's write lands: its
    // state is not terminal, so it stays.
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::Triangulating {
            discid: crate::identify::DiscidProgress::Computing,
            barcode: crate::identify::BarcodeProgress::Scanning,
            catalog: crate::identify::CatalogProgress::Skipped,
            context: signals_context(9),
        },
    ));
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
        candidate_key: key.to_string(),
    }));
    assert!(matches!(
        runtime
            .get(key)
            .and_then(|runtime| runtime.identify)
            .and_then(CandidateIdentifyRuntime::into_state),
        Some(crate::identify::IdentifyState::Triangulating { .. })
    ));

    // A genuine mid-run cancel empties the key.
    runtime.record_event(&identify(key, crate::identify::IdentifyState::Idle));
    assert!(runtime.get(key).is_none());
}

#[test]
fn a_failed_finalization_replaces_progress_without_discarding_the_result() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::ManualOnly {
            track_count: 9,
            context: signals_context(9),
        },
    ));

    runtime.fail_identification(key, "database write failed".to_string());

    let recorded = runtime.get(key).expect("the failure remains visible");
    assert!(matches!(
        recorded
            .identify
            .as_ref()
            .and_then(CandidateIdentifyRuntime::state),
        Some(crate::identify::IdentifyState::ManualOnly { .. })
    ));
    assert_eq!(
        crate::import::triage::TriageRuntimeFacts::of(&recorded).identification,
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            error: "database write failed".to_string(),
        })
    );

    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::ManualOnly {
            track_count: 9,
            context: signals_context(9),
        },
    ));
    assert!(matches!(
        crate::import::triage::TriageRuntimeFacts::of(
            &runtime.get(key).expect("the failure remains visible")
        )
        .identification,
        Some(crate::import::IdentificationStatus::FinalizationFailed { .. })
    ));
}

/// A claimed import outlives the run's write: the key keeps the half that is
/// still happening.
#[test]
fn a_stored_verdict_keeps_a_key_whose_import_is_running() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::ManualOnly {
            track_count: 9,
            context: signals_context(9),
        },
    ));
    runtime.claim_for_import(key);

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
        candidate_key: key.to_string(),
    }));
    let recorded = runtime.get(key).expect("the claim keeps the key");
    assert!(recorded.identify.is_none());
    assert!(recorded.import.is_some());
}
