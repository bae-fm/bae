//! What each thing that happens to a candidate does to the queue: a run's
//! state, an answer settled, a scan's report, a decision a person made.

use super::*;

/// Apply one import event. Reports whether the loop carries on.
pub(super) async fn handle_event(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    event: Option<Result<ImportEvent, broadcast::error::RecvError>>,
) -> bool {
    match event {
        Some(Ok(ImportEvent::IdentifyStateChanged {
            candidate_key,
            run,
            state,
            ..
        })) => {
            advance(
                context,
                queue,
                config,
                settle_token,
                settling,
                &candidate_key,
                run,
                state,
            )
            .await;
        }
        // A scan finished: whatever it found, and whatever it took away, the
        // automatic admission reads the queue afresh.
        Some(Ok(ImportEvent::Scan(ScanEvent::Finished))) => {
            if automatic_is_on(config) {
                admit_automatically(context, queue).await;
            }
            // A launch's first scan is where an import owed before the app
            // last closed is found again.
            pay_owed_imports(context, config).await;
        }
        // A scan announces every candidate it walks, including ones the queue
        // is not responsible for — skipped, already in the library, or claimed
        // by an import that started since. Whether this is one of ours is
        // `answerable_candidate`'s question and nobody else's: the event's own
        // flags answer a narrower one, and re-deriving the answer here is what
        // let a re-scan count an importing candidate back into a total the
        // import had just taken it out of.
        Some(Ok(ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }))) => {
            let key = candidate.path.to_string_lossy().into_owned();
            observe(context, queue, config, &key).await;
        }
        // The folder is a different shape now, or a person decided the
        // candidate: they picked a release, said file metadata, or cleared what it
        // had. The command that decided ended its run as part of its own write.
        // What the decision left is what the queue takes the candidate as: a
        // pick answers it, and a clear puts it back on the queue rather than
        // leaving it for the next scan.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))) => {
            let key = candidate.path.to_string_lossy().into_owned();
            reconsider(context, queue, config, &key).await;
        }
        Some(Ok(ImportEvent::Scan(
            ScanEvent::CandidateMetadataChanged { candidate_key }
            | ScanEvent::CandidateSkipChanged { candidate_key, .. },
        ))) => {
            reconsider(context, queue, config, &candidate_key).await;
        }
        // The folder was removed, renamed or unmounted, or an import has taken
        // the candidate. Extraction is cancelled by the signal service's own
        // listener, so no further `Signals` will ever arrive and a driver left
        // running would sit in `Triangulating` forever, holding a slot that
        // never frees.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })))
        | Some(Ok(ImportEvent::ImportProgress { candidate_key, .. })) => {
            queue.withdraw(context, &candidate_key);
        }
        Some(Ok(_)) => {}
        Some(Err(broadcast::error::RecvError::Lagged(n))) => {
            // A dropped `IdentifyStateChanged` would leave its job running with
            // nothing left to wake it, so the queue would stall on a slot that
            // never frees. Give the affected candidates back to the queue and
            // run them again: nothing durable was written, so replaying them
            // whole is the only shape that cannot leave a wrong answer behind.
            warn!("identification: import bus lagged by {n} events; replaying what was running");
            context
                .library_manager
                .record_telemetry(crate::diagnostics::TelemetryEvent::Anomaly {
                    kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                });
            queue.replay_running(context);
            if automatic_is_on(config) {
                admit_automatically(context, queue).await;
            }
            pay_owed_imports(context, config).await;
        }
        Some(Err(broadcast::error::RecvError::Closed)) | None => {
            info!("identification: the import event stream closed");
            return false;
        }
    }
    true
}

/// One state of the run answering a job.
#[allow(clippy::too_many_arguments)]
async fn advance(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    key: &str,
    run: IdentifyRunId,
    state: IdentifyState,
) {
    let Some(index) = queue.running_job(key, run) else {
        return;
    };
    // The run ended with no answer: something decided the candidate, or took
    // it away. The queue takes every member as it stands now rather than
    // dropping the group — what ended the run says nothing about the others.
    if matches!(state, IdentifyState::Idle) {
        let job = queue.jobs.remove(index).expect("the located job exists");
        for member_key in job.keys() {
            context.import.withdraw_identification(&member_key);
            readmit(context, queue, config, &member_key).await;
        }
        return;
    }
    // Terminal means the machine stopped moving, including on an explicit
    // failure verdict. Either way the candidate's slot is free now.
    if !state.is_terminal() {
        return;
    }
    let job = &mut queue.jobs[index];
    let identity = job.identity.clone();
    let priority = job.priority();
    // Settling is where a run's answer becomes the candidate's, so this is
    // where it is decided whether the answer owes an import: only the
    // automatic admission's own run, only while the setting is on.
    let owes_import = job.admission() == Admission::Automatic && imports_when_identified(config);
    let JobState::Running {
        expected_metadata_revision,
        ..
    } = job.state
    else {
        unreachable!("the job this run was located by is the running one");
    };
    // A member leaving takes its job's run with it, so a job still running for
    // this key still holds it.
    let candidate = job
        .members
        .iter()
        .find(|member| member.candidate.key() == key)
        .map(|member| member.candidate.clone())
        .expect("a job's running representative is one of its members");
    // The run ended on its own, so there is no driver left to cancel — only
    // the extraction that fed it, whose artwork pass would otherwise keep
    // going beside the settle.
    context.import.cancel_candidate_extraction(key);
    let settle_token = settle_token.child_token();
    job.state = JobState::Settling {
        representative: key.to_string(),
        run,
        abandon: settle_token.clone(),
    };
    let context = context.clone();
    settling.spawn(async move {
        settle_answer(
            context,
            identity,
            candidate,
            run,
            expected_metadata_revision,
            state,
            priority,
            owes_import,
            settle_token,
        )
        .await
    });
}

/// What one settled answer does to the job it answered.
pub(super) async fn finish(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    done: Finished,
) {
    let Some(index) = queue.settling_job(&done.identity, &done.representative_key, done.run) else {
        debug!(
            "identification: {} settled for a job the queue has moved off",
            done.representative_key
        );
        return;
    };
    let job = queue.jobs.remove(index).expect("the located job exists");
    let keys = job.keys();
    match done.settled {
        Settled::Stored => {
            info!(
                "identification: stored the verdict for {}",
                done.representative_key
            );
            for key in &keys {
                context.import.withdraw_identification(key);
            }
            // The verdict is stored and nothing is running for it any more,
            // so whatever it owes can be paid now.
            pay_owed_import(context, config, &done.representative_key).await;
        }
        // Nothing was stored and nothing failed: the candidate changed while
        // its answer was being written, or the answer was given up. Every
        // member is taken as it stands now, on the admission it was on: a
        // person who asked is still owed an answer, and waiting for the
        // automatic admission would leave them none when it is off.
        Settled::Refused | Settled::Abandoned => {
            info!(
                "identification: {} stored no answer; re-reading {} candidate(s) for it",
                done.representative_key,
                keys.len()
            );
            for member in &job.members {
                let key = member.candidate.key();
                context.import.withdraw_identification(&key);
                match member.admission {
                    Admission::Requested => ask_again(context, queue, &key).await,
                    Admission::Automatic => readmit(context, queue, config, &key).await,
                }
            }
        }
        Settled::WriteFailed { error } | Settled::Unwritable { error } => {
            warn!(
                "identification: could not store the answer for {} ({error})",
                done.representative_key
            );
            for key in &keys {
                context.import.withdraw_identification(key);
                // The representative's own failure is already on its row — the
                // write's, or the settle's when no write ran — and saying it
                // again says the same thing. Its group learns it here.
                context
                    .import
                    .fail_identification(key, done.run, error.clone());
            }
        }
    }
}

/// Take the candidate at `key` as it stands right now: off the queue, read
/// afresh, and back on it if it still wants an answer.
///
/// What an event that changes what a candidate *is* resolves to, and the same
/// resolution for each of them: a decision made about it, a skip lifted or
/// applied, a binding changed.
async fn reconsider(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    // What changed may be what its verdict owed an import for: a decision
    // about the candidate is the person's, and a candidate set aside is not
    // imported behind them.
    pay_owed_import(context, config, key).await;
    let Some(candidate) = answerable_candidate(context, key).await else {
        // Not the queue's any more: set aside, already in the library, claimed
        // by an import, or gone.
        queue.withdraw(context, key);
        return;
    };
    // A candidate a person asked for is theirs until their run ends: what
    // changed here is not what they asked about, and the write of their answer
    // checks for itself that the candidate still stands where it read it.
    if queue.requested(key) {
        return;
    }
    queue.withdraw(context, key);
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// The same, for a scan re-announcing a candidate: one already being answered
/// as exactly this shape is left alone, because taking it back would cancel the
/// run answering it.
async fn observe(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        queue.withdraw(context, key);
        return;
    };
    if queue.holds(key, &candidate_identity(&candidate)) {
        return;
    }
    queue.withdraw(context, key);
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// Read `key` afresh and put it back on the queue as the request it was, if an
/// answer can still be stored for it.
async fn ask_again(context: &Context, queue: &mut Queue, key: &str) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        return;
    };
    admit(context, queue, vec![candidate], Admission::Requested).await;
}

/// Read `key` afresh and put it back on the queue if it still wants an answer.
async fn readmit(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        return;
    };
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// Put `candidate` on the queue if its row states no result for the files it
/// has right now.
///
/// Always as an automatic admission: a request is one act, and once the run it
/// asked for is over the person's latest word is whatever decided the candidate
/// since.
async fn admit_as_it_stands(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    candidate: FolderCandidate,
) {
    if !automatic_is_on(config) {
        return;
    }
    if wants_an_answer(context, &candidate).await {
        admit(context, queue, vec![candidate], Admission::Automatic).await;
    }
}
