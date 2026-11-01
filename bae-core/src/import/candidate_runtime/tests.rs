use super::*;
use crate::import::candidate_search::{CandidateSearch, SourceSearch};
use crate::import::candidates::ImportInFlight;
use crate::import::candidates::{
    Admission, CandidateFiles, IdentificationRuntime, SaveFailure, SavingAnswer,
};
use crate::import::folder_scanner::{CategorizedFiles, InvalidCandidate};
use crate::import::types::{
    CatalogAvailability, ImportPhase, ImportProgress, PrepareStep, SourceAvailability,
};
use crate::import::ImportStep;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::rate_limiter::CallPriority;
use std::path::PathBuf;

fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        files: Vec::new(),
        parts: Vec::new(),
    }
}

fn folder_candidate(key: &crate::import::CandidateKey, watched: &str) -> FolderCandidate {
    let path = key.test_folder();
    FolderCandidate {
        relative_folder: crate::import::RelativeFolder::of(path.strip_prefix(watched).unwrap())
            .unwrap(),
        name: format!("Candidate {}", path.display()),
        file_root: path.clone(),
        path,
        files: empty_categorized(),
        watched_folder_path: PathBuf::from(watched),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        grouping: None,
    }
}

fn scanned(candidate: FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate,

        found_while_automatic: false,
    })
}

fn progress(key: &crate::import::CandidateKey, percent: u8) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: Some(percent),
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    }
}

/// Claim `key` for the import every [`progress`] reports on, and take it up
/// as the worker does: its first step written, then reported.
async fn claim(runtime: &CandidateRuntime, key: &crate::import::CandidateKey) {
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    runtime
        .move_import(
            key,
            "imp-1",
            Some(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles)),
        )
        .await
        .unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Preparing {
            import_id: "imp-1".to_string(),
            step: PrepareStep::ValidatingSourceFiles,
        },
    });
}

/// Move `key`'s import to measuring loudness, as the worker writes it before
/// it starts, and report `percent` of it, as the measuring does.
async fn measuring(runtime: &CandidateRuntime, key: &crate::import::CandidateKey, percent: u8) {
    runtime
        .move_import(
            key,
            "imp-1",
            Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
        )
        .await
        .unwrap();
    runtime.record_event(&progress(key, percent));
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

/// `state` as its run's rows read it back, with no signals stored beside it.
pub(crate) fn published(state: crate::identify::IdentifyState) -> crate::identify::PublishedRun {
    crate::identify::PublishedRun::of(
        crate::identify::RunOutcome::of(&state).expect("a test publishes a run's state"),
        None,
    )
}

/// Store `key` as a candidate the scan holds under `/watch` with `revision`
/// of its file decisions, as the scan would, and return the files it is
/// identified as.
async fn scanned_at(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
    revision: u64,
) -> CandidateFiles {
    let files = CandidateFiles {
        content_hash: format!("hash:{key}"),
        file_edit_revision: revision,
    };
    runtime
        .execute_local_sql_for_test(&format!(
            "INSERT OR IGNORE INTO watched_import_folders (path, position)
                 VALUES (CAST('/watch' AS BLOB), 0);
             INSERT OR IGNORE INTO folder_scan_roots (watched_folder_path, generation, status, volume)
                 VALUES (CAST('/watch' AS BLOB), 1, 'complete', 'local');
             DELETE FROM scan_candidate WHERE path = X'{key}';
             INSERT INTO scan_candidate (watched_folder_path, path, generation, kind, name,
                 relative_folder, folder, file_root, scope, content_hash, file_edit_revision)
                 VALUES (CAST('/watch' AS BLOB), X'{key}', 1, 'valid', 'Candidate', X'{relative}',
                     X'{key}', X'{key}', 'recursive', '{hash}', {revision});",
            key = hex::encode(key.as_bytes()),
            relative = hex::encode(
                crate::import::RelativeFolder::of(key.test_folder().strip_prefix("/watch").unwrap())
                    .unwrap()
                    .as_bytes()
            ),
            hash = files.content_hash,
        ))
        .await;
    files
}

async fn scanned_key(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
) -> CandidateFiles {
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

/// Report `state` as `run` of `key` does: the run started with it stored,
/// then its answer written — the way the identify driver reports.
async fn report(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
    files: &CandidateFiles,
    run: crate::identify::IdentifyRunId,
    state: crate::identify::IdentifyState,
) {
    assert!(runtime.run_started(key, files, run, &state).await.unwrap());
    if state.is_terminal() {
        assert!(runtime
            .run_answered(key, run, state.picks_unattended())
            .await
            .unwrap());
    }
}

/// What the table says `key`'s identification is.
async fn identification(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
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

/// An import's step is the one its claim holds, per key, and how far through
/// it the import is is held per key apart from it.
#[tokio::test]
async fn an_import_s_step_is_its_claim_s_and_its_percent_is_held_per_key() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;
    claim(
        &runtime,
        &crate::import::CandidateKey::test("/watch/a/rel2"),
    )
    .await;

    measuring(&runtime, key, 42).await;

    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            step: ImportStep::Running(ImportPhase::MeasuringLoudness),
        })
    );
    assert_eq!(
        runtime
            .snapshot(&crate::import::CandidateKey::test("/watch/a/rel2"))
            .await
            .unwrap()
            .and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            step: ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
        }),
        "another key's progress is its own"
    );
    assert_eq!(
        runtime.percents(),
        HashMap::from([(key.clone(), 42)]),
        "only the key that reported a percent has one"
    );
}

/// Every way an import ends is a row by the time the event reaches here — the
/// release the worker committed, the failure it wrote, or the claim a cancel
/// ended — so the key stops being in flight.
#[tokio::test]
async fn a_finished_import_leaves_nothing_in_flight() {
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
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
        },
        ImportProgress::Failed {
            error: "no space left".to_string(),
            import_id: "imp-1".to_string(),
        },
    ];
    for ending in endings {
        let runtime = CandidateRuntime::for_test().await;
        claim(&runtime, key).await;
        measuring(&runtime, key, 42).await;

        runtime.move_import(key, "imp-1", None).await.unwrap();
        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.clone(),
            progress: ending.clone(),
        });
        assert!(
            runtime.snapshot(key).await.unwrap().is_none(),
            "{ending:?} left something behind"
        );
        assert!(runtime.inner.lock().unwrap().runtime.is_empty());
        assert!(runtime.percents().is_empty(), "{ending:?} left a percent");
    }
}

/// An answer still being written keeps the key: only the import half ends.
#[tokio::test]
async fn a_finished_import_keeps_a_key_whose_answer_is_still_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
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
            state: crate::identify::PublishedRun {
                outcome: crate::identify::RunOutcome::Answered(
                    crate::identify::TerminalVerdict::ManualOnly { .. }
                ),
                ..
            },
            ..
        }))
    ));
}

/// A reader that joins after imports started reads every one of them.
#[tokio::test]
async fn a_late_reader_reads_every_running_key() {
    let runtime = CandidateRuntime::for_test().await;
    claim(
        &runtime,
        &crate::import::CandidateKey::test("/watch/a/rel1"),
    )
    .await;
    measuring(
        &runtime,
        &crate::import::CandidateKey::test("/watch/a/rel1"),
        10,
    )
    .await;
    claim(
        &runtime,
        &crate::import::CandidateKey::test("/watch/a/rel2"),
    )
    .await;

    let running = runtime.snapshots().await.unwrap();
    assert_eq!(running.len(), 2);
    assert_eq!(
        running[&crate::import::CandidateKey::test("/watch/a/rel1")].import,
        Some(ImportInFlight {
            step: ImportStep::Running(ImportPhase::MeasuringLoudness),
        })
    );
    assert!(running[&crate::import::CandidateKey::test("/watch/a/rel2")]
        .import
        .is_some());
}

/// A preparing step reads back as the step the worker wrote.
#[tokio::test]
async fn a_preparing_step_is_the_step_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;

    runtime
        .move_import(
            key,
            "imp-1",
            Some(ImportStep::Preparing(PrepareStep::FetchingArtistPictures)),
        )
        .await
        .unwrap();

    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import)
            .expect("preparation is in flight"),
        ImportInFlight {
            step: ImportStep::Preparing(PrepareStep::FetchingArtistPictures),
        }
    );
}

/// An admission is one write, so the whole of what it opened stands at once;
/// withdrawing is per key, because that is how the queue lets keys go.
#[tokio::test]
async fn an_admission_is_one_write_and_each_key_leaves_on_its_own() {
    let runtime = CandidateRuntime::for_test().await;
    let first = &crate::import::CandidateKey::test("/watch/a/rel1");
    let second = &crate::import::CandidateKey::test("/watch/a/rel2");
    let marks = vec![
        (first.clone(), scanned_key(&runtime, first).await),
        (second.clone(), scanned_key(&runtime, second).await),
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    let work = runtime.start_work(CandidateWork::Identify, key.clone(), |token, _| token);
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");

    runtime.record_event(&ImportEvent::SignalsUpdated {
        candidate_key: key.clone(),
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
        candidate_key: key.clone(),
    }));
    assert!(runtime.signals(key).is_none());
}

/// A run's terminal state is its answer, and answering is not running: the
/// table moves it to being written, and the pane draws the answer.
#[tokio::test]
async fn a_terminal_state_moves_the_run_to_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
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
            crate::identify::PublishedRun {
                outcome: crate::identify::RunOutcome::Running { .. },
                ..
            }
        ))
    ));

    assert!(runtime
        .publish_candidate_run(key, run(1), &manual_only(), None)
        .await
        .unwrap());
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
            state: crate::identify::PublishedRun {
                outcome: crate::identify::RunOutcome::Answered(
                    crate::identify::TerminalVerdict::ManualOnly { .. }
                ),
                ..
            },
            import_asked: false,
        }))
    ));
}

/// A cancelled run's end ends that run and nothing else: a run that took the
/// key over keeps it, and a run that lost its wait reports into nothing.
#[tokio::test]
async fn a_run_s_end_ends_only_that_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(2), triangulating()).await;

    assert!(
        !runtime
            .run_started(key, &files, run(1), &triangulating())
            .await
            .unwrap(),
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
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
        .admit(vec![(key.clone(), files.clone())], Admission::Requested)
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime.cancel_identification(key).await.unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
    assert!(runtime.inner.lock().unwrap().runtime.is_empty());
}

include!("tests/claims.rs");
include!("tests/counting.rs");
include!("tests/departures.rs");
include!("tests/failing_writes.rs");
include!("tests/in_flight.rs");
include!("tests/published_runs.rs");
include!("tests/reading_the_pick.rs");
include!("tests/searches.rs");
include!("tests/watches.rs");
