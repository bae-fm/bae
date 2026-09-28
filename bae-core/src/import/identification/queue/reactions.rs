//! What each event does to the queue. Only a found release puts a candidate
//! the queue does not hold on it.

use super::*;

/// Apply `events` in the order they were sent. The releases found among them
/// are one admission, so the count opens at their total; it goes ahead of any
/// later event about one of them.
pub(super) async fn handle_events(
    context: &Context,
    queue: &mut Queue,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    events: Vec<ImportEvent>,
) {
    let mut found: Vec<String> = Vec::new();
    for event in events {
        match event {
            ImportEvent::Scan(ScanEvent::FolderCandidate {
                candidate,
                found_while_automatic: true,
                ..
            }) => {
                let key = candidate.key();
                if !found.contains(&key) {
                    found.push(key);
                }
            }
            event => {
                if subject(&event).is_some_and(|key| found.contains(&key)) {
                    admit_found_releases(context, queue, std::mem::take(&mut found)).await;
                }
                handle_event(context, queue, settle_token, settling, event).await;
            }
        }
    }
    admit_found_releases(context, queue, found).await;
}

/// The candidate `event` is about, when it names one.
fn subject(event: &ImportEvent) -> Option<String> {
    match event {
        ImportEvent::ImportProgress { candidate_key, .. }
        | ImportEvent::IdentifyStateChanged { candidate_key, .. }
        | ImportEvent::SignalsUpdated { candidate_key, .. }
        | ImportEvent::Scan(
            ScanEvent::CandidateRemoved { candidate_key }
            | ScanEvent::CandidateSkipChanged { candidate_key, .. }
            | ScanEvent::CandidateMetadataChanged { candidate_key },
        ) => Some(candidate_key.clone()),
        ImportEvent::Scan(
            ScanEvent::FolderCandidate { candidate, .. }
            | ScanEvent::CandidateDiscovered { candidate, .. }
            | ScanEvent::CandidateBindingChanged { candidate },
        ) => Some(candidate.key()),
        ImportEvent::Scan(ScanEvent::InvalidCandidate(candidate)) => Some(candidate.key()),
        ImportEvent::Scan(
            ScanEvent::WatchedFoldersChanged { .. }
            | ScanEvent::FolderScanStatusChanged { .. }
            | ScanEvent::Finished,
        ) => None,
    }
}

/// Queue the releases at `keys`, then follow each as any announced candidate
/// is followed.
async fn admit_found_releases(context: &Context, queue: &mut Queue, keys: Vec<String>) {
    if keys.is_empty() {
        return;
    }
    admit_found(context, queue, keys.clone()).await;
    for key in &keys {
        follow(context, queue, key).await;
    }
}

/// Apply one import event.
async fn handle_event(
    context: &Context,
    queue: &mut Queue,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    event: ImportEvent,
) {
    match event {
        ImportEvent::IdentifyStateChanged {
            candidate_key,
            run,
            state,
            ..
        } => {
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
        ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }) => {
            follow(context, queue, &candidate.key()).await;
        }
        // A person's decision about the candidate ends its identification.
        ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }) => {
            queue.withdraw(context, &candidate.key());
        }
        ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }) => {
            queue.withdraw(context, &candidate_key);
        }
        ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
            candidate_key,
            skipped: true,
        }) => {
            queue.withdraw(context, &candidate_key);
        }
        // Gone, or taken by an import: a run left going would never finish.
        ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })
        | ImportEvent::ImportProgress { candidate_key, .. } => {
            queue.withdraw(context, &candidate_key);
        }
        _ => {}
    }
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
        Settled::Stored { picked_unattended } => {
            info!(
                "identification: stored the verdict for {}",
                done.representative_key
            );
            if job.admission() == Admission::Automatic && picked_unattended {
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

/// Import `key`, whose automatic run just stored an auto-importable verdict, when "Import
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
/// when they changed while it waited. A run reading the old files was ended
/// when the runtime recorded the change, and its ending takes the key off
/// the queue; an answer already being written is refused for files the
/// candidate no longer has.
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
