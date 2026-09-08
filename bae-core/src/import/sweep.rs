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
//! **It is the one writer of a candidate's verdict**, including for runs it
//! did not start: [`QueueSweepHandle::identify_for_explicit_lookup`] hangs a
//! recorder off a candidate after a person enters Lookup, so their answer
//! persists too.
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

use super::handle::{ImportEvent, ImportServiceHandle, ScanEvent};
use super::release_candidate::ReleaseCandidate;
use crate::db::{DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::{IdentifyRunId, IdentifyState, TerminalVerdict};
use crate::import::search::MetadataResult;
use crate::import::LookupChoices;
use crate::library::LibraryManager;
use crate::signals::ExtractionSource;
use crate::util::rate_limiter::CallPriority;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, info, warn};

mod handle;
mod pass;
mod plan;
mod settle;

pub use handle::QueueSweepHandle;
use pass::Pass;
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

/// The services and live ownership one queue-identification pass needs. The
/// identify driver and the extraction behind it are the import handle's, so
/// the sweep and the commands that decide a candidate act on the same pair.
#[derive(Clone)]
struct SweepContext {
    import: ImportServiceHandle,
    library_manager: LibraryManager,
    /// Candidate keys the sweep currently has drivers running for.
    ours: Arc<Mutex<HashSet<String>>>,
}

impl SweepContext {
    /// Whether something is already answering this candidate: a driver alive,
    /// or a terminal answer whose durable write has not landed yet. Two
    /// separate facts, and the candidate is taken while either holds — a run
    /// that has answered still owns its key until the row states its result,
    /// or a pass planning inside that interval would identify it again.
    fn identification_in_flight(&self, key: &str) -> bool {
        self.import.is_identifying(key) || self.import.is_saving_identification(key)
    }

    fn owned_elsewhere(&self, key: &str) -> bool {
        self.identification_in_flight(key) && !self.ours.lock().unwrap().contains(key)
    }

    /// Stop the run the sweep started for `key` and stop counting it as ours.
    /// For a candidate the sweep is ending mid-run itself.
    ///
    /// Only a run that is still the sweep's is cancelled. A decision about a
    /// candidate ends its run at the command that decided, and the sweep
    /// hears about that decision afterwards — by which time the key may name
    /// a run somebody else started, which is not the sweep's to tear down.
    fn release(&self, key: &str) {
        if !self.ours.lock().unwrap().remove(key) {
            return;
        }
        self.import.cancel_identification(key);
    }

    /// The run answered and ended on its own, so there is no driver left to
    /// cancel — only the extraction that fed it, and the ownership mark.
    /// Cancelling identify here would tear down whatever run has taken the key
    /// since the answer landed.
    fn release_settled(&self, key: &str) {
        self.import.cancel_candidate_extraction(key);
        self.ours.lock().unwrap().remove(key);
    }

    /// Stop counting `key` as ours without cancelling anything. For the one
    /// case where the run is already gone and something else holds the key: a
    /// cancel here would tear down whoever took it over.
    fn disown(&self, key: &str) {
        self.ours.lock().unwrap().remove(key);
    }

    fn release_all(&self) {
        let keys = self
            .ours
            .lock()
            .unwrap()
            .drain()
            .collect::<Vec<_>>();
        for key in keys {
            self.import.cancel_identification(&key);
        }
    }
}

/// Start the queue sweep. A candidate becoming actionable, a binding change,
/// or a completed folder scan plans a pass.
pub fn start(import: ImportServiceHandle, library_manager: LibraryManager) -> QueueSweepHandle {
    let token = CancellationToken::new();
    let tasks = TaskTracker::new();
    let context = SweepContext {
        import,
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
                        if config.borrow().prefs.identify_automatically {
                            run_pass(&loop_context, &loop_token, &mut event_rx, &mut config).await;
                        } else {
                            loop_context.release_all();
                            announce_empty_queue(&loop_context);
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
    candidate: ReleaseCandidate,
    signals: Option<crate::signals::Signals>,
    expected_metadata_revision: u64,
}

/// What a finished candidate reports back to the pass loop.
struct Finished {
    representative_key: String,
    /// The run whose answer this is. What the write it asked for leaves in the
    /// candidate runtime is recorded against it.
    run: IdentifyRunId,
    identity: CandidateIdentity,
    candidate_keys: Vec<String>,
    current_candidates: Vec<ReleaseCandidate>,
    outcome: FinishCandidateOutcome,
}

type CandidateIdentity = (String, u64);

struct IdentifyJob {
    identity: CandidateIdentity,
    candidates: Vec<ReleaseCandidate>,
}

impl IdentifyJob {
    fn representative(&self) -> &ReleaseCandidate {
        self.candidates
            .first()
            .expect("an identify job always has a candidate")
    }

    fn candidate_keys(&self) -> impl Iterator<Item = String> + '_ {
        self.candidates
            .iter()
            .map(|candidate| candidate.key().into_owned())
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

/// Wait for every settling task to run out, and stop owning what each of them
/// settled.
///
/// Nothing is aborted. A settling task holds a durable write and ends its own
/// pending save whichever way it goes, so tearing one down mid-flight would
/// leave a candidate's row saying a commit is still coming. Cancelling
/// `settling` first is what abandons their answers; the tasks then return at
/// their next check.
async fn drain(context: &SweepContext, finishing: &mut JoinSet<Finished>) {
    while let Some(result) = finishing.join_next().await {
        match result {
            Ok(done) => context.release_settled(&done.representative_key),
            Err(error) => warn!("sweep finishing task failed: {error}"),
        }
    }
}

/// Walk the queue once: plan what still needs answering, drive it under the
/// concurrency cap, and report progress as verdicts land.
async fn run_pass(
    context: &SweepContext,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<Result<ImportEvent, broadcast::error::RecvError>>,
    config: &mut tokio::sync::watch::Receiver<crate::config::Config>,
) {
    if !config.borrow().prefs.identify_automatically {
        context.release_all();
        announce_empty_queue(context);
        return;
    }
    let candidates = match new_candidates(context).await {
        Ok(candidates) => candidates,
        Err(error) => {
            // Without the list the sweep cannot plan. Skip the pass; the next
            // scan plans another.
            warn!("sweep: could not read the candidate list ({error}); skipping this pass");
            return;
        }
    };
    let stored = match context.library_manager.load_import_candidate_states().await {
        Ok(stored) => stored,
        Err(e) => {
            // Without the stored set the sweep cannot tell answered from
            // unanswered, and identifying the whole queue again would spend the
            // rate limit re-learning what it already knows. Skip the pass; the
            // next scan plans another.
            warn!("sweep: could not read stored candidate states ({e}); skipping this pass");
            return;
        }
    };

    let mut pass = Pass::new(candidates, &stored);
    pass.announce(context);
    pass.publish_queue(context);
    if pass.is_idle() {
        return;
    }
    let _automatic_queue = AutomaticQueueGuard(context.import.clone());
    let mut finishing = JoinSet::<Finished>::new();
    // Every settling task takes this token, a child of the pass's own. A pass
    // that abandons its answers cancels it and waits; one that stops taking new
    // candidates but keeps the answers it already has just waits.
    let settling = token.child_token();

    loop {
        while pass.in_flight_count() + finishing.len() < MAX_IN_FLIGHT {
            if !config.borrow().prefs.identify_automatically {
                context.release_all();
                drain(context, &mut finishing).await;
                announce_empty_queue(context);
                return;
            }
            let Some(mut job) = pass.next_job() else {
                break;
            };
            let Some(representative_index) = job
                .candidates
                .iter()
                .position(|candidate| !context.owned_elsewhere(candidate.key().as_ref()))
            else {
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
            let key = candidate.key().into_owned();
            let start = match candidate_run_start(context, &candidate).await {
                Ok(start) => start,
                Err(error) => {
                    warn!(
                            "sweep: cannot read what {key} runs from ({error}); aborting pass"
                        );
                    pass.release_in_flight(context);
                    settling.cancel();
                    drain(context, &mut finishing).await;
                    return;
                }
            };
            let CandidateRunStart {
                metadata_revision: expected_metadata_revision,
                choices,
            } = start;
            context.ours.lock().unwrap().insert(key.clone());
            let run = context.import.new_identification_run();
            context.import.start_identification(
                run,
                key.clone(),
                ExtractionSource::Candidate {
                    candidate: candidate.clone(),
                },
                CallPriority::Background,
                choices,
            );
            // The job is running now, so it is not waiting for a slot. The
            // members it shares an identity with still are: they are waiting
            // for the answer this run stores.
            context.import.clear_automatic_identification(&key);
            pass.track(key, job, run, expected_metadata_revision);
        }

        if pass.is_idle() && finishing.is_empty() {
            return;
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                // `settling` is a child of this token, so the answers in
                // flight are already told to stop; what is left is waiting
                // for them to say so.
                pass.release_in_flight(context);
                drain(context, &mut finishing).await;
                return;
            }
            changed = config.changed() => {
                if changed.is_err() || !config.borrow().prefs.identify_automatically {
                    // The runs the sweep has going are cancelled; the answers
                    // already being written are not. A candidate whose verdict
                    // is in flight keeps its write and its row lands.
                    context.release_all();
                    drain(context, &mut finishing).await;
                    announce_empty_queue(context);
                    return;
                }
            }
            Some(result) = finishing.join_next() => {
                match result {
                    Ok(done) => {
                        context.release_settled(&done.representative_key);
                        let deferred = pass.take_finishing_members(&done.identity);
                        let stored = matches!(&done.outcome, FinishCandidateOutcome::Stored);
                        match done.outcome {
                            FinishCandidateOutcome::Stored => {
                                for key in &done.candidate_keys {
                                    context.import.clear_automatic_identification(key);
                                }
                            }
                            FinishCandidateOutcome::Superseded => {
                                for candidate in
                                    done.current_candidates.into_iter().chain(deferred)
                                {
                                    pass.enqueue(context, candidate);
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
                                    let key = candidate.key();
                                    context.import.clear_automatic_identification(&key);
                                    context.import.fail_identification(
                                        &key,
                                        done.run,
                                        error.clone(),
                                    );
                                }
                            }
                        }
                        if !(stored && pass.answer_identity(context, &done.identity)) {
                            debug!(
                                "sweep: {} finished without a current stored verdict",
                                done.representative_key
                            );
                        }
                    }
                    Err(error) => warn!("sweep finishing task failed: {error}"),
                }
            }
            event = bus.recv() => match event {
                Some(Ok(ImportEvent::SignalsUpdated { candidate_key, signals, .. })) => {
                    pass.record_signals(&candidate_key, signals);
                }
                Some(Ok(ImportEvent::IdentifyStateChanged { candidate_key, run, state, .. })) => {
                    pass.settle(context, &settling, &mut finishing, &candidate_key, run, state);
                }
                // A person decided this candidate: they picked a release, said
                // File Tags, or cleared what it had. The command that decided
                // ended its run as part of its own write, so nothing is
                // cancelled here — the candidate simply stops being the pass's,
                // and every other candidate carries on to its verdict.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged {
                    candidate_key,
                }))) => {
                    pass.drop_candidate(context, &candidate_key);
                }
                // The folder was removed, renamed, or unmounted while we were
                // identifying it. Extraction is cancelled for us by the signal
                // service's own listener, so no further `Signals` will ever
                // arrive and the driver would sit in `Triangulating` forever —
                // holding a slot that never frees and stalling the pass, and
                // with it every later scan. The removal is an event, so react to
                // it rather than waiting out a clock.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }))) => {
                    pass.drop_candidate(context, &candidate_key);
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
                        pass.drop_candidate(context, &candidate_key);
                        continue;
                    };
                    let identity = candidate_identity(&candidate);
                    if pass.counts(&candidate_key, &identity) {
                        continue;
                    }
                    pass.recount(context, &candidate_key, identity.clone());
                    let stored_now = match current_stored_answer(context, &candidate).await {
                        Ok(stored) => stored,
                        Err(error) => {
                            warn!(
                                "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                            );
                            pass.release_in_flight(context);
                            settling.cancel();
                            drain(context, &mut finishing).await;
                            return;
                        }
                    };
                    // Already answered — either on disk, or by a candidate this
                    // pass settled that hashes the same.
                    if stored_now || pass.answered(&identity) {
                        pass.mark_answered(candidate_key, identity);
                    } else {
                        pass.defer_or_enqueue(context, &identity, candidate);
                    }
                    pass.announce(context);
                }
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key,
                    skipped,
                }))) => {
                    if skipped {
                        pass.drop_candidate(context, &candidate_key);
                    } else {
                        pass.detach(context, &candidate_key);
                        if let Some(candidate) = sweepable_candidate(context, &candidate_key).await {
                            let identity = candidate_identity(&candidate);
                            pass.count(candidate_key.clone(), identity.clone());
                            let stored_now =
                                match current_stored_answer(context, &candidate).await {
                                    Ok(stored) => stored,
                                    Err(error) => {
                                        warn!(
                                            "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                                        );
                                        pass.release_in_flight(context);
                                        settling.cancel();
                                        drain(context, &mut finishing).await;
                                        return;
                                    }
                                };
                            if stored_now {
                                pass.mark_answered(candidate_key, identity);
                            } else {
                                pass.enqueue(context, candidate);
                            }
                            pass.announce(context);
                        }
                    }
                }
                // The folder is a different shape now. A run already under way
                // is answering the shape it had, and letting it settle would
                // write that answer straight back over the one the binding
                // change just cleared — so drop it. The pass this event also
                // plans identifies the candidate again, against what it now is.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))) => {
                    let candidate = ReleaseCandidate::from(candidate);
                    let candidate_key = candidate.key().into_owned();
                    let identity = candidate_identity(&candidate);
                    pass.recount(context, &candidate_key, identity.clone());
                    pass.defer_or_enqueue(context, &identity, candidate);
                    pass.announce(context);
                }
                Some(Ok(ImportEvent::ImportProgress { candidate_key, .. })) => {
                    pass.drop_candidate(context, &candidate_key);
                }
                Some(Ok(_)) => {}
                Some(Err(broadcast::error::RecvError::Lagged(n))) => {
                    // A dropped `IdentifyStateChanged` would leave its candidate
                    // in flight with nothing left to wake it, so the pass would
                    // stall on a slot that never frees. Give the affected
                    // candidates back to the queue and run them again: nothing
                    // durable was written, so replaying them whole is the only
                    // shape that cannot leave a wrong answer behind.
                    warn!("sweep: import bus lagged by {n} events; replaying {} in-flight candidates", pass.in_flight_count());
                    context.library_manager.record_telemetry(
                        crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        },
                    );
                    pass.replay_in_flight(context);
                }
                Some(Err(broadcast::error::RecvError::Closed)) | None => return,
            },
        }
    }
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
async fn actionable_candidate(context: &SweepContext, key: &str) -> Option<ReleaseCandidate> {
    match context.import.get_release_candidate(key).await {
        Ok(candidate) => candidate,
        Err(error) => {
            warn!("cannot read candidate {key}: {error}");
            None
        }
    }
}

/// The candidate the sweep is responsible for at `key`, read exactly — see
/// [`ImportServiceHandle::sweepable_candidate`]. A read that fails answers
/// no candidate, and says so.
async fn sweepable_candidate(context: &SweepContext, key: &str) -> Option<ReleaseCandidate> {
    match context.import.sweepable_candidate(key).await {
        Ok(candidate) => candidate,
        Err(error) => {
            warn!("sweep: cannot read candidate {key} ({error}); treating it as not ours");
            None
        }
    }
}

/// What one run of a candidate begins from.
struct CandidateRunStart {
    /// The editable metadata revision the run answers. A later edit makes its
    /// terminal result stale even when the candidate's files did not change.
    metadata_revision: u64,
    /// What the person decided this candidate's identification asks about.
    choices: LookupChoices,
}

/// Read both in one go, off the one stored row that states them.
async fn candidate_run_start(
    context: &SweepContext,
    candidate: &ReleaseCandidate,
) -> Result<CandidateRunStart, crate::library::LibraryError> {
    context
        .library_manager
        .load_import_candidate_state(&candidate.files().content_hash())
        .await?
        .map(|state| CandidateRunStart {
            metadata_revision: state.metadata_revision,
            choices: state.lookup_choices,
        })
        .ok_or_else(|| {
            crate::library::LibraryError::Internal(format!(
                "candidate {} has no persisted state row",
                candidate.key()
            ))
        })
}

/// Announce that the sweep is answering nothing: automatic identification is
/// off, or the pass gave up before it counted a queue. [`Pass::announce`] is
/// what reports a queue it does have.
fn announce_empty_queue(context: &SweepContext) {
    context.import.announce_queue_identify_progress(0, 0);
}

#[cfg(test)]
mod tests;
