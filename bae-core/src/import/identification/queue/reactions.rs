//! What each event does to the work already on the queue. None of them puts a
//! candidate the queue does not hold on it.

use super::*;

/// Apply one import event. Reports whether the loop carries on.
pub(super) async fn handle_event(
    context: &Context,
    queue: &mut Queue,
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
                settle_token,
                settling,
                &candidate_key,
                run,
                state,
            )
            .await;
        }
        Some(Ok(ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }))) => {
            follow(context, queue, &candidate.key()).await;
        }
        // A person's decision about the candidate ends its identification.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))) => {
            queue.withdraw(context, &candidate.key());
        }
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }))) => {
            queue.withdraw(context, &candidate_key);
        }
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
            candidate_key,
            skipped,
        }))) => {
            if skipped {
                queue.withdraw(context, &candidate_key);
            }
        }
        // Gone, or taken by an import: a run left going would never finish.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })))
        | Some(Ok(ImportEvent::ImportProgress { candidate_key, .. })) => {
            queue.withdraw(context, &candidate_key);
        }
        Some(Ok(_)) => {}
        Some(Err(broadcast::error::RecvError::Lagged(n))) => {
            // A dropped run state would hold its slot forever, so every running
            // job starts over.
            warn!("identification: import bus lagged by {n} events; replaying what was running");
            context
                .library_manager
                .record_telemetry(crate::diagnostics::TelemetryEvent::Anomaly {
                    kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                });
            queue.replay_running(context);
        }
        Some(Err(broadcast::error::RecvError::Closed)) | None => {
            info!("identification: the import event stream closed");
            return false;
        }
    }
    true
}

/// One state of the run answering a job.
async fn advance(
    context: &Context,
    queue: &mut Queue,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    key: &str,
    run: IdentifyRunId,
    state: IdentifyState,
) {
    let Some(index) = queue.running_job(key, run) else {
        return;
    };
    // Something outside the queue ended the run, which ends this candidate's
    // identification; the rest of its job waits for a run of its own.
    if matches!(state, IdentifyState::Idle) {
        queue.withdraw(context, key);
        return;
    }
    if !state.is_terminal() {
        return;
    }
    let job = &mut queue.jobs[index];
    let identity = job.identity.clone();
    let priority = job.priority();
    let candidate = job
        .members
        .iter()
        .find(|member| member.candidate.key() == key)
        .map(|member| member.candidate.clone())
        .expect("a job's running representative is one of its members");
    // The run ended on its own; only its extraction may still be working.
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
            state,
            priority,
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
    for key in &keys {
        context.import.withdraw_identification(key);
    }
    match done.settled {
        Settled::Stored { classification } => {
            info!(
                "identification: stored the verdict for {}",
                done.representative_key
            );
            if job.admission() == Admission::Automatic
                && classification == crate::identify::QueueClassification::Ready
            {
                import_when_identified(context, config, &done.representative_key).await;
            }
        }
        // Nothing was stored, and nothing runs again on its own.
        Settled::Refused | Settled::Abandoned => {
            info!(
                "identification: {} stored no answer; its job is over",
                done.representative_key
            );
        }
        Settled::WriteFailed { error } | Settled::Unwritable { error } => {
            warn!(
                "identification: could not store the answer for {} ({error})",
                done.representative_key
            );
            for key in &keys {
                context
                    .import
                    .fail_identification(key, done.run, error.clone());
            }
        }
    }
}

/// Import `key`, whose automatic run just stored a Ready verdict, when "Import
/// automatically when identified" is on.
async fn import_when_identified(
    context: &Context,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    if !config
        .borrow()
        .prefs
        .identification
        .imports_when_identified()
    {
        return;
    }
    match context.import.import_identified(key).await {
        Ok(import_id) => info!("identification: importing {key} automatically as {import_id}"),
        Err(error) => {
            warn!("identification: the automatic import of {key} could not start: {error}")
        }
    }
}

/// Follow a candidate the queue holds to what is stored for it now: off the
/// queue when it can no longer be answered, and placed again as its new files
/// when they changed while it waited. A run already reading the old files is
/// left to finish; its write refuses an answer for files the candidate no
/// longer has.
async fn follow(context: &Context, queue: &mut Queue, key: &str) {
    let Some(held) = queue.held(key) else {
        return;
    };
    let Some(candidate) = answerable_candidate(context, key).await else {
        queue.withdraw(context, key);
        return;
    };
    if !held.running && candidate_identity(&candidate) != held.identity {
        admit(context, queue, vec![candidate], held.admission).await;
    }
}
