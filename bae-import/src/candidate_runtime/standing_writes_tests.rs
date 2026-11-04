//! The standings table's writes: admitting and withdrawing, a run's start,
//! answer and end, an answer's write ending or failing, an import's claim and
//! its steps, the identification count each write moves, and a write that
//! cannot land.

use super::*;
use crate::candidate_runtime::tests::*;
use crate::folder_scanner::InvalidCandidate;
use crate::types::{ImportPhase, ImportProgress, PrepareStep};
use crate::ImportEvent;
use crate::ImportInFlight;
use crate::ImportStep;
use crate::ScanEvent;
use crate::{Admission, IdentificationRuntime, SaveFailure, SavingAnswer};
use bae_catalogs::catalog::LookupCatalog;

/// An answer still being written keeps the key: only the import half ends.
#[tokio::test]
async fn a_finished_import_keeps_a_key_whose_answer_is_still_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    claim(&runtime, key).await;

    runtime.move_import(key, "imp-1", None).await.unwrap();
    let recorded = runtime.snapshot(key).await.unwrap().expect("the answer keeps the key");
    assert!(recorded.import.is_none());
    assert!(matches!(
        recorded.identification,
        Some(IdentificationRuntime::Saving(SavingAnswer {
            state: bae_identify::PublishedRun {
                outcome: bae_identify::RunOutcome::Answered(
                    bae_identify::TerminalVerdict::ManualOnly { .. }
                ),
                ..
            },
            ..
        }))
    ));
}

/// A preparing step reads back as the step the worker wrote.
#[tokio::test]
async fn a_preparing_step_is_the_step_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;

    runtime
        .move_import(key, "imp-1", Some(ImportStep::Preparing(PrepareStep::FetchingArtistPictures)))
        .await
        .unwrap();

    assert_eq!(
        runtime
            .snapshot(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import)
            .expect("preparation is in flight"),
        ImportInFlight { step: ImportStep::Preparing(PrepareStep::FetchingArtistPictures) }
    );
}

/// An admission is one write, so the whole of what it opened stands at once;
/// withdrawing is per key, because that is how the queue lets keys go.
#[tokio::test]
async fn an_admission_is_one_write_and_each_key_leaves_on_its_own() {
    let runtime = CandidateRuntime::for_test().await;
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
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

/// A run's terminal state is its answer, and answering is not running: the
/// table moves it to being written, and the pane draws the answer.
#[tokio::test]
async fn a_terminal_state_moves_the_run_to_being_written() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert!(matches!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Running(bae_identify::PublishedRun {
            outcome: bae_identify::RunOutcome::Running { .. },
            ..
        }))
    ));

    assert!(runtime.tables.publish_candidate_run(key, run(1), &manual_only(), None).await.unwrap());
    assert!(runtime.run_answered(key, run(1), false).await.unwrap());
    assert_eq!(identification(&runtime, key).await, Some(crate::IdentificationStatus::Finalizing));
    assert!(matches!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Saving(SavingAnswer {
            state: bae_identify::PublishedRun {
                outcome: bae_identify::RunOutcome::Answered(
                    bae_identify::TerminalVerdict::ManualOnly { .. }
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
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(2), triangulating()).await;

    assert!(
        !runtime.run_started(key, &files, run(1), &triangulating()).await.unwrap(),
        "a run that lost its wait holds no standing"
    );
    runtime.run_ended(key, run(1)).await.unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::Running),
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
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime.end_identification_answer(key, run(2)).await.unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::Finalizing),
        "another run's write says nothing about this answer"
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// A write that did not land, which another run may land.
fn not_written(error: &str) -> SaveFailure {
    SaveFailure::NotWritten { error: error.to_string() }
}

/// A write that failed leaves the error where a row can say so, and nothing
/// waiting on a commit that is not coming; the next run's start clears it.
#[tokio::test]
async fn a_failed_write_stands_until_the_next_run_starts() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime
        .fail_identification(key, run(1), &files, not_written("database write failed"))
        .await
        .unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::FinalizationFailed {
            failure: not_written("database write failed"),
        })
    );
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::SaveFailed(not_written("database write failed")))
    );

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    report(&runtime, key, &files, run(2), triangulating()).await;
    assert_eq!(identification(&runtime, key).await, Some(crate::IdentificationStatus::Running));
}

/// A claimed import outlives the run's write: the key keeps the half that is
/// still happening.
#[tokio::test]
async fn a_finished_answer_keeps_a_key_whose_import_is_running() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    claim(&runtime, key).await;

    runtime.end_identification_answer(key, run(1)).await.unwrap();

    let recorded = runtime.snapshot(key).await.unwrap().expect("the claim keeps the key");
    assert!(recorded.identification.is_none());
    assert!(recorded.import.is_some());
}

/// A cancel ends whatever run or answer the key holds and what every run of
/// it published, and leaves its wait.
#[tokio::test]
async fn a_cancel_ends_the_run_and_what_it_published() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime.cancel_identification(key).await.unwrap();
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

// What an import's claim on a candidate records, and what it refuses.

#[tokio::test]
async fn a_second_claim_on_an_owned_candidate_is_refused_and_changes_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    let claimed = runtime.snapshot(key).await.unwrap();

    assert!(matches!(
        runtime.claim_for_import(key, "imp-2", false).await,
        Err(crate::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(runtime.snapshot(key).await.unwrap(), claimed);
}

/// The refusal is the claim's row: a runtime that holds nothing of the first
/// claim — another one over the same library — is refused all the same.
#[tokio::test]
async fn a_claim_is_refused_by_the_claim_the_table_holds() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();

    let holding_nothing = runtime.over_the_same_library_for_test();
    assert!(matches!(
        holding_nothing.claim_for_import(key, "imp-2", false).await,
        Err(crate::ImportError::CandidateImportInProgress)
    ));
    assert!(holding_nothing.import_claimed(key).await.unwrap());
}

#[tokio::test]
async fn a_claim_is_the_queued_step_until_the_worker_takes_it_up() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");

    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        Some(ImportInFlight { step: ImportStep::Preparing(PrepareStep::Queued) })
    );

    runtime.move_import(key, "imp-1", None).await.unwrap();
    assert!(
        runtime.snapshot(key).await.unwrap().is_none(),
        "with nothing else running, releasing the claim empties the key"
    );
}

/// Only the import holding a claim moves or ends it, and a report from an
/// import that already ended moves nothing a reader draws.
#[tokio::test]
async fn only_the_claiming_import_moves_its_claim() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;
    measuring(&runtime, key, 60).await;
    runtime.move_import(key, "imp-1", None).await.unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Cancelled { import_id: "imp-1".to_string() },
    });

    runtime.claim_for_import(key, "imp-2", false).await.unwrap();
    let queued = Some(ImportInFlight { step: ImportStep::Preparing(PrepareStep::Queued) });
    runtime.record_event(&progress(key, 80));
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        queued,
        "the ended import's report does not move the new claim"
    );
    assert!(runtime.percents().is_empty(), "nor is the ended import's percent drawn");
    assert!(
        runtime
            .move_import(key, "imp-1", Some(ImportStep::Running(ImportPhase::ReadingFiles)),)
            .await
            .is_err(),
        "nor may the ended import move it"
    );
    assert!(runtime.move_import(key, "imp-1", None).await.is_err());
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        queued,
        "nor end it"
    );
}

/// The scan dropping a candidate an import holds — the folder removed, read
/// with other files, or found invalid — does not end the import. The claim
/// stays until the import's own ending, so no second import takes the key
/// meanwhile.
#[tokio::test]
async fn an_import_outlives_its_candidate_leaving_the_scan() {
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let mut reshaped = folder_candidate(key, "/watch/a");
    reshaped.file_edit_revision = 1;
    let departures = [
        ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key.clone() }),
        scanned(reshaped),
        ImportEvent::Scan(ScanEvent::InvalidCandidate(InvalidCandidate {
            path: key.test_folder(),
            name: "rel1".to_string(),
            watched_folder_path: std::path::PathBuf::from("/watch/a"),
            relative_folder: bae_db::RelativeFolder::test("rel1"),
            grouping: None,
            files: Vec::new(),
            tags: None,
        })),
    ];
    for departure in departures {
        let runtime = CandidateRuntime::for_test().await;
        runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
        claim(&runtime, key).await;
        runtime.record_event(&progress(key, 42));

        runtime.record_event(&departure);
        assert!(runtime.import_claimed(key).await.unwrap(), "{departure:?}");
        assert!(
            matches!(
                runtime.claim_for_import(key, "imp-2", false).await,
                Err(crate::ImportError::CandidateImportInProgress)
            ),
            "{departure:?} let a second import claim the key"
        );

        runtime.move_import(key, "imp-1", None).await.unwrap();
        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.clone(),
            progress: ImportProgress::Failed {
                error: "the files changed".to_string(),
                import_id: "imp-1".to_string(),
            },
        });
        assert!(runtime.snapshot(key).await.unwrap().is_none(), "{departure:?}");
    }
}

/// Claiming a candidate for an import ends whatever run or answer its
/// identification holds, in the claim's own write.
#[tokio::test]
async fn a_claim_that_ends_identification_ends_it_in_its_own_write() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    report(&runtime, key, &files, run(1), triangulating()).await;

    runtime.claim_for_import(key, "imp-1", true).await.unwrap();
    let facts = runtime.standing_facts(key).await.unwrap();
    assert_eq!(facts.identification, None);
    assert_eq!(facts.import, Some(ImportStep::Preparing(PrepareStep::Queued)));
    assert_eq!(runtime.identification_progress().await.unwrap(), (0, 0));
}

// ── The identification count ────────────────────────────────────────────────
//
// What the count does over the writes of each candidate's identification.

/// The count as the sidebar reads it, if it moved since the last read.
async fn counts(runtime: &CandidateRuntime, last: &mut (u32, u32)) -> Vec<(u32, u32)> {
    let now = runtime.identification_progress().await.unwrap();
    if now == *last {
        return Vec::new();
    }
    *last = now;
    vec![now]
}

/// A Lookup a person starts is an identification like any other: it opens a
/// batch on its own, without a sweep pass behind it.
#[tokio::test]
async fn a_lookup_started_by_hand_opens_a_batch() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 1)]);

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(
        counts(&runtime, &mut last).await,
        Vec::new(),
        "the run starting is the same identification, still waited on"
    );

    assert!(runtime.tables.publish_candidate_run(key, run(1), &manual_only(), None).await.unwrap());
    runtime.run_answered(key, run(1), false).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last).await,
        Vec::new(),
        "and so is the answer waiting on its write"
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last).await,
        vec![(0, 0)],
        "the write landing ends the only identification, and the batch with it"
    );
}

/// The sweep's queue is a batch: what it publishes is admitted, and each
/// verdict advances the count until the last one ends it.
#[tokio::test]
async fn the_sweeps_queue_is_counted_and_drains_to_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    let first_files = scanned_key(&runtime, first).await;
    let second_files = scanned_key(&runtime, second).await;

    runtime
        .admit(
            vec![(first.clone(), first_files.clone()), (second.clone(), second_files.clone())],
            Admission::Automatic,
        )
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 2)]);

    report(&runtime, first, &first_files, run(1), manual_only()).await;
    runtime.end_identification_answer(first, run(1)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(1, 2)]);

    report(&runtime, second, &second_files, run(2), manual_only()).await;
    runtime.end_identification_answer(second, run(2)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 0)]);
}

/// A key the sweep drops before it runs is an identification that is over —
/// nothing is going to answer it, and the count must not wait on it for ever.
#[tokio::test]
async fn a_key_dropped_from_the_queue_before_it_ran_is_over() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    let marks = vec![
        (first.clone(), scanned_key(&runtime, first).await),
        (second.clone(), scanned_key(&runtime, second).await),
    ];

    runtime.admit(marks, Admission::Automatic).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 2)]);

    runtime.withdraw(second).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last).await,
        vec![(1, 2)],
        "the dropped key is counted as ended, not taken out of the total"
    );

    runtime.withdraw(first).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 0)]);
}

/// A write that failed is a run that is over: the row says why, and nothing is
/// waiting on a write that is not coming.
#[tokio::test]
async fn a_failed_write_ends_the_identification() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 1)]);

    runtime.fail_identification(key, run(1), &files, not_written("disk is full")).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 0)]);
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::FinalizationFailed {
            failure: not_written("disk is full"),
        }),
        "the failure stays on the row after the batch is over"
    );
}

/// A run cancelled because its candidate left the scan ends its
/// identification with its end.
#[tokio::test]
async fn a_cancelled_run_s_end_ends_its_identification() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 1)]);

    runtime.run_ended(key, run(1)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 0)]);
}

/// A second batch counts only its own work — the first one's total described
/// identifications that are over.
#[tokio::test]
async fn a_batch_after_a_drain_starts_from_zero() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    let first_files = scanned_key(&runtime, first).await;
    let second_files = scanned_key(&runtime, second).await;

    runtime
        .admit(
            vec![(first.clone(), first_files.clone()), (second.clone(), second_files)],
            Admission::Automatic,
        )
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 2)]);
    runtime.withdraw(first).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(1, 2)]);
    runtime.withdraw(second).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 0)]);

    runtime.admit(vec![(first.clone(), first_files)], Admission::Requested).await.unwrap();
    assert_eq!(counts(&runtime, &mut last).await, vec![(0, 1)]);
}

/// A library release being re-identified in its own sheet is not the import
/// queue's work: nothing admits it, its run reaches the sheet through the
/// runtime, and the count never picks it up.
#[tokio::test]
async fn a_release_re_identified_in_its_own_sheet_is_not_counted() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = &crate::CandidateKey::reidentify("release-1");

    runtime.tables.publish_release_run(key, run(1), &triangulating(), None).await.unwrap();
    runtime.tables.publish_release_run(key, run(1), &manual_only(), None).await.unwrap();
    assert!(
        runtime.snapshot(key).await.unwrap().is_some(),
        "the sheet reads its run off the runtime"
    );
    assert_eq!(counts(&runtime, &mut last).await, Vec::new());
}

// ── A write of a standing that fails ────────────────────────────────────────

/// Make every write that would store `stage` as a candidate's identification
/// fail, as a full disk fails it.
async fn fail_writes_of(runtime: &CandidateRuntime, stage: &str) {
    let when = format!("NEW.identification = '{stage}'");
    fail_standing_writes(
        runtime._test_directory.as_ref().expect("a test runtime keeps its directory").path(),
        &[("INSERT", when.as_str()), ("UPDATE", when.as_str())],
    )
    .await;
}

/// A run's start that cannot be written fails to the driver that reported
/// it, and the table, the count and what the runtime draws stay as they were.
#[tokio::test]
async fn a_start_that_cannot_be_written_fails_and_leaves_the_table_as_it_was() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    let before = runtime.stored_standing_for_test(key).await;
    fail_writes_of(&runtime, "running").await;

    let error = runtime.run_started(key, &files, run(1), &triangulating()).await.unwrap_err();
    assert!(error.to_string().contains("the disk is full"), "{error}");

    assert_eq!(runtime.stored_standing_for_test(key).await, before);
    assert_eq!(runtime.identification_progress().await.unwrap(), (0, 1));
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Queued(Admission::Requested))
    );
}

/// An admission that cannot be written fails whole: none of its candidates
/// is marked, and the count does not open.
#[tokio::test]
async fn an_admission_that_cannot_be_written_marks_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    let marks = vec![
        (first.clone(), scanned_key(&runtime, first).await),
        (second.clone(), scanned_key(&runtime, second).await),
    ];
    fail_writes_of(&runtime, "queued").await;

    let error = runtime.admit(marks, Admission::Automatic).await.unwrap_err();
    assert!(error.to_string().contains("the disk is full"), "{error}");

    assert!(runtime.snapshots().await.unwrap().is_empty());
    assert_eq!(runtime.identification_progress().await.unwrap(), (0, 0));
}

// ── An answer whose pick is being read ──────────────────────────────────────

/// A found answer over `pressings` (each release's row) whose lead lists two
/// tracks, for a folder of two.
fn found_over(pressings: Vec<u32>) -> bae_identify::IdentifyState {
    let matches = (0..pressings.len())
        .map(|index| bae_catalogs::search::MetadataResult {
            source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 2 }),
            ..bae_catalogs::search::MetadataResult::for_test(
                LookupCatalog::MusicBrainz,
                &format!("mb-{index}"),
                Some("rg-1"),
            )
        })
        .collect();
    bae_identify::IdentifyState::Found {
        findings: bae_identify::Findings { matches, pressings, ..Default::default() },
        track_count: 2,
        ledger: None,
        context: signals_context(2),
    }
}

/// An answer that picks its one pressing unattended reads as its pick being
/// read, and an Import asked of it rides on it until the answer is disposed
/// of, which hands the ask back.
#[tokio::test]
async fn an_answer_that_picks_reads_as_its_pick_being_read_and_carries_an_import_ask() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), found_over(vec![0])).await;
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::ReadingPick { import_asked: false })
    );

    assert!(runtime.ask_import_once_stored(key).await.unwrap());
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::ReadingPick { import_asked: true })
    );
    assert!(runtime.ask_import_once_stored(key).await.unwrap(), "asking again asks the same thing");

    assert!(
        runtime.end_identification_answer(key, run(1)).await.unwrap(),
        "the write that ends the answer is handed the ask"
    );
    assert_eq!(identification(&runtime, key).await, None);
    assert!(
        !runtime.ask_import_once_stored(key).await.unwrap(),
        "with no answer held there is nothing to ask of"
    );
}

/// An answer that picks nothing reads as finalizing, and an Import is not
/// asked of it.
#[tokio::test]
async fn an_answer_that_picks_nothing_reads_as_finalizing_and_takes_no_ask() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), found_over(vec![0, 1])).await;
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::Finalizing),
        "two pressings are the person's to pick from"
    );

    assert!(!runtime.ask_import_once_stored(key).await.unwrap());
    assert_eq!(identification(&runtime, key).await, Some(crate::IdentificationStatus::Finalizing));
    assert!(!runtime.end_identification_answer(key, run(1)).await.unwrap());
}

/// The ask is the answer's: an older run's ending hands back nothing, and a
/// failed write drops it.
#[tokio::test]
async fn an_import_ask_goes_with_the_answer_it_was_asked_of() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(2), found_over(vec![0])).await;
    assert!(runtime.ask_import_once_stored(key).await.unwrap());

    assert!(
        !runtime.end_identification_answer(key, run(1)).await.unwrap(),
        "an older run's answer is not the one held"
    );

    runtime
        .fail_identification(key, run(2), &files, not_written("database write failed"))
        .await
        .unwrap();
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::IdentificationStatus::FinalizationFailed {
            failure: not_written("database write failed"),
        })
    );
    assert!(!runtime.end_identification_answer(key, run(2)).await.unwrap());
}
