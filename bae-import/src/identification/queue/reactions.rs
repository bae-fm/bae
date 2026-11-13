//! What each event does to the queue. Only a found release puts a candidate
//! the queue does not hold on it.

use super::*;
use crate::ImportProgress;

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
    let mut found: Vec<crate::CandidateKey> = Vec::new();
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
fn subject(event: &ImportEvent) -> Option<crate::CandidateKey> {
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
        ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { .. } | ScanEvent::Finished) => None,
    }
}

/// Queue the releases at `keys`, then follow each as any announced candidate
/// is followed.
async fn admit_found_releases(
    context: &Context,
    queue: &mut Queue,
    keys: Vec<crate::CandidateKey>,
) {
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
        ImportEvent::IdentifyStateChanged { candidate_key, run, state, .. } => {
            advance(context, queue, settle_token, settling, &candidate_key, run, state).await;
        }
        ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }) => {
            follow(context, queue, &candidate.key()).await;
        }
        // A person's decision about the candidate ends its identification.
        ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }) => {
            queue.withdraw(context, &candidate.key()).await;
        }
        ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }) => {
            queue.withdraw(context, &candidate_key).await;
        }
        ImportEvent::Scan(ScanEvent::CandidateSkipChanged { candidate_key, skipped: true }) => {
            queue.withdraw(context, &candidate_key).await;
        }
        // Gone: a run left going would never finish.
        ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
            queue.withdraw(context, &candidate_key).await;
        }
        // Taken by an import, which says so once, in its first report.
        ImportEvent::ImportProgress { candidate_key, progress }
            if import_takes_candidate(&progress) =>
        {
            queue.withdraw(context, &candidate_key).await;
        }
        _ => {}
    }
}

/// Whether `progress` is the report that says an import took its candidate.
/// An import's first report is its `Preparing`, sent before it writes
/// anything; every later one (percent ticks, endings) finds the key already
/// off the queue, and withdrawing it again is a write that changes nothing.
fn import_takes_candidate(progress: &ImportProgress) -> bool {
    match progress {
        ImportProgress::Preparing { .. } => true,
        ImportProgress::Progress { .. }
        | ImportProgress::Complete { .. }
        | ImportProgress::RemoteUploadQueued { .. }
        | ImportProgress::Failed { .. }
        | ImportProgress::Cancelled { .. } => false,
    }
}

/// One state of the run answering a job.
async fn advance(
    context: &Context,
    queue: &mut Queue,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    key: &crate::CandidateKey,
    run: IdentifyRunId,
    state: IdentifyState,
) {
    let Some(index) = queue.running_job(key, run) else {
        return;
    };
    // Something outside the queue ended the run, which ends this candidate's
    // identification; the rest of its job waits for a run of its own.
    if matches!(state, IdentifyState::Idle) {
        queue.withdraw(context, key).await;
        return;
    }
    if !state.is_terminal() {
        return;
    }
    let job = &mut queue.jobs[index];
    let identity = job.identity.clone();
    let candidate = job
        .members
        .iter()
        .find(|member| member.candidate.key() == *key)
        .map(|member| member.candidate.clone())
        .expect("a job's running representative is one of its members");
    // The run ended on its own; only its extraction may still be working.
    context.import.cancel_candidate_extraction(key);
    let settle_token = settle_token.child_token();
    job.state =
        JobState::Settling { representative: key.clone(), run, abandon: settle_token.clone() };
    let context = context.clone();
    #[expect(
        clippy::disallowed_methods,
        reason = "the settle catches its own panic, and the queue joins every settle in its set and tells of a failed join"
    )]
    settling.spawn(async move {
        let representative_key = candidate.key();
        settled_or_failed(
            &context,
            identity.clone(),
            representative_key,
            run,
            settle_answer(context.clone(), identity, candidate, run, state, settle_token),
        )
        .await
    });
}

/// Follow a candidate the queue holds to what is stored for it now: off the
/// queue when it can no longer be answered, and placed again as its new files
/// when they changed while it waited. A run reading the old files was ended
/// when the runtime recorded the change, and its ending takes the key off
/// the queue; an answer already being written is refused for files the
/// candidate no longer has.
async fn follow(context: &Context, queue: &mut Queue, key: &crate::CandidateKey) {
    let Some(held) = queue.held(key) else {
        return;
    };
    let candidate = match answerable_candidate(context, key).await {
        Ok(Some(candidate)) => candidate,
        Ok(None) => {
            queue.withdraw(context, key).await;
            return;
        }
        // What the candidate stands at is unknown, so the queue keeps it as
        // it is and the failure is reported.
        Err(error) => {
            record(context, &format!("read {key} to follow it"), Err(error));
            return;
        }
    };
    if !held.running && CandidateFiles::of(&candidate) != held.identity {
        admit(context, queue, vec![candidate], held.admission).await;
    }
}

#[cfg(test)]
#[path = "reactions_tests.rs"]
mod tests;
