use super::*;
use crate::db::LibraryStatus;
use crate::import::candidate_search::{CandidateSearch, SourceSearch};
use crate::import::folder_scanner::{CategorizedFiles, InvalidCandidate, InvalidReason};
use crate::import::types::{
    ImportPhase, ImportProgress, MetadataSource, MetadataSourceAvailability, PrepareStep,
    SourceAvailability,
};
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
        artwork: crate::signals::ArtworkScan::Absent,
        disc: crate::identify::state::DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent { track_count },
            ..Default::default()
        },
        barcode: Default::default(),
        catalog: Default::default(),
        track_count,
    }
}

fn run(number: u64) -> crate::identify::IdentifyRunId {
    crate::identify::IdentifyRunId::for_test(number)
}

fn identify(key: &str, run: u64, state: crate::identify::IdentifyState) -> ImportEvent {
    ImportEvent::IdentifyStateChanged {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(run),
        state,
        priority: CallPriority::Background,
    }
}

fn triangulating() -> crate::identify::IdentifyState {
    crate::identify::IdentifyState::Triangulating {
        discid: crate::identify::DiscidProgress::Computing,
        barcode: crate::identify::BarcodeProgress::Scanning,
        catalog: crate::identify::CatalogProgress::Skipped,
        context: signals_context(9),
    }
}

fn manual_only() -> crate::identify::IdentifyState {
    crate::identify::IdentifyState::ManualOnly {
        ledger: None,
        track_count: 9,
        context: signals_context(9),
    }
}

fn identification(runtime: &CandidateRuntime, key: &str) -> Option<crate::import::IdentificationStatus> {
    crate::import::triage::TriageRuntimeFacts::of(&runtime.get(key)?).identification
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

/// A verdict still being saved keeps the key: only the import half ends.
#[test]
fn a_finished_import_keeps_a_key_whose_verdict_is_still_being_saved() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, manual_only()));
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
    let recorded = runtime.get(key).expect("the pending save keeps the key");
    assert!(recorded.import.is_none());
    assert!(matches!(
        recorded.saving,
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
        artwork: crate::signals::ArtworkScan::Absent,
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

/// A run's terminal state is its answer, and answering is not saving: the
/// state leaves `running` and becomes the save the write step owns.
#[test]
fn a_terminal_state_moves_the_run_from_running_to_saving() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";

    runtime.record_event(&identify(key, 1, triangulating()));
    let progressing = runtime.get(key).expect("the run is in flight");
    assert!(matches!(
        progressing.running,
        Some(crate::identify::IdentifyState::Triangulating { .. })
    ));
    assert!(progressing.saving.is_none());

    runtime.record_event(&identify(key, 1, manual_only()));
    let answered = runtime.get(key).expect("the answer is being saved");
    assert!(answered.running.is_none());
    assert!(matches!(
        answered.saving,
        Some(crate::identify::IdentifyState::ManualOnly { .. })
    ));
}

/// A cancelled run broadcasts `Idle` on its way out. It ends that run and
/// nothing else: the run that superseded it keeps the field.
#[test]
fn an_idle_ends_only_the_run_that_broadcast_it() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 2, triangulating()));

    runtime.record_event(&identify(key, 1, crate::identify::IdentifyState::Idle));
    assert!(
        runtime
            .get(key)
            .is_some_and(|runtime| runtime.running.is_some()),
        "a superseded run's ending does not end the run that replaced it"
    );

    runtime.record_event(&identify(key, 2, crate::identify::IdentifyState::Idle));
    assert!(
        runtime.get(key).is_none(),
        "the running run's own ending empties the key"
    );
}

/// A save is over when its write says so, and the run id says whose write it
/// was: a later run's save outlives an earlier one's write landing.
#[test]
fn a_write_ends_the_save_it_ran_for_and_no_other() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, manual_only()));

    runtime.finish_identification_save(key, run(2));
    assert!(
        runtime
            .get(key)
            .is_some_and(|runtime| runtime.saving.is_some()),
        "another run's write says nothing about this save"
    );

    runtime.finish_identification_save(key, run(1));
    assert!(runtime.get(key).is_none());
}

/// A write that failed leaves the error where a row can say so, and nothing
/// waiting on a commit that is not coming.
#[test]
fn a_failed_write_replaces_the_pending_save_with_its_error() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, manual_only()));

    runtime.fail_identification(key, run(1), "database write failed".to_string());

    let failed = runtime.get(key).expect("the failure remains visible");
    assert!(failed.saving.is_none());
    assert_eq!(failed.save_failed.as_deref(), Some("database write failed"));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            error: "database write failed".to_string(),
        })
    );
}

/// The failure describes the run that failed. The next run is a fresh attempt,
/// so its first state clears it — and a further state of that same run does
/// not, because nothing about it is new.
#[test]
fn a_new_runs_first_state_clears_the_previous_runs_failure() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, manual_only()));
    runtime.fail_identification(key, run(1), "database write failed".to_string());

    runtime.record_event(&identify(key, 2, triangulating()));
    let rerun = runtime.get(key).expect("the new run is in flight");
    assert!(rerun.save_failed.is_none());
    assert!(rerun.running.is_some());

    runtime.fail_identification(key, run(2), "database write failed again".to_string());
    runtime.record_event(&identify(key, 2, triangulating()));
    assert_eq!(
        runtime
            .get(key)
            .and_then(|runtime| runtime.save_failed)
            .as_deref(),
        Some("database write failed again"),
        "a state from the run that failed is not a new attempt"
    );
}

/// A claimed import outlives the run's write: the key keeps the half that is
/// still happening.
#[test]
fn a_finished_save_keeps_a_key_whose_import_is_running() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&identify(key, 1, manual_only()));
    runtime.claim_for_import(key);

    runtime.finish_identification_save(key, run(1));

    let recorded = runtime.get(key).expect("the claim keeps the key");
    assert!(recorded.saving.is_none());
    assert!(recorded.import.is_some());
}

/// An answer that will never be stored — the candidate moved on while its run
/// settled — leaves nothing in flight. Kept, the pending save would read as a
/// commit still waiting, for good.
#[test]
fn discarding_an_unstorable_answer_leaves_nothing_in_flight() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel";
    runtime.record_event(&identify(
        key,
        1,
        crate::identify::IdentifyState::NotFoundAnywhere {
            ledger: None,
            context: signals_context(9),
        },
    ));
    assert!(runtime
        .get(key)
        .is_some_and(|runtime| runtime.saving.is_some()));

    runtime.finish_identification_save(key, run(1));

    assert!(runtime.get(key).is_none());
}

/// Each field is one fact, and a row reads them in one order: what went wrong
/// last, then what is being committed, then what is running, then what is
/// waiting for a slot.
#[test]
fn every_field_yields_its_own_status_in_one_order() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";

    runtime.requeue_automatic_identification(key);
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Queued)
    );

    runtime.record_event(&identify(key, 1, triangulating()));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Running),
        "a run outranks the queue marker it was planned under"
    );

    runtime.record_event(&identify(key, 1, manual_only()));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Finalizing)
    );

    runtime.fail_identification(key, run(1), "database write failed".to_string());
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            error: "database write failed".to_string(),
        })
    );

    runtime.record_event(&identify(key, 2, manual_only()));
    assert_eq!(
        identification(&runtime, key),
        Some(crate::import::IdentificationStatus::Finalizing),
        "the new run's save outranks the previous run's failure, which it cleared"
    );
}

/// A library that asks every source — what these tests search from unless
/// they are about a source that is not asked.
fn every_source_on() -> Vec<MetadataSourceAvailability> {
    MetadataSource::ALL
        .into_iter()
        .map(|source| MetadataSourceAvailability {
            source,
            state: SourceAvailability::On,
        })
        .collect()
}

fn search_query() -> crate::import::search::SearchQuery {
    crate::import::search::SearchQuery::General {
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
    }
}

fn search_result(
    source: MetadataSource,
    release_id: &str,
) -> (crate::import::search::MetadataResult, LibraryStatus) {
    (
        crate::import::search::MetadataResult {
            source,
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1992),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            cover_art: None,
            source_group_id: None,
            source_tracks: None,
        },
        LibraryStatus::absent(release_id),
    )
}

fn published_search(
    changes: &mut broadcast::Receiver<CandidateRuntimeChange>,
) -> Vec<Option<CandidateSearch>> {
    drain(changes)
        .into_iter()
        .map(|change| match change {
            CandidateRuntimeChange::Updated { runtime, .. } => runtime.search,
            CandidateRuntimeChange::Removed { .. } => None,
            CandidateRuntimeChange::Reset { .. } => panic!("a search never resets the queue"),
        })
        .collect()
}

/// Both sources land on the same run, one after the other, and neither
/// landing loses the other's answer — nor does what is published lag what the
/// next landing folds into.
#[test]
fn two_landings_on_one_run_both_stand() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";
    let run = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));

    assert!(runtime.land_search(
        key,
        run,
        MetadataSource::MusicBrainz,
        Ok(vec![search_result(MetadataSource::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        MetadataSource::Discogs,
        Ok(vec![search_result(MetadataSource::Discogs, "dg-1")]),
    ));

    let landed = runtime
        .get(key)
        .and_then(|runtime| runtime.search)
        .expect("the search is what is in flight for the key");
    assert!(matches!(
        landed.source(MetadataSource::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert!(matches!(
        landed.source(MetadataSource::Discogs),
        Some(SourceSearch::Done { .. })
    ));
    assert_eq!(landed.library_statuses.len(), 2);
    assert_eq!(
        published_search(&mut changes).last().cloned().flatten(),
        Some(landed),
        "the last change published carries what the map holds"
    );
}

/// A superseded run's landing goes nowhere, publishes nothing, and leaves the
/// new run untouched.
#[test]
fn a_superseded_run_cannot_land() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    let first = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));
    let second = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));
    assert!(!runtime.search_run_is_current(key, first));
    assert!(runtime.search_run_is_current(key, second));

    let mut changes = runtime.subscribe();
    assert!(!runtime.land_search(
        key,
        first,
        MetadataSource::MusicBrainz,
        Ok(vec![search_result(MetadataSource::MusicBrainz, "mb-1")]),
    ));
    assert!(published_search(&mut changes).is_empty());
    assert!(matches!(
        runtime
            .get(key)
            .and_then(|runtime| runtime.search)
            .expect("the second run stands")
            .source(MetadataSource::MusicBrainz),
        Some(SourceSearch::Searching)
    ));
}

/// A cleared search is gone from the key, and the run it was on cannot be
/// started again by a landing that was still out.
#[test]
fn a_cleared_search_cannot_land() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    let run = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));

    runtime.clear_search(key);
    assert!(runtime.get(key).is_none(), "nothing else was running");
    assert!(!runtime.search_run_is_current(key, run));
    assert!(!runtime.land_search(
        key,
        run,
        MetadataSource::MusicBrainz,
        Ok(vec![search_result(MetadataSource::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.get(key).is_none());
}

/// Retry re-asks only the failed source, on a new run, keeping what the other
/// found. A search with nothing to re-ask changes nothing.
#[test]
fn a_retry_re_asks_only_the_failed_sources_on_a_new_run() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    let run = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));
    assert!(runtime.land_search(
        key,
        run,
        MetadataSource::MusicBrainz,
        Ok(vec![search_result(MetadataSource::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        MetadataSource::Discogs,
        Err(crate::signals::LookupFailure::Network),
    ));

    let (query, sources, retried) = runtime.retry_search(key).expect("Discogs failed");
    assert_eq!(query, search_query());
    assert_eq!(sources, vec![MetadataSource::Discogs]);
    assert!(runtime.search_run_is_current(key, retried));
    assert!(!runtime.search_run_is_current(key, run));
    let search = runtime
        .get(key)
        .and_then(|runtime| runtime.search)
        .expect("the retried search is in flight");
    assert!(matches!(
        search.source(MetadataSource::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert_eq!(
        search.source(MetadataSource::Discogs),
        Some(&SourceSearch::Searching)
    );

    assert!(runtime.land_search(
        key,
        retried,
        MetadataSource::Discogs,
        Ok(vec![search_result(MetadataSource::Discogs, "dg-1")]),
    ));
    assert!(
        runtime.retry_search(key).is_none(),
        "a settled search has no source to re-ask"
    );
    assert!(
        runtime.retry_search("/watch/a/rel2").is_none(),
        "a key with no search has none either"
    );
}

/// The search is what is in flight for the key, so a rebound sheet — a
/// different disc — takes it with the rest of the key's runtime.
#[test]
fn rebinding_a_sheet_drops_the_key_s_search() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    let run = runtime.start_search(key, CandidateSearch::started(search_query(), &every_source_on()));

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
        candidate: folder_candidate(key, "/watch/a"),
    }));

    assert!(runtime.get(key).is_none());
    assert!(!runtime.search_run_is_current(key, run));
}

/// Switching a source off closes its part of every search running right now,
/// wherever the person is looking — and only publishes the keys it changed.
#[test]
fn switching_a_source_off_closes_its_part_of_every_live_search() {
    let runtime = CandidateRuntime::default();
    let searching = "/watch/a/rel1";
    let importing = "/watch/a/rel2";
    let run = runtime.start_search(
        searching,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    runtime.claim_for_import(importing);
    let mut changes = runtime.subscribe();

    runtime.switch_source_off(MetadataSource::MusicBrainz);

    let search = runtime
        .get(searching)
        .and_then(|state| state.search)
        .expect("the search is still what is in flight for the key");
    assert_eq!(
        search.source(MetadataSource::MusicBrainz),
        Some(&SourceSearch::Off)
    );
    assert_eq!(
        search.source(MetadataSource::Discogs),
        Some(&SourceSearch::Searching),
        "the source still being asked is untouched"
    );
    assert!(
        runtime.search_run_is_current(searching, run),
        "the run stands, so the other source's lookup still lands"
    );
    assert_eq!(
        published_search(&mut changes),
        vec![Some(search)],
        "only the key whose search changed is published"
    );
}

/// The lookup the dropped source already had out lands on a part that is no
/// longer looking, so it goes nowhere and the search stays closed on it.
#[test]
fn a_landing_for_a_switched_off_source_goes_nowhere() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.switch_source_off(MetadataSource::MusicBrainz);
    assert!(
        runtime.land_search(
            key,
            run,
            MetadataSource::MusicBrainz,
            Ok(vec![search_result(MetadataSource::MusicBrainz, "mb-1")]),
        ),
        "the run is current, so the landing reaches the search"
    );

    let search = runtime
        .get(key)
        .and_then(|state| state.search)
        .expect("the search is what is in flight for the key");
    assert_eq!(
        search.source(MetadataSource::MusicBrainz),
        Some(&SourceSearch::Off),
        "a part that is not looking takes no answer"
    );
    assert!(search.groups.is_empty());
}
