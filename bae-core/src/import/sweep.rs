//! Queue-wide identification: while `identify_automatically` is on, every
//! unseeded Lookup candidate acquires a verdict without anyone clicking it.
//!
//! The sweep owns no pipeline of its own. It walks the candidates the scan
//! already found, drives each through the existing extraction → identify pair
//! at [`CallPriority::Background`], and writes the terminal verdict to
//! `import_candidate_state`. What it adds over the explicit Lookup path is
//! scheduling: which candidates still need answering, how many at once, and the
//! one settle step that buys the documents of the single pressing it matched —
//! the tracklist that decides Ready, and everything opening the candidate would
//! otherwise re-fetch.
//!
//! **It starts and stops with the library, not with a view.**
//! [`crate::library::AppServices`] constructs one and its `Drop` stops it, so
//! the queue is identified whether or not anyone has the Import section open.
//! Opening a view triggers nothing.
//!
//! **It is the one writer of `import_candidate_state`'s verdict**, including
//! for runs it did not start: [`QueueSweepHandle::record_explicit_lookup`]
//! hangs a recorder off a candidate after a person enters Lookup, so their
//! answer persists too.
//! Everything that decides what to store lives here rather than being spread
//! across the two producers. The row's other half — the user's sheet bindings —
//! is written by the import handle, and writing it *clears* the verdict, which
//! is what brings a re-bound candidate back to this sweep.
//!
//! **A candidate whose content hash already holds applied metadata provenance or a
//! finished verdict is skipped.** A source-less draft and File Tags are complete metadata
//! choices, not inputs to Lookup. A stored identify verdict is settled because the settle
//! step and the verdict are written together.
//!
//! **Provider failures are answers.** They are stored as failed verdicts and
//! automatic passes leave them alone; only an explicit re-run replaces one.
//! Cancellation and a candidate that vanished mid-flight still write nothing,
//! because neither is an outcome of the candidate's lookup.

use super::folder_scanner::FolderCandidate;
use super::handle::{ImportEvent, ImportServiceHandle, ScanEvent};
use super::ImportCandidateSnapshot;
use crate::db::{DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::{IdentifyRunId, IdentifyServiceHandle, IdentifyState, TerminalVerdict};
use crate::import::search::MetadataResult;
use crate::library::LibraryManager;
use crate::signals::{ExtractionServiceHandle, ExtractionSource};
use crate::util::rate_limiter::CallPriority;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, info, warn};

mod handle;
mod plan;
mod settle;

pub use handle::QueueSweepHandle;
use plan::*;
use settle::*;

/// How many candidates are identified at once.
///
/// The local half of a candidate — the folder walk, disc-ID derivation,
/// duration probing, artwork OCR — is CPU and disk work that parallelises, and
/// the network half is serialised by the provider rate limiter however many run
/// at once. So the cap exists to keep OCR off every core, not to pace the
/// network. A constant, not configuration: there is no setting a user could
/// meaningfully choose here.
const MAX_IN_FLIGHT: usize = 4;

/// The services and live ownership one queue-identification pass needs.
#[derive(Clone)]
struct SweepContext {
    import: ImportServiceHandle,
    identify: IdentifyServiceHandle,
    extraction: ExtractionServiceHandle,
    library_manager: LibraryManager,
    /// Candidate keys the sweep currently has drivers running for.
    ours: Arc<Mutex<HashSet<String>>>,
}

impl SweepContext {
    fn owned_elsewhere(&self, key: &str) -> bool {
        self.identify.is_running(key) && !self.ours.lock().unwrap().contains(key)
    }

    fn release(&self, key: &str) {
        self.identify.cancel(key);
        self.extraction.cancel(key);
        self.ours.lock().unwrap().remove(key);
    }

    fn release_all(&self) {
        let keys = self
            .ours
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.identify.cancel(&key);
            self.extraction.cancel(&key);
        }
        self.ours.lock().unwrap().clear();
    }
}

/// Start the queue sweep. A candidate becoming actionable, a binding change,
/// or a completed folder scan plans a pass.
pub fn start(
    import: ImportServiceHandle,
    identify: IdentifyServiceHandle,
    extraction: ExtractionServiceHandle,
    library_manager: LibraryManager,
) -> QueueSweepHandle {
    let token = CancellationToken::new();
    let tasks = TaskTracker::new();
    let context = SweepContext {
        import,
        identify,
        extraction,
        library_manager,
        ours: Arc::new(Mutex::new(HashSet::new())),
    };

    // Subscribe before the task is spawned so the launch scan's `Finished`
    // cannot land in the gap between `start` returning and the loop's first
    // `recv`.
    let mut bus = context.import.subscribe_events();
    let mut config = context.library_manager.subscribe_config_changes();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("queue sweep runtime");
    let runtime_handle = runtime.handle().clone();
    let relay_token = token.clone();
    tasks.spawn_on(
        async move {
            loop {
                let event = tokio::select! {
                    biased;
                    _ = relay_token.cancelled() => return,
                    event = bus.recv() => event,
                };
                if event_tx.send(event).is_err() {
                    return;
                }
            }
        },
        &runtime_handle,
    );
    let loop_token = token.clone();
    let loop_context = context.clone();
    tasks.spawn_on(
        async move {
            loop {
                let event = tokio::select! {
                    biased;
                    _ = loop_token.cancelled() => return,
                    changed = config.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if config.borrow().identify_automatically {
                            run_pass(&loop_context, &loop_token, &mut event_rx, &mut config).await;
                        } else {
                            loop_context.release_all();
                            emit_progress(&loop_context, 0, 0);
                        }
                        continue;
                    }
                    event = event_rx.recv() => event,
                };
                // A pass over an already-answered queue is one DB read and one
                // event, so a scan that changed nothing costs nothing and there is
                // no debounce to get wrong. Scans finishing while a pass runs queue
                // up behind it and produce another pass, which is what makes a
                // folder added mid-sweep get picked up.
                match event {
                    // A binding change plans a pass for the same reason a finished
                    // scan does: a candidate that has no stored answer, because the
                    // change cleared it.
                    Some(Ok(ImportEvent::Scan(
                        ScanEvent::FolderCandidate { .. }
                        | ScanEvent::Finished
                        | ScanEvent::CandidateBindingChanged { .. }
                        | ScanEvent::CandidateMetadataChanged { .. }
                        | ScanEvent::CandidateSkipChanged { .. },
                    ))) => {
                        run_pass(
                            &loop_context,
                            &loop_token,
                            &mut event_rx,
                            &mut config,
                        )
                        .await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(broadcast::error::RecvError::Lagged(n))) => {
                        warn!("sweep: import bus lagged by {n} events; planning a pass in case a scan finished inside the gap");
                        run_pass(
                            &loop_context,
                            &loop_token,
                            &mut event_rx,
                            &mut config,
                        )
                        .await;
                    }
                    Some(Err(broadcast::error::RecvError::Closed)) | None => return,
                }
            }
        },
        &runtime_handle,
    );

    let completion_tasks = tasks.clone();
    let executor_thread = std::thread::Builder::new()
        .name("bae-import-sweep".to_string())
        .spawn(move || runtime.block_on(completion_tasks.wait()))
        .expect("queue sweep executor thread");

    QueueSweepHandle::new(context, token, tasks, runtime_handle, executor_thread)
}

/// One candidate the pass is driving: what it will need once a verdict lands.
struct InFlight {
    job: IdentifyJob,
    /// The identify run this pass started for the representative. A settled
    /// earlier run of the same candidate still broadcasts; only this run's
    /// states are this pass's answer.
    run: IdentifyRunId,
    /// The candidate's latest `SignalsUpdated` value. `None` until extraction
    /// reports one; by the time a verdict is terminal the identify machine has
    /// consumed a settled snapshot, so this holds it.
    signals: Option<crate::signals::Signals>,
    /// Editable metadata revision this run began from. A later edit makes the
    /// terminal result stale even when the candidate's files did not change.
    expected_metadata_revision: u64,
}

struct ExplicitLookupInFlight {
    candidate: FolderCandidate,
    signals: Option<crate::signals::Signals>,
    expected_metadata_revision: u64,
}

/// What a finished candidate reports back to the pass loop.
struct Finished {
    representative_key: String,
    identity: CandidateIdentity,
    candidate_keys: Vec<String>,
    current_candidates: Vec<FolderCandidate>,
    outcome: FinishCandidateOutcome,
}

type CandidateIdentity = (String, u64);

struct IdentifyJob {
    identity: CandidateIdentity,
    candidates: Vec<FolderCandidate>,
}

impl IdentifyJob {
    fn representative(&self) -> &FolderCandidate {
        self.candidates
            .first()
            .expect("an identify job always has a candidate")
    }

    fn candidate_keys(&self) -> impl Iterator<Item = String> + '_ {
        self.candidates
            .iter()
            .map(|candidate| candidate.path.to_string_lossy().into_owned())
    }
}

/// Removes every automatic queue marker when this pass exits. Driver-reported
/// and explicit Lookup state belong to different producers and survive it.
struct AutomaticQueueGuard(ImportServiceHandle);

impl Drop for AutomaticQueueGuard {
    fn drop(&mut self) {
        self.0
            .replace_automatic_identification_queue(std::iter::empty());
    }
}

/// What a pass has to do, decided against the stored rows before any of it
/// starts.
struct Plan {
    /// Candidates with neither applied metadata provenance nor a usable stored verdict.
    identify: VecDeque<IdentifyJob>,
    /// How many of `total` already hold provenance or a verdict.
    identified: u32,
    total: u32,
}

enum PassOutcome {
    Complete,
    Replan,
}

/// Walk the queue once: plan what still needs answering, drive it under the
/// concurrency cap, and report progress as verdicts land.
async fn run_pass(
    context: &SweepContext,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<Result<ImportEvent, broadcast::error::RecvError>>,
    config: &mut tokio::sync::watch::Receiver<crate::config::Config>,
) {
    while let PassOutcome::Replan = run_pass_once(context, token, bus, config).await {}
}

async fn run_pass_once(
    context: &SweepContext,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<Result<ImportEvent, broadcast::error::RecvError>>,
    config: &mut tokio::sync::watch::Receiver<crate::config::Config>,
) -> PassOutcome {
    if !config.borrow().identify_automatically {
        context.release_all();
        emit_progress(context, 0, 0);
        return PassOutcome::Complete;
    }
    let candidates = match new_candidates(context).await {
        Ok(candidates) => candidates,
        Err(error) => {
            // Without the list the sweep cannot plan. Skip the pass; the next
            // scan plans another.
            warn!("sweep: could not read the candidate list ({error}); skipping this pass");
            return PassOutcome::Complete;
        }
    };
    let total = candidates.len() as u32;
    let mut known_identities: HashMap<String, CandidateIdentity> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.path.to_string_lossy().into_owned(),
                candidate_identity(candidate),
            )
        })
        .collect();

    let stored = match context.library_manager.load_import_candidate_states().await {
        Ok(stored) => stored,
        Err(e) => {
            // Without the stored set the sweep cannot tell answered from
            // unanswered, and identifying the whole queue again would spend the
            // rate limit re-learning what it already knows. Skip the pass; the
            // next scan plans another.
            warn!("sweep: could not read stored candidate states ({e}); skipping this pass");
            return PassOutcome::Complete;
        }
    };
    let mut answered_keys: HashSet<String> = candidates
        .iter()
        .filter(|candidate| usable_stored_answer(&stored, candidate).is_some())
        .map(|candidate| candidate.path.to_string_lossy().into_owned())
        .collect();
    let mut answered_identities: HashSet<CandidateIdentity> = candidates
        .iter()
        .filter(|candidate| usable_stored_answer(&stored, candidate).is_some())
        .map(candidate_identity)
        .collect();

    let Plan {
        identify: mut pending,
        mut identified,
        mut total,
    } = plan(candidates, &stored, total);
    emit_progress(context, identified, total);

    if pending.is_empty() {
        context
            .import
            .replace_automatic_identification_queue(std::iter::empty());
        return PassOutcome::Complete;
    }

    context.import.replace_automatic_identification_queue(
        pending.iter().flat_map(IdentifyJob::candidate_keys),
    );
    let _automatic_queue = AutomaticQueueGuard(context.import.clone());

    let mut in_flight: HashMap<String, InFlight> = HashMap::new();
    let mut finishing_members: HashMap<CandidateIdentity, Vec<FolderCandidate>> = HashMap::new();
    let mut finishing = JoinSet::<Finished>::new();

    loop {
        while in_flight.len() + finishing.len() < MAX_IN_FLIGHT {
            if !config.borrow().identify_automatically {
                context.release_all();
                finishing.shutdown().await;
                emit_progress(context, 0, 0);
                return PassOutcome::Complete;
            }
            let Some(mut job) = pending.pop_front() else {
                break;
            };
            let Some(representative_index) = job.candidates.iter().position(|candidate| {
                !context.owned_elsewhere(candidate.path.to_string_lossy().as_ref())
            }) else {
                debug!(
                    "sweep: every member of {:?} is identified elsewhere",
                    job.identity
                );
                for key in job.candidate_keys() {
                    context.import.clear_automatic_identification(&key);
                }
                continue;
            };
            job.candidates.swap(0, representative_index);
            let candidate = job.representative().clone();
            let key = candidate.path.to_string_lossy().into_owned();
            let expected_metadata_revision = match candidate_metadata_revision(context, &candidate)
                .await
            {
                Ok(revision) => revision,
                Err(error) => {
                    warn!(
                            "sweep: cannot read the metadata revision for {key} ({error}); aborting pass"
                        );
                    for running_key in in_flight.keys() {
                        context.release(running_key);
                    }
                    finishing.shutdown().await;
                    return PassOutcome::Complete;
                }
            };
            context.ours.lock().unwrap().insert(key.clone());
            // Identify first: it takes its bus subscription synchronously, so
            // extraction's first snapshot cannot be emitted into a void.
            let run = context.identify.new_run();
            context
                .identify
                .start(run, key.clone(), CallPriority::Background);
            context.extraction.start(
                key.clone(),
                ExtractionSource::Folder {
                    path: candidate.path.clone(),
                    files: candidate.files.clone(),
                },
                CallPriority::Background,
            );
            in_flight.insert(
                key,
                InFlight {
                    job,
                    run,
                    signals: None,
                    expected_metadata_revision,
                },
            );
        }

        if in_flight.is_empty() && pending.is_empty() && finishing.is_empty() {
            return PassOutcome::Complete;
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                for key in in_flight.keys() {
                    context.release(key);
                }
                finishing.shutdown().await;
                return PassOutcome::Complete;
            }
            changed = config.changed() => {
                if changed.is_err()
                    || !config
                        .borrow()
                        .identify_automatically
                {
                    context.release_all();
                    finishing.shutdown().await;
                    emit_progress(context, 0, 0);
                    return PassOutcome::Complete;
                }
            }
            Some(result) = finishing.join_next() => {
                match result {
                    Ok(done) => {
                        context.release(&done.representative_key);
                        let deferred = finishing_members
                            .remove(&done.identity)
                            .expect("finishing identity is registered before its task starts");
                        let stored = matches!(&done.outcome, FinishCandidateOutcome::Stored);
                        match done.outcome {
                            FinishCandidateOutcome::Stored => {
                                answered_identities.insert(done.identity.clone());
                                pending.retain(|job| job.identity != done.identity);
                                for key in &done.candidate_keys {
                                    context.import.clear_automatic_identification(key);
                                }
                            }
                            FinishCandidateOutcome::Superseded => {
                                for candidate in
                                    done.current_candidates.into_iter().chain(deferred)
                                {
                                    enqueue_automatic_candidate(context, &mut pending, candidate);
                                }
                            }
                            FinishCandidateOutcome::Failed { error } => {
                                warn!(
                                    "sweep: could not commit identification for {} ({error})",
                                    done.representative_key
                                );
                                for candidate in
                                    done.current_candidates.into_iter().chain(deferred)
                                {
                                    context.import.fail_identification(
                                        &candidate.path.to_string_lossy(),
                                        error.clone(),
                                    );
                                }
                            }
                        }
                        let newly_answered = known_identities
                            .iter()
                            .filter(|(_, identity)| *identity == &done.identity)
                            .map(|(key, _)| key)
                            .filter(|key| stored && answered_keys.insert((*key).clone()))
                            .count() as u32;
                        if newly_answered > 0 {
                            identified = identified.saturating_add(newly_answered).min(total);
                            emit_progress(context, identified, total);
                        } else {
                            debug!(
                                "sweep: {} finished without a current stored verdict",
                                done.representative_key
                            );
                        }
                    }
                    Err(error) if error.is_cancelled() && token.is_cancelled() => {
                        return PassOutcome::Complete;
                    }
                    Err(error) => warn!("sweep finishing task failed: {error}"),
                }
            }
            event = bus.recv() => match event {
                Some(Ok(ImportEvent::SignalsUpdated { candidate_key, signals, .. })) => {
                    if let Some(entry) = in_flight.get_mut(&candidate_key) {
                        entry.signals = Some(signals);
                    }
                }
                Some(Ok(ImportEvent::IdentifyStateChanged { candidate_key, run, state, .. })) => {
                    // Terminal means the machine stopped moving, including on
                    // an explicit failure verdict. Either way the candidate's
                    // slot is free now.
                    // A state from another run of the same candidate -- an
                    // earlier one still broadcasting -- is not this pass's.
                    let ours = in_flight
                        .get(&candidate_key)
                        .filter(|entry| entry.run == run);
                    if let Some(entry) = ours {
                        for member_key in entry.job.candidate_keys() {
                            if member_key != candidate_key {
                                context.import.report_identification(&member_key, &state);
                            }
                        }
                    }
                    let settled = (state.is_terminal() && ours.is_some())
                    .then(|| in_flight.remove(&candidate_key))
                    .flatten();
                    if let Some(entry) = settled {
                        let identity = entry.job.identity.clone();
                        finishing_members.insert(identity.clone(), Vec::new());
                        let representative_key = candidate_key.clone();
                        let context = context.clone();
                        let child = token.child_token();
                        finishing.spawn(async move {
                            let candidate_keys = entry.job.candidate_keys().collect();
                            let outcome = finish_candidate(&context, &entry, state, &child).await;
                            let current_candidates = if matches!(
                                &outcome,
                                FinishCandidateOutcome::Stored
                            ) {
                                Vec::new()
                            } else {
                                let mut current = Vec::new();
                                for candidate in &entry.job.candidates {
                                    let key = candidate.path.to_string_lossy();
                                    if usable_current_candidate(&context, &key, &identity).await {
                                        current.push(candidate.clone());
                                    }
                                }
                                current
                            };
                            Finished {
                                representative_key,
                                identity,
                                candidate_keys,
                                current_candidates,
                                outcome,
                            }
                        });
                    }
                }
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { .. }))) => {
                    // A chosen source owns this candidate now. Cancel every
                    // background run from this pass and plan again from the
                    // committed provenance so no in-flight duplicate can continue
                    // OCR or provider lookup for the same content hash.
                    context.release_all();
                    finishing.shutdown().await;
                    return PassOutcome::Replan;
                }
                // The folder was removed, renamed, or unmounted while we were
                // identifying it. Extraction is cancelled for us by the signal
                // service's own listener, so no further `Signals` will ever
                // arrive and the driver would sit in `Triangulating` forever —
                // holding a slot that never frees and stalling the pass, and
                // with it every later scan. The removal is an event, so react to
                // it rather than waiting out a clock.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }))) => {
                    context.import.clear_automatic_identification(&candidate_key);
                    remove_finishing_member(&mut finishing_members, &candidate_key);
                    let running_representative = in_flight.iter().find_map(|(representative, entry)| {
                        entry
                            .job
                            .candidates
                            .iter()
                            .any(|candidate| candidate.path.to_string_lossy() == candidate_key)
                            .then(|| representative.clone())
                    });
                    if let Some(representative) = running_representative {
                        let mut entry = in_flight
                            .remove(&representative)
                            .expect("located in-flight job still exists");
                        entry.job.candidates.retain(|candidate| {
                            candidate.path.to_string_lossy() != candidate_key
                        });
                        if representative == candidate_key {
                            context.release(&representative);
                            if !entry.job.candidates.is_empty() {
                                pending.push_front(entry.job);
                            }
                        } else if !entry.job.candidates.is_empty() {
                            in_flight.insert(representative, entry);
                        }
                    }
                    pending.retain_mut(|job| {
                        job.candidates.retain(|candidate| {
                            candidate.path.to_string_lossy() != candidate_key
                        });
                        !job.candidates.is_empty()
                    });
                    if forget_candidate(
                        &candidate_key,
                        &mut known_identities,
                        &mut answered_keys,
                        &mut answered_identities,
                        &mut identified,
                        &mut total,
                    ) {
                        emit_progress(context, identified.min(total), total);
                    }
                }
                Some(Ok(ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }))) => {
                    let candidate_key = candidate.path.to_string_lossy().into_owned();
                    // A scan announces every candidate it walks, including ones
                    // the sweep is not responsible for — skipped, already in the
                    // library, or claimed by an import that started since the
                    // pass began. Whether this is one of ours is
                    // `sweepable_candidate`'s question and nobody else's: the
                    // event's own flags answer a narrower one, and re-deriving
                    // the answer here is what let a re-scan count an importing
                    // candidate back into a total the import had just taken it
                    // out of. Asked against live state rather than the event,
                    // because the claim that supersedes it carries no event.
                    let Some(candidate) = sweepable_candidate(context, &candidate_key).await else {
                        detach_candidate(
                            context,
                            &candidate_key,
                            &mut in_flight,
                            &mut pending,
                        );
                        remove_finishing_member(&mut finishing_members, &candidate_key);
                        if forget_candidate(
                            &candidate_key,
                            &mut known_identities,
                            &mut answered_keys,
                            &mut answered_identities,
                            &mut identified,
                            &mut total,
                        ) {
                            emit_progress(context, identified.min(total), total);
                        }
                        continue;
                    };
                    let identity = candidate_identity(&candidate);
                    if known_identities.get(&candidate_key) == Some(&identity) {
                        continue;
                    }
                    forget_candidate(
                        &candidate_key,
                        &mut known_identities,
                        &mut answered_keys,
                        &mut answered_identities,
                        &mut identified,
                        &mut total,
                    );
                    known_identities.insert(candidate_key.clone(), identity.clone());
                    total = total.saturating_add(1);
                    detach_candidate(
                        context,
                        &candidate_key,
                        &mut in_flight,
                        &mut pending,
                    );
                    remove_finishing_member(&mut finishing_members, &candidate_key);
                    let stored_now = match current_stored_answer(context, &candidate).await {
                        Ok(stored) => stored,
                        Err(error) => {
                            warn!(
                                "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                            );
                            for key in in_flight.keys() {
                                context.release(key);
                            }
                            finishing.shutdown().await;
                            return PassOutcome::Complete;
                        }
                    };
                    // Already answered — either on disk, or by a candidate this
                    // pass settled that hashes the same.
                    if stored_now || answered_identities.contains(&identity) {
                        answered_identities.insert(identity.clone());
                        answered_keys.insert(candidate_key);
                        identified = identified.saturating_add(1).min(total);
                    } else if let Some(members) = finishing_members.get_mut(&identity) {
                        members.push(candidate);
                    } else {
                        enqueue_automatic_candidate(context, &mut pending, candidate);
                    }
                    emit_progress(context, identified.min(total), total);
                }
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key,
                    skipped,
                }))) => {
                    detach_candidate(
                        context,
                        &candidate_key,
                        &mut in_flight,
                        &mut pending,
                    );
                    remove_finishing_member(&mut finishing_members, &candidate_key);
                    if skipped {
                        if forget_candidate(
                            &candidate_key,
                            &mut known_identities,
                            &mut answered_keys,
                            &mut answered_identities,
                            &mut identified,
                            &mut total,
                        ) {
                            emit_progress(context, identified.min(total), total);
                        }
                    } else if let Some(candidate) = sweepable_candidate(context, &candidate_key).await {
                        let identity = candidate_identity(&candidate);
                        if known_identities
                            .insert(candidate_key.clone(), identity.clone())
                            .is_none()
                        {
                            total = total.saturating_add(1);
                        }
                        let stored_now =
                            match current_stored_answer(context, &candidate).await {
                                Ok(stored) => stored,
                                Err(error) => {
                                    warn!(
                                        "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                                    );
                                    for key in in_flight.keys() {
                                        context.release(key);
                                    }
                                    finishing.shutdown().await;
                                    return PassOutcome::Complete;
                                }
                            };
                        if stored_now {
                            answered_identities.insert(identity);
                            if answered_keys.insert(candidate_key.clone()) {
                                identified = identified.saturating_add(1).min(total);
                            }
                        } else {
                            enqueue_automatic_candidate(context, &mut pending, candidate);
                        }
                        emit_progress(context, identified.min(total), total);
                    }
                }
                // The folder is a different shape now. A run already under way
                // is answering the shape it had, and letting it settle would
                // write that answer straight back over the one the binding
                // change just cleared — so drop it. The pass this event also
                // plans identifies the candidate again, against what it now is.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))) => {
                    let candidate_key = candidate.path.to_string_lossy().into_owned();
                    detach_candidate(
                        context,
                        &candidate_key,
                        &mut in_flight,
                        &mut pending,
                    );
                    remove_finishing_member(&mut finishing_members, &candidate_key);
                    forget_candidate(
                        &candidate_key,
                        &mut known_identities,
                        &mut answered_keys,
                        &mut answered_identities,
                        &mut identified,
                        &mut total,
                    );
                    let identity = candidate_identity(&candidate);
                    known_identities.insert(candidate_key.clone(), identity.clone());
                    total = total.saturating_add(1);
                    if let Some(members) = finishing_members.get_mut(&identity) {
                        members.push(candidate);
                    } else {
                        enqueue_automatic_candidate(context, &mut pending, candidate);
                    }
                    emit_progress(context, identified.min(total), total);
                }
                Some(Ok(ImportEvent::ImportProgress { candidate_key, .. })) => {
                    detach_candidate(
                        context,
                        &candidate_key,
                        &mut in_flight,
                        &mut pending,
                    );
                    remove_finishing_member(&mut finishing_members, &candidate_key);
                    if forget_candidate(
                        &candidate_key,
                        &mut known_identities,
                        &mut answered_keys,
                        &mut answered_identities,
                        &mut identified,
                        &mut total,
                    ) {
                        emit_progress(context, identified.min(total), total);
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(broadcast::error::RecvError::Lagged(n))) => {
                    // A dropped `IdentifyStateChanged` would leave its candidate
                    // in flight with nothing left to wake it, so the pass would
                    // stall on a slot that never frees. Give the affected
                    // candidates back to the queue and run them again: nothing
                    // durable was written, so replaying them whole is the only
                    // shape that cannot leave a wrong answer behind.
                    warn!("sweep: import bus lagged by {n} events; replaying {} in-flight candidates", in_flight.len());
                    context.library_manager.record_telemetry(
                        crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        },
                    );
                    for (key, entry) in in_flight.drain() {
                        context.release(&key);
                        for candidate_key in entry.job.candidate_keys() {
                            context
                                .import
                                .requeue_automatic_identification(&candidate_key);
                        }
                        pending.push_back(entry.job);
                    }
                }
                Some(Err(broadcast::error::RecvError::Closed)) | None => {
                    return PassOutcome::Complete;
                }
            },
        }
    }
}

fn enqueue_automatic_candidate(
    context: &SweepContext,
    pending: &mut VecDeque<IdentifyJob>,
    candidate: FolderCandidate,
) {
    let key = candidate.path.to_string_lossy().into_owned();
    enqueue_candidate(pending, candidate);
    context.import.requeue_automatic_identification(&key);
}

#[cfg(test)]
async fn run_pass_for_test(context: &SweepContext, token: &CancellationToken) {
    let mut bus = context.import.subscribe_events();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let relay_token = token.child_token();
    let relay = tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                _ = relay_token.cancelled() => return,
                event = bus.recv() => event,
            };
            if event_tx.send(event).is_err() {
                return;
            }
        }
    });
    let mut config = context.library_manager.subscribe_config_changes();
    run_pass(context, token, &mut event_rx, &mut config).await;
    relay.abort();
}

/// The scanned folder candidate behind `key` when it is actionable, read by
/// key. A read that fails answers no key, and says so.
async fn actionable_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    match context.import.get_candidate(key).await {
        Ok(Some(ImportCandidateSnapshot::Folder {
            candidate,
            actionable: true,
            ..
        })) => Some(candidate),
        Ok(_) => None,
        Err(error) => {
            warn!("cannot read candidate {key}: {error}");
            None
        }
    }
}

/// The candidate the sweep is responsible for at `key`, read exactly — see
/// [`ImportServiceHandle::sweepable_candidate`]. A read that fails answers
/// no candidate, and says so.
async fn sweepable_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    match context.import.sweepable_candidate(key).await {
        Ok(candidate) => candidate,
        Err(error) => {
            warn!("sweep: cannot read candidate {key} ({error}); treating it as not ours");
            None
        }
    }
}

async fn candidate_metadata_revision(
    context: &SweepContext,
    candidate: &FolderCandidate,
) -> Result<u64, crate::library::LibraryError> {
    context
        .library_manager
        .load_import_candidate_state(&candidate.files.content_hash())
        .await?
        .map(|state| state.metadata_revision)
        .ok_or_else(|| {
            crate::library::LibraryError::Internal(format!(
                "candidate {} has no persisted state row",
                candidate.path.display()
            ))
        })
}

/// Announce how much of the queue has been answered.
///
/// Both numbers are the sweep's, not the UI's: the total is how many candidates
/// the sweep is responsible for, which is a domain fact about the queue and not
/// something a view can infer from the rows it happens to be holding.
fn emit_progress(context: &SweepContext, identified: u32, total: u32) {
    context
        .import
        .announce_queue_identify_progress(identified, total);
}

#[cfg(test)]
mod tests;
