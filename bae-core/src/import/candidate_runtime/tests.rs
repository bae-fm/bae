use super::*;
use crate::db::LibraryStatus;
use crate::import::candidate_search::{CandidateSearch, SourceSearch};
use crate::import::candidates::{
    Admission, CandidateFiles, IdentificationRuntime, SaveFailure, SavingAnswer,
};
use crate::import::folder_scanner::{CategorizedFiles, InvalidCandidate};
use crate::import::types::{
    CatalogAvailability, ImportPhase, ImportProgress, PrepareStep, SourceAvailability,
};
use crate::import::{ImportStanding, ImportStep};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::rate_limiter::CallPriority;
use std::path::PathBuf;

fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        files: Vec::new(),
        parts: Vec::new(),
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
        grouping: None,
    }
}

fn scanned(candidate: FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate,
        skipped: false,
        is_added: false,
        found_while_automatic: false,
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

/// Claim `key` for the import every [`progress`] reports on, and take it up
/// as the worker does before it reports.
async fn claim(runtime: &CandidateRuntime, key: &str) {
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    runtime
        .move_import(key, "imp-1", Some(ImportStanding::Running))
        .await
        .unwrap();
}

pub(crate) fn signals_context(track_count: u32) -> crate::identify::state::SignalsContext {
    crate::identify::state::SignalsContext {
        origin: crate::signals::AudioOrigin::default(),
        providers: Vec::new(),
        artwork: crate::signals::ArtworkScan::Absent,
        disc: crate::identify::state::DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent,
            ..Default::default()
        },
        barcode: Default::default(),
        catalog: Default::default(),
        search: Default::default(),
        text: Default::default(),
        text_settled: true,
        audio: crate::signals::AudioFacts {
            track_count,
            ..Default::default()
        },
        documents: crate::identify::documents::DocumentReading::Pending,
        pressings: Vec::new(),
        isrc: Default::default(),
        track_titles: Vec::new(),
    }
}

fn run(number: u64) -> crate::identify::IdentifyRunId {
    crate::identify::IdentifyRunId::for_test(number)
}

/// A run's state partway through, which the runtime records as running.
pub(crate) fn triangulating() -> crate::identify::IdentifyState {
    crate::identify::IdentifyState::Triangulating {
        discid: crate::identify::DiscidProgress::Computing,
        barcode: crate::identify::BarcodeProgress::Scanning,
        catalog: crate::identify::CatalogProgress::Skipped,
        isrc: crate::identify::IsrcProgress::Skipped,
        search: crate::identify::SearchProgress::Pending,
        context: signals_context(9),
    }
}

pub(crate) fn manual_only() -> crate::identify::IdentifyState {
    crate::identify::IdentifyState::ManualOnly {
        ledger: None,
        track_count: 9,
        context: signals_context(9),
    }
}

/// Store `key` as a candidate the scan holds under `/watch` with `revision`
/// of its file decisions, as the scan would, and return the files it is
/// identified as.
async fn scanned_at(runtime: &CandidateRuntime, key: &str, revision: u64) -> CandidateFiles {
    let files = CandidateFiles {
        content_hash: format!("hash:{key}"),
        file_edit_revision: revision,
    };
    runtime
        .execute_local_sql_for_test(&format!(
            "INSERT OR IGNORE INTO watched_import_folders (path, position) VALUES ('/watch', 0);
             INSERT OR IGNORE INTO folder_scan_roots (watched_folder_path, generation, status, volume)
                 VALUES ('/watch', 1, 'complete', 'local');
             DELETE FROM scan_candidate WHERE path = '{key}';
             INSERT INTO scan_candidate (watched_folder_path, path, generation, kind, name,
                 display_path, folder, file_root, scope, content_hash, file_edit_revision)
                 VALUES ('/watch', '{key}', 1, 'valid', 'Candidate', '{display_path}', '{key}', '{key}',
                     'recursive', '{hash}', {revision});",
            hash = files.content_hash,
            display_path = key.trim_start_matches('/'),
        ))
        .await;
    files
}

async fn scanned_key(runtime: &CandidateRuntime, key: &str) -> CandidateFiles {
    scanned_at(runtime, key, 0).await
}

/// Make every write of a candidate's standing that one of `failing` names —
/// a statement (`INSERT`, `UPDATE`, `DELETE`) and the condition on its row,
/// `NEW` or `OLD`, that it fails for — fail, as a full disk fails it: a
/// trigger on the table of the library under `directory`, added from a
/// connection of its own, since no write of the library makes a schema
/// change.
pub(crate) async fn fail_standing_writes(directory: &std::path::Path, failing: &[(&str, &str)]) {
    let directory = directory.to_path_buf();
    let triggers: String = failing
        .iter()
        .enumerate()
        .map(|(index, (statement, when))| {
            format!(
                "CREATE TRIGGER fail_standing_write_{index} BEFORE {statement} \
                 ON import_candidate_standing WHEN {when} \
                 BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;"
            )
        })
        .collect();
    tokio::task::spawn_blocking(move || {
        let mut found = false;
        let mut dirs = vec![directory];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let is_database =
                    std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(b"SQLite format 3\0"));
                if !is_database {
                    continue;
                }
                let connection = rusqlite::Connection::open(&path).unwrap();
                let holds_standings: bool = connection
                    .query_row(
                        "SELECT EXISTS (SELECT 1 FROM sqlite_master \
                         WHERE name = 'import_candidate_standing')",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                if !holds_standings {
                    continue;
                }
                connection.execute_batch(&triggers).unwrap();
                found = true;
            }
        }
        assert!(
            found,
            "the library under the directory holds the standings table"
        );
    })
    .await
    .unwrap();
}

/// Report `state` as `run` of `key` does: what it published recorded, then
/// where it leaves the run written — the way the identify driver reports.
async fn report(
    runtime: &CandidateRuntime,
    key: &str,
    files: &CandidateFiles,
    run: crate::identify::IdentifyRunId,
    state: crate::identify::IdentifyState,
) {
    runtime.record_run_state(key, run, true, &state);
    if state.is_terminal() {
        runtime.run_started(key, files, run).await.unwrap();
        assert!(runtime
            .run_answered(key, run, state.picks_unattended())
            .await
            .unwrap());
    } else {
        assert!(runtime.run_started(key, files, run).await.unwrap());
    }
}

/// What the table says `key`'s identification is.
async fn identification(
    runtime: &CandidateRuntime,
    key: &str,
) -> Option<crate::import::IdentificationStatus> {
    runtime.standing_facts(key).await.unwrap().identification
}

fn extracted_signals() -> crate::signals::Signals {
    crate::signals::Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: crate::signals::DiscIdSignal::Absent,
        barcode: crate::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: crate::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// How far an import has got is held per key and composed with the claim the
/// table holds; a report of the step the table has not reached is not drawn.
#[tokio::test]
async fn import_progress_is_held_per_key_beside_its_claim() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    claim(&runtime, key).await;
    claim(&runtime, "/watch/a/rel2").await;

    runtime.record_event(&progress(key, 42));

    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            progress_percent: Some(42),
            step: ImportStep::Running(ImportPhase::MeasuringLoudness),
        })
    );
    assert_eq!(
        runtime
            .snapshot("/watch/a/rel2")
            .await
            .unwrap()
            .and_then(|runtime| runtime.import)
            .and_then(|import| import.progress_percent),
        None,
        "another key's progress is its own"
    );
}

/// Every way an import ends is a row by the time the event reaches here — the
/// release the worker committed, the failure it wrote, or the claim a cancel
/// ended — so the key stops being in flight.
#[tokio::test]
async fn a_finished_import_leaves_nothing_in_flight() {
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
        let runtime = CandidateRuntime::for_test().await;
        claim(&runtime, key).await;
        runtime.record_event(&progress(key, 42));

        runtime.move_import(key, "imp-1", None).await.unwrap();
        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.to_string(),
            progress: ending.clone(),
        });
        assert!(
            runtime.snapshot(key).await.unwrap().is_none(),
            "{ending:?} left something behind"
        );
        assert!(runtime.inner.lock().unwrap().runtime.is_empty());
    }
}

/// An answer still being written keeps the key: only the import half ends.
#[tokio::test]
async fn a_finished_import_keeps_a_key_whose_answer_is_still_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    claim(&runtime, key).await;

    runtime.move_import(key, "imp-1", None).await.unwrap();
    let recorded = runtime
        .snapshot(key)
        .await
        .unwrap()
        .expect("the answer keeps the key");
    assert!(recorded.import.is_none());
    assert!(matches!(
        recorded.identification,
        Some(IdentificationRuntime::Saving(SavingAnswer {
            state: crate::identify::IdentifyState::ManualOnly { .. },
            ..
        }))
    ));
}

/// A reader that joins after imports started reads every one of them.
#[tokio::test]
async fn a_late_reader_reads_every_running_key() {
    let runtime = CandidateRuntime::for_test().await;
    claim(&runtime, "/watch/a/rel1").await;
    runtime.record_event(&progress("/watch/a/rel1", 10));
    claim(&runtime, "/watch/a/rel2").await;

    let running = runtime.snapshots().await.unwrap();
    assert_eq!(running.len(), 2);
    assert_eq!(
        running["/watch/a/rel1"].import,
        Some(ImportInFlight {
            progress_percent: Some(10),
            step: ImportStep::Running(ImportPhase::MeasuringLoudness),
        })
    );
    assert!(running["/watch/a/rel2"].import.is_some());
}

#[tokio::test]
async fn a_preparing_step_has_no_progress_fraction() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    claim(&runtime, key).await;

    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Preparing {
            import_id: "imp-1".to_string(),
            step: PrepareStep::ValidatingSourceFiles,
        },
    });

    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import)
            .expect("preparation is in flight")
            .progress_percent,
        None
    );
}

/// An admission is one write, so the whole of what it opened stands at once;
/// withdrawing is per key, because that is how the queue lets keys go.
#[tokio::test]
async fn an_admission_is_one_write_and_each_key_leaves_on_its_own() {
    let runtime = CandidateRuntime::for_test().await;
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let marks = vec![
        (first.to_string(), scanned_key(&runtime, first).await),
        (second.to_string(), scanned_key(&runtime, second).await),
    ];

    runtime.admit(marks, Admission::Automatic).await.unwrap();

    let queued = runtime.snapshots().await.unwrap();
    assert_eq!(queued.len(), 2);
    for key in [first, second] {
        assert_eq!(
            queued[key].identification,
            Some(IdentificationRuntime::Queued(Admission::Automatic))
        );
    }

    runtime.withdraw(first).await.unwrap();
    assert!(runtime.snapshot(first).await.unwrap().is_none());
    runtime.withdraw(second).await.unwrap();
    assert!(runtime.snapshots().await.unwrap().is_empty());
}

/// An import's claim stands whatever the scan reports about its candidate.
#[tokio::test]
async fn a_claim_survives_the_scan_reporting_the_key() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    claim(&runtime, key).await;
    runtime.record_event(&progress(key, 42));

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));

    assert!(runtime
        .snapshot(key)
        .await
        .unwrap()
        .is_some_and(|runtime| runtime.import.is_some()));
}

/// A rescan reporting the same files keeps what is held for the key; one
/// reporting other files ends its work and its search.
#[tokio::test]
async fn a_rescan_reporting_new_files_ends_the_key_s_work_and_search() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    let work = runtime.start_work(CandidateWork::Identify, key.to_string(), |token, _| token);
    runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    assert!(!work.is_cancelled(), "an unchanged re-scan keeps the work");
    assert!(runtime.snapshot(key).await.unwrap().is_some());

    let mut reshaped = folder_candidate(key, "/watch/a");
    reshaped.file_edit_revision = 1;
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
        candidate: reshaped,
    }));
    assert!(work.is_cancelled());
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// Extraction's snapshots are kept for the one form that reads them, but they
/// are not what is in flight: no entry appears for them. The form is fed by
/// the UI events instead.
#[tokio::test]
async fn extracted_signals_are_retained_without_a_runtime_entry() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";

    runtime.record_event(&ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(1),
        signals: extracted_signals(),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });

    assert!(
        runtime.signals(key).is_some(),
        "the form can read them back"
    );
    assert!(runtime.snapshot(key).await.unwrap().is_none());

    // They describe this key's current files, so the scan dropping the key
    // drops them with it.
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: key.to_string(),
    }));
    assert!(runtime.signals(key).is_none());
}

/// A run's terminal state is its answer, and answering is not running: the
/// table moves it to being written, and the pane draws the answer.
#[tokio::test]
async fn a_terminal_state_moves_the_run_to_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert!(matches!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Running(
            crate::identify::IdentifyState::Triangulating { .. }
        ))
    ));

    runtime.record_run_state(key, run(1), true, &manual_only());
    assert!(runtime.run_answered(key, run(1), false).await.unwrap());
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::Finalizing)
    );
    assert!(matches!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Saving(SavingAnswer {
            state: crate::identify::IdentifyState::ManualOnly { .. },
            import_asked: false,
        }))
    ));
}

/// A cancelled run's end ends that run and nothing else: a run that took the
/// key over keeps it, and a run that lost its wait reports into nothing.
#[tokio::test]
async fn a_run_s_end_ends_only_that_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(2), triangulating()).await;

    assert!(
        !runtime.run_started(key, &files, run(1)).await.unwrap(),
        "a run that lost its wait holds no standing"
    );
    runtime.run_ended(key, run(1)).await.unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::Running),
        "another run's ending does not end the run that replaced it"
    );

    runtime.run_ended(key, run(2)).await.unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// An answer is over when its write says so, and the run id says whose write
/// it was.
#[tokio::test]
async fn a_write_ends_the_answer_it_ran_for_and_no_other() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime
        .end_identification_answer(key, run(2))
        .await
        .unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::Finalizing),
        "another run's write says nothing about this answer"
    );

    runtime
        .end_identification_answer(key, run(1))
        .await
        .unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// A write that did not land, which another run may land.
fn not_written(error: &str) -> SaveFailure {
    SaveFailure::NotWritten {
        error: error.to_string(),
    }
}

/// A write that failed leaves the error where a row can say so, and nothing
/// waiting on a commit that is not coming; the next run's start clears it.
#[tokio::test]
async fn a_failed_write_stands_until_the_next_run_starts() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime
        .fail_identification(key, run(1), &files, not_written("database write failed"))
        .await
        .unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            failure: not_written("database write failed"),
        })
    );
    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::SaveFailed(not_written(
            "database write failed"
        )))
    );

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    report(&runtime, key, &files, run(2), triangulating()).await;
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::Running)
    );
}

/// A claimed import outlives the run's write: the key keeps the half that is
/// still happening.
#[tokio::test]
async fn a_finished_answer_keeps_a_key_whose_import_is_running() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    claim(&runtime, key).await;

    runtime
        .end_identification_answer(key, run(1))
        .await
        .unwrap();

    let recorded = runtime
        .snapshot(key)
        .await
        .unwrap()
        .expect("the claim keeps the key");
    assert!(recorded.identification.is_none());
    assert!(recorded.import.is_some());
}

/// A cancel ends whatever run or answer the key holds and what every run of
/// it published, and leaves its wait.
#[tokio::test]
async fn a_cancel_ends_the_run_and_what_it_published() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime.cancel_identification(key).await.unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
    assert!(runtime.inner.lock().unwrap().runtime.is_empty());
}

/// A library that asks every catalog it can — what these tests search from
/// unless they are about a catalog that is not asked.
fn every_source_on() -> Vec<CatalogAvailability> {
    Catalog::LOOKUP
        .into_iter()
        .map(|catalog| CatalogAvailability {
            catalog,
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
    source: Catalog,
    release_id: &str,
) -> (crate::import::search::MetadataResult, LibraryStatus) {
    (
        crate::import::search::MetadataResult {
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1992),
            ..crate::import::search::MetadataResult::for_test(source, release_id, None)
        },
        LibraryStatus::absent(release_id),
    )
}

/// The search `key` holds, as its snapshot draws it.
async fn search_of(runtime: &CandidateRuntime, key: &str) -> Option<CandidateSearch> {
    runtime
        .snapshot(key)
        .await
        .unwrap()
        .and_then(|runtime| runtime.search)
}

/// Both sources land on the same run, one after the other, and neither
/// landing loses the other's answer.
#[tokio::test]
async fn two_landings_on_one_run_both_stand() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    assert!(runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        Catalog::Discogs,
        Ok(vec![search_result(Catalog::Discogs, "dg-1")]),
    ));

    let landed = search_of(&runtime, key)
        .await
        .expect("the search is what is in flight for the key");
    assert!(matches!(
        landed.source(Catalog::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert!(matches!(
        landed.source(Catalog::Discogs),
        Some(SourceSearch::Done { .. })
    ));
    assert_eq!(landed.library_statuses.len(), 2);
}

/// A superseded run's landing goes nowhere and leaves the new run untouched.
#[tokio::test]
async fn a_superseded_run_cannot_land() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let first = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    let second = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    assert!(!runtime.search_run_is_current(key, first));
    assert!(runtime.search_run_is_current(key, second));

    assert!(!runtime.land_search(
        key,
        first,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(matches!(
        search_of(&runtime, key)
            .await
            .expect("the second run stands")
            .source(Catalog::MusicBrainz),
        Some(SourceSearch::Searching)
    ));
}

/// A cleared search is gone from the key, and the run it was on cannot be
/// started again by a landing that was still out.
#[tokio::test]
async fn a_cleared_search_cannot_land() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.clear_search(key);
    assert!(
        runtime.snapshot(key).await.unwrap().is_none(),
        "nothing else was running"
    );
    assert!(!runtime.search_run_is_current(key, run));
    assert!(!runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// Retry re-asks only the failed source, on a new run, keeping what the other
/// found. A search with nothing to re-ask changes nothing.
#[tokio::test]
async fn a_retry_re_asks_only_the_failed_sources_on_a_new_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    assert!(runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        Catalog::Discogs,
        Err(crate::signals::LookupFailure::Network.into()),
    ));

    let (query, sources, retried) = runtime.retry_search(key).expect("Discogs failed");
    assert_eq!(query, search_query());
    assert_eq!(sources, vec![Catalog::Discogs]);
    assert!(runtime.search_run_is_current(key, retried));
    assert!(!runtime.search_run_is_current(key, run));
    let search = search_of(&runtime, key)
        .await
        .expect("the retried search is in flight");
    assert!(matches!(
        search.source(Catalog::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert_eq!(
        search.source(Catalog::Discogs),
        Some(&SourceSearch::Searching)
    );

    assert!(runtime.land_search(
        key,
        retried,
        Catalog::Discogs,
        Ok(vec![search_result(Catalog::Discogs, "dg-1")]),
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

/// Switching a source off closes its part of every search running right now,
/// wherever the person is looking, and leaves its run standing.
#[tokio::test]
async fn switching_a_source_off_closes_its_part_of_every_live_search() {
    let runtime = CandidateRuntime::for_test().await;
    let searching = "/watch/a/rel1";
    let run = runtime.start_search(
        searching,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.switch_source_off(Catalog::MusicBrainz);

    let search = search_of(&runtime, searching)
        .await
        .expect("the search is still what is in flight for the key");
    assert_eq!(
        search.source(Catalog::MusicBrainz),
        Some(&SourceSearch::Off)
    );
    assert_eq!(
        search.source(Catalog::Discogs),
        Some(&SourceSearch::Searching),
        "the source still being asked is untouched"
    );
    assert!(
        runtime.search_run_is_current(searching, run),
        "the run stands, so the other source's lookup still lands"
    );
}

/// The lookup the dropped source already had out lands on a part that is no
/// longer looking, so it goes nowhere and the search stays closed on it.
#[tokio::test]
async fn a_landing_for_a_switched_off_source_goes_nowhere() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.switch_source_off(Catalog::MusicBrainz);
    assert!(
        runtime.land_search(
            key,
            run,
            Catalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        ),
        "the run is current, so the landing reaches the search"
    );

    let search = search_of(&runtime, key)
        .await
        .expect("the search is what is in flight for the key");
    assert_eq!(
        search.source(Catalog::MusicBrainz),
        Some(&SourceSearch::Off),
        "a part that is not looking takes no answer"
    );
    assert!(search.groups.is_empty());
}

include!("tests/claims.rs");
include!("tests/counting.rs");
include!("tests/departures.rs");
include!("tests/failing_writes.rs");
include!("tests/in_flight.rs");
include!("tests/reading_the_pick.rs");
include!("tests/watches.rs");
