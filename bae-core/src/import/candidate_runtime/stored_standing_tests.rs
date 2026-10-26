//! Each owner's change to a candidate's identification, decided from the one
//! stored.

use super::*;

fn files(revision: u64) -> CandidateFiles {
    CandidateFiles {
        content_hash: "hash-1".to_string(),
        file_edit_revision: revision,
    }
}

fn run(number: u64) -> IdentifyRunId {
    IdentifyRunId::for_test(number)
}

fn at(stage: IdentificationStage) -> Option<StoredIdentification> {
    Some(StoredIdentification {
        files: files(0),
        stage,
    })
}

fn queued() -> Option<StoredIdentification> {
    at(IdentificationStage::Queued {
        admission: Admission::Automatic,
    })
}

fn failed() -> Option<StoredIdentification> {
    at(IdentificationStage::Failed {
        failure: SaveFailure::NotWritten {
            error: "the disk is full".to_string(),
        },
    })
}

/// An admission waits for a run whatever stood before it.
#[test]
fn an_admission_waits_over_whatever_stood() {
    let admit = IdentificationChange::Admit {
        files: files(1),
        admission: Admission::Requested,
    };
    for stored in [
        None,
        queued(),
        at(IdentificationStage::Finalizing { run: run(1) }),
        failed(),
    ] {
        assert_eq!(
            admit.applied(stored.as_ref()),
            Some(StoredIdentification {
                files: files(1),
                stage: IdentificationStage::Queued {
                    admission: Admission::Requested,
                },
            })
        );
    }
}

/// A run starts only from the wait for the files it reads: a run that lost
/// that wait reports into nothing.
#[test]
fn a_run_starts_only_from_the_wait_for_its_files() {
    let started = IdentificationChange::RunStarted {
        files: files(0),
        run: run(2),
    };
    assert_eq!(
        started.applied(queued().as_ref()),
        at(IdentificationStage::Running { run: run(2) })
    );
    for stored in [
        None,
        failed(),
        at(IdentificationStage::Running { run: run(3) }),
        Some(StoredIdentification {
            files: files(1),
            stage: IdentificationStage::Queued {
                admission: Admission::Automatic,
            },
        }),
    ] {
        assert_eq!(started.applied(stored.as_ref()), stored);
    }
}

/// A run's answer, its end and its write's failure each move that run alone.
#[test]
fn a_run_s_changes_move_that_run_alone() {
    let running = at(IdentificationStage::Running { run: run(1) });
    let other = at(IdentificationStage::Running { run: run(2) });

    let answered = IdentificationChange::RunAnswered {
        run: run(1),
        reads_pick: true,
    };
    assert_eq!(
        answered.applied(running.as_ref()),
        at(IdentificationStage::ReadingPick {
            run: run(1),
            import_asked: false,
        })
    );
    assert_eq!(answered.applied(other.as_ref()), other);
    assert_eq!(
        IdentificationChange::RunAnswered {
            run: run(1),
            reads_pick: false,
        }
        .applied(running.as_ref()),
        at(IdentificationStage::Finalizing { run: run(1) })
    );

    let ended = IdentificationChange::RunEnded { run: run(1) };
    assert_eq!(ended.applied(running.as_ref()), None);
    assert_eq!(ended.applied(other.as_ref()), other);
    assert_eq!(
        ended.applied(at(IdentificationStage::Finalizing { run: run(1) }).as_ref()),
        at(IdentificationStage::Finalizing { run: run(1) }),
        "a run's end without an answer leaves its answer being written"
    );
}

/// An answer ends whether its stage was recorded or not, and only its own.
#[test]
fn an_answer_ends_its_own_run_whatever_stage_it_reached() {
    let ended = IdentificationChange::AnswerEnded { run: run(1) };
    for stored in [
        at(IdentificationStage::Running { run: run(1) }),
        at(IdentificationStage::Finalizing { run: run(1) }),
        at(IdentificationStage::ReadingPick {
            run: run(1),
            import_asked: true,
        }),
    ] {
        assert_eq!(ended.applied(stored.as_ref()), None);
    }
    for stored in [
        queued(),
        failed(),
        at(IdentificationStage::Finalizing { run: run(2) }),
    ] {
        assert_eq!(ended.applied(stored.as_ref()), stored);
    }
}

/// A failure replaces its own answer, an earlier failure, or nothing; a
/// later identification keeps standing.
#[test]
fn a_failure_never_replaces_a_later_identification() {
    let failure = SaveFailure::Inapplicable {
        error: "a track has no title".to_string(),
    };
    let failed_now = IdentificationChange::AnswerFailed {
        files: files(0),
        run: run(1),
        failure: failure.clone(),
    };
    let expected = at(IdentificationStage::Failed { failure });
    for stored in [
        None,
        failed(),
        at(IdentificationStage::Finalizing { run: run(1) }),
    ] {
        assert_eq!(failed_now.applied(stored.as_ref()), expected);
    }
    for stored in [queued(), at(IdentificationStage::Running { run: run(2) })] {
        assert_eq!(failed_now.applied(stored.as_ref()), stored);
    }
}

/// An Import is asked only of an answer that reads its pick, and goes with
/// it.
#[test]
fn an_import_is_asked_only_of_an_answer_reading_its_pick() {
    let reading = at(IdentificationStage::ReadingPick {
        run: run(1),
        import_asked: false,
    });
    let asked = at(IdentificationStage::ReadingPick {
        run: run(1),
        import_asked: true,
    });
    assert_eq!(
        IdentificationChange::ImportAsked.applied(reading.as_ref()),
        asked
    );
    let finalizing = at(IdentificationStage::Finalizing { run: run(1) });
    assert_eq!(
        IdentificationChange::ImportAsked.applied(finalizing.as_ref()),
        finalizing
    );
    let ended = IdentificationMove {
        before: asked.clone(),
        after: IdentificationChange::AnswerEnded { run: run(1) }.applied(asked.as_ref()),
    };
    assert!(ended.ended_with_import_asked());
}

/// A cancel ends a run or an answer, and leaves a wait or a failure.
#[test]
fn a_cancel_ends_a_run_or_an_answer_and_nothing_else() {
    let cancelled = IdentificationChange::Cancelled;
    for stored in [
        at(IdentificationStage::Running { run: run(1) }),
        at(IdentificationStage::Finalizing { run: run(1) }),
    ] {
        assert_eq!(cancelled.applied(stored.as_ref()), None);
    }
    for stored in [queued(), failed()] {
        assert_eq!(cancelled.applied(stored.as_ref()), stored);
    }
    assert_eq!(IdentificationChange::Withdraw.applied(queued().as_ref()), None);
    assert_eq!(
        IdentificationChange::Withdraw.applied(failed().as_ref()),
        failed()
    );
}
