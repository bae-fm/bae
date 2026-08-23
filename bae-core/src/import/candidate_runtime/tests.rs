use super::*;
use crate::import::folder_scanner::{CategorizedFiles, InvalidCandidate, InvalidReason};
use crate::import::types::{ImportPhase, ImportProgress, PrepareStep};
use crate::util::rate_limiter::CallPriority;
use std::path::PathBuf;

fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        files: Vec::new(),
        format_label: "FLAC".to_string(),
    }
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
            percent,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    }
}

fn signals_context(track_count: u32) -> crate::identify::state::SignalsContext {
    crate::identify::state::SignalsContext {
        disc_id: crate::signals::DiscIdSignal::Absent { track_count },
        barcode_codes: Vec::new(),
        had_barcode_source: false,
        catalogs: Vec::new(),
        excluded: Default::default(),
        discid_results: Vec::new(),
        barcode_results: Vec::new(),
        discid_failure: None,
        barcode_failure: None,
        matched_barcode: None,
        track_count,
    }
}

fn identify(key: &str, state: crate::identify::IdentifyState) -> ImportEvent {
    ImportEvent::IdentifyStateChanged {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(0),
        state,
        toolbar: Vec::new(),
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

#[test]
fn import_progress_is_recorded_per_key_and_published_for_that_key_only() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";

    runtime.record_event(&progress(key, 42));

    let status = runtime
        .get(key)
        .and_then(|runtime| runtime.import_status)
        .expect("import status recorded");
    assert_eq!(
        status,
        CandidateImportStatusSnapshot::Importing {
            progress_percent: 42,
            step: Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
        }
    );
    let published = drain(&mut changes);
    assert_eq!(published.len(), 1);
    assert!(matches!(
        &published[0],
        CandidateRuntimeChange::Updated { key: changed, runtime }
            if changed == key && runtime.import_status == Some(status.clone())
    ));

    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::RemoteUploadQueued {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
            outbox_revision: 7,
        },
    });
    assert!(matches!(
        runtime.get(key).and_then(|runtime| runtime.import_status),
        Some(CandidateImportStatusSnapshot::CloudUploadQueued {
            ref release,
            outbox_revision: 7,
        }) if release.release_id == "rel1" && release.album_id == "alb"
    ));
}

#[test]
fn a_claim_is_the_queued_step_until_the_worker_reports() {
    let runtime = CandidateRuntime::default();
    runtime.claim_for_import("/watch/a/rel1");
    assert_eq!(
        runtime.runtime_for("/watch/a/rel1").import_status,
        Some(CandidateImportStatusSnapshot::Importing {
            progress_percent: 0,
            step: Some(ImportStep::Preparing(PrepareStep::Queued)),
        })
    );
    runtime.release_import_claim("/watch/a/rel1");
    assert_eq!(runtime.runtime_for("/watch/a/rel1").import_status, None);
}

#[test]
fn runtime_recorded_before_the_scan_survives_the_scan_reporting_the_key() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&progress(key, 42));

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));

    assert!(runtime
        .get(key)
        .is_some_and(|runtime| runtime.import_status.is_some()));
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

#[test]
fn loudness_and_queue_progress_never_touch_the_runtime() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    runtime.record_event(&ImportEvent::ImportLoudnessProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        tracks_done: 1,
        tracks_total: 9,
        fraction: Some(0.5),
    });
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
    let key = "/watch/a/rel1";
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    runtime.record_event(&ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        signals: crate::signals::Signals {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode: crate::signals::BarcodeSignal::Settled { codes: Vec::new() },
            text: crate::signals::TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            durations: crate::import::probe::ProbedDurations::totalling(1_000),
        },
        priority: CallPriority::Background,
    });
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
        runtime.runtime_for(key).identify_state,
        crate::identify::IdentifyState::ManualOnly { .. }
    ));

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
        candidate_key: key.to_string(),
    }));
    let recorded = runtime.runtime_for(key);
    assert!(
        matches!(
            recorded.identify_state,
            crate::identify::IdentifyState::Idle
        ),
        "the stored verdict owns the answer now, got {:?}",
        recorded.identify_state
    );
    assert!(recorded.toolbar.is_empty());
    assert!(
        recorded.signals.is_some(),
        "extraction's signals outlive the run's write"
    );

    // A newer run is in flight when the previous run's write lands: its
    // state is not terminal, so it stays.
    runtime.record_event(&identify(
        key,
        crate::identify::IdentifyState::Triangulating {
            discid: crate::identify::DiscidProgress::Computing,
            barcode: crate::identify::BarcodeProgress::Scanning,
            context: signals_context(9),
        },
    ));
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
        candidate_key: key.to_string(),
    }));
    assert!(matches!(
        runtime.runtime_for(key).identify_state,
        crate::identify::IdentifyState::Triangulating { .. }
    ));

    // A genuine mid-run cancel resets.
    runtime.record_event(&identify(key, crate::identify::IdentifyState::Idle));
    assert!(matches!(
        runtime.runtime_for(key).identify_state,
        crate::identify::IdentifyState::Idle
    ));
}
