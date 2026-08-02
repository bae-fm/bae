//! Queue-wide identification: every candidate the scan produced acquires a
//! verdict, without anyone clicking it.
//!
//! The sweep owns no pipeline of its own. It walks the candidates the scan
//! already found, drives each through the existing extraction → identify pair
//! at [`CallPriority::Background`], and writes the terminal verdict to
//! `import_candidate_state`. What it adds over the per-selection path is
//! scheduling: which candidates still need answering, how many at once, and the
//! one settle step that buys a single match's documents — the tracklist that
//! decides Ready, and everything opening the candidate would otherwise re-fetch.
//!
//! **It starts and stops with the library, not with a view.**
//! [`crate::library::AppServices`] constructs one and its `Drop` stops it, so
//! the queue is identified whether or not anyone has the Import section open.
//! Opening a view triggers nothing.
//!
//! **It is the one writer of `import_candidate_state`'s verdict**, including
//! for runs it did not start: [`QueueSweepHandle::record_selection`] hangs a
//! recorder off a candidate a person opened, so their answer persists too.
//! Everything that decides what to store lives here rather than being spread
//! across the two producers. The row's other half — the user's sheet bindings —
//! is written by the import handle, and writing it *clears* the verdict, which
//! is what brings a re-bound candidate back to this sweep.
//!
//! **A candidate whose content hash already holds a finished verdict is
//! skipped**, which is what makes every launch after the first instant. There
//! is nothing left to finish on such a row: the settle step and the verdict are
//! written together, so a stored verdict is a settled one.
//!
//! **Nothing durable is written for work that did not complete.** A transport
//! failure, a cancelled shutdown, a settle lookup that never answered, a
//! candidate that vanished mid-flight — each leaves no row, and absence is the
//! retry signal. There are no attempt counters and no backoff, because a stored
//! failure is a stored answer, and the retry that would have succeeded then
//! never happens.

use super::folder_scanner::FolderCandidate;
use super::handle::{ImportCandidateSnapshot, ImportEvent, ImportServiceHandle, ScanEvent};
use crate::db::{DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::verdict::decode_stored as decode;
use crate::identify::{IdentifyServiceHandle, IdentifyState, TerminalVerdict};
use crate::import::MetadataRef;
use crate::library::LibraryManager;
use crate::signals::{ExtractionServiceHandle, ExtractionSource};
use crate::util::rate_limiter::CallPriority;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, warn};

/// How many candidates are identified at once.
///
/// The local half of a candidate — the folder walk, disc-ID derivation,
/// duration probing, artwork OCR — is CPU and disk work that parallelises, and
/// the network half is serialised by the provider rate limiter however many run
/// at once. So the cap exists to keep OCR off every core, not to pace the
/// network. A constant, not configuration: there is no setting a user could
/// meaningfully choose here.
const MAX_IN_FLIGHT: usize = 4;

/// The running queue sweep.
#[derive(Clone)]
pub struct QueueSweepHandle {
    context: SweepContext,
    token: CancellationToken,
    tasks: TaskTracker,
    runtime_handle: tokio::runtime::Handle,
    executor_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl QueueSweepHandle {
    /// Stop sweeping. In-flight candidates are cancelled and write no rows; the
    /// next launch picks them up, which is correct for the same reason a
    /// transport failure is — nothing was learned.
    ///
    /// This has to run *before* the tokio runtime is dropped, or every task it
    /// would cancel is already gone — see the field ordering on
    /// [`crate::app::RunningApp`].
    pub fn stop(&self) {
        self.token.cancel();
        self.tasks.close();
        let Some(executor_thread) = self.executor_thread.lock().unwrap().take() else {
            return;
        };
        if executor_thread.join().is_err() {
            warn!("queue sweep executor thread panicked during shutdown");
        }
    }

    /// Persist the verdict of a run a person started, so opening a candidate
    /// answers it for good rather than only for this session.
    ///
    /// Call this from the import selection path, not from the identify service:
    /// the same pipeline also re-identifies existing library releases
    /// (`ExtractionSource::Release`), which have no candidate folder, no content
    /// hash to key a row by, and no probed duration. The recorder resolves its
    /// candidate through the import service's scanned set, so a re-identify key
    /// finds nothing there and writes nothing — a second guard on the same
    /// fact.
    ///
    /// The verdict settles before it stores, here as in the sweep's own pass:
    /// the lead's documents are bought and written first, so a candidate a
    /// person answered opens with no network on every launch after.
    /// Identify a folder candidate for a person who is looking at it.
    ///
    /// The three steps have to happen together and in this order: the recorder
    /// has to be watching before a candidate with no signals settles on its
    /// first step, and identify takes its bus subscription synchronously so
    /// extraction cannot start ahead of it. Composed here rather than at each
    /// surface so there is one ordering to get right.
    pub fn identify_for_selection(&self, candidate_key: String) {
        let Some(ImportCandidateSnapshot::Folder {
            candidate,
            actionable: true,
            ..
        }) = self.context.import.get_candidate(&candidate_key)
        else {
            warn!("cannot identify selection {candidate_key}: it is not a folder candidate");
            return;
        };
        self.record_selection(candidate_key.clone());
        self.context
            .identify
            .start(candidate_key.clone(), CallPriority::Interactive);
        self.context.extraction.start(
            candidate_key,
            ExtractionSource::Folder {
                path: candidate.path,
                files: candidate.files,
            },
            CallPriority::Interactive,
        );
    }

    pub fn record_selection(&self, candidate_key: String) {
        let context = self.context.clone();
        let token = self.token.child_token();
        if self.token.is_cancelled() {
            return;
        }
        self.tasks.spawn_on(
            async move {
                record_selection_verdict(&context, candidate_key, &token).await;
            },
            &self.runtime_handle,
        );
    }
}

/// What one pass needs to reach. Grouped because every one of them is required
/// and they are always passed together.
#[derive(Clone)]
struct SweepContext {
    import: ImportServiceHandle,
    identify: IdentifyServiceHandle,
    extraction: ExtractionServiceHandle,
    library_manager: LibraryManager,
    /// Candidate keys the sweep currently has drivers running for. Entries are
    /// added when a run starts and removed when it is finished with — the sweep
    /// cancels its own drivers once they settle, so this never accumulates keys
    /// it no longer owns and can never mistake a candidate the user has since
    /// opened for one of its own.
    ours: Arc<Mutex<HashSet<String>>>,
}

impl SweepContext {
    /// Whether this candidate belongs to someone else — the user opened it, and
    /// their run finishes at its own priority. `IdentifyServiceHandle::start`
    /// supersedes, so starting one here would cancel theirs and restart it in
    /// the background, which is the opposite of what the priority is for.
    ///
    /// Asked at the moment a run would start, never cached: between planning a
    /// pass and reaching a candidate, someone can open it.
    fn owned_elsewhere(&self, key: &str) -> bool {
        self.identify.is_running(key) && !self.ours.lock().unwrap().contains(key)
    }

    /// Take a candidate the sweep started: cancel its driver and its extraction,
    /// and give up ownership.
    ///
    /// The sweep never toggles a signal or re-runs, so a settled driver of its
    /// own is pure cost — `run_driver` only ends via `Cancelled`, so each one
    /// left alive parks a task, a bus-relay task, and a live broadcast receiver
    /// that every later `IdentifyStateChanged` is deep-cloned into. Over a whole
    /// queue that is fan-out quadratic in its size. A driver a *person* started
    /// is not touched: they can still toggle and re-run it.
    fn release(&self, key: &str) {
        self.identify.cancel(key);
        self.extraction.cancel(key);
        self.ours.lock().unwrap().remove(key);
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
                        | ScanEvent::CandidateSkipChanged { .. },
                    ))) => {
                        run_pass(&loop_context, &loop_token, &mut event_rx).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(broadcast::error::RecvError::Lagged(n))) => {
                        warn!("sweep: import bus lagged by {n} events; planning a pass in case a scan finished inside the gap");
                        run_pass(&loop_context, &loop_token, &mut event_rx).await;
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

    QueueSweepHandle {
        context,
        token,
        tasks,
        runtime_handle,
        executor_thread: Arc::new(Mutex::new(Some(executor_thread))),
    }
}

/// One candidate the pass is driving: what it will need once a verdict lands.
struct InFlight {
    job: IdentifyJob,
    /// The probed total from the candidate's latest `SignalsUpdated`. `0` until
    /// the fast pass reports one, and `0` forever for audio that would not
    /// probe — see [`crate::signals::Signals::probed_total_duration_ms`].
    probed_total_duration_ms: u64,
}

struct SelectionInFlight {
    candidate: FolderCandidate,
    probed_total_duration_ms: u64,
}

/// What a finished candidate reports back to the pass loop.
struct Finished {
    representative_key: String,
    identity: CandidateIdentity,
    rehome: Vec<FolderCandidate>,
    stored: bool,
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
}

/// What a pass has to do, decided against the stored rows before any of it
/// starts.
struct Plan {
    /// Candidates with no usable stored verdict: identify them.
    identify: VecDeque<IdentifyJob>,
    /// How many of `total` already hold a verdict.
    identified: u32,
    total: u32,
}

/// Walk the queue once: plan what still needs answering, drive it under the
/// concurrency cap, and report progress as verdicts land.
async fn run_pass(
    context: &SweepContext,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<Result<ImportEvent, broadcast::error::RecvError>>,
) {
    let candidates = new_candidates(context);
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
            warn!("sweep: could not read stored verdicts ({e}); skipping this pass");
            return;
        }
    };
    for row in stored.values() {
        if let Err(error) = decode(row) {
            warn!("sweep: {error}; skipping this pass");
            return;
        }
    }

    let mut answered_keys: HashSet<String> = candidates
        .iter()
        .filter(|candidate| usable_stored_row(&stored, candidate).is_some())
        .map(|candidate| candidate.path.to_string_lossy().into_owned())
        .collect();
    let mut answered_identities: HashSet<CandidateIdentity> = candidates
        .iter()
        .filter(|candidate| usable_stored_row(&stored, candidate).is_some())
        .map(candidate_identity)
        .collect();

    let Plan {
        identify: mut pending,
        mut identified,
        mut total,
    } = plan(candidates, &stored, total);
    emit_progress(context, identified, total);

    if pending.is_empty() {
        return;
    }

    let mut in_flight: HashMap<String, InFlight> = HashMap::new();
    let mut finishing_members: HashMap<CandidateIdentity, Vec<FolderCandidate>> = HashMap::new();
    let mut finishing = JoinSet::<Finished>::new();

    loop {
        while in_flight.len() + finishing.len() < MAX_IN_FLIGHT {
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
                continue;
            };
            job.candidates.swap(0, representative_index);
            let candidate = job.representative().clone();
            let key = candidate.path.to_string_lossy().into_owned();
            context.ours.lock().unwrap().insert(key.clone());
            // Identify first: it takes its bus subscription synchronously, so
            // extraction's first snapshot cannot be emitted into a void.
            context
                .identify
                .start(key.clone(), CallPriority::Background);
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
                    probed_total_duration_ms: 0,
                },
            );
        }

        if in_flight.is_empty() && pending.is_empty() && finishing.is_empty() {
            return;
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                for key in in_flight.keys() {
                    context.release(key);
                }
                finishing.shutdown().await;
                return;
            }
            Some(result) = finishing.join_next() => {
                match result {
                    Ok(done) => {
                        context.release(&done.representative_key);
                        let deferred = finishing_members
                            .remove(&done.identity)
                            .expect("finishing identity is registered before its task starts");
                        if done.stored {
                            answered_identities.insert(done.identity.clone());
                            pending.retain(|job| job.identity != done.identity);
                        } else {
                            for candidate in done.rehome {
                                enqueue_candidate(&mut pending, candidate);
                            }
                            for candidate in deferred {
                                enqueue_candidate(&mut pending, candidate);
                            }
                        }
                        let newly_answered = known_identities
                            .iter()
                            .filter(|(_, identity)| *identity == &done.identity)
                            .map(|(key, _)| key)
                            .filter(|key| done.stored && answered_keys.insert((*key).clone()))
                            .count() as u32;
                        if newly_answered > 0 {
                            identified = identified.saturating_add(newly_answered).min(total);
                            emit_progress(context, identified, total);
                        } else {
                            debug!(
                                "sweep: {} learned nothing; it is retried next pass",
                                done.representative_key
                            );
                        }
                    }
                    Err(error) if error.is_cancelled() && token.is_cancelled() => return,
                    Err(error) => warn!("sweep finishing task failed: {error}"),
                }
            }
            event = bus.recv() => match event {
                Some(Ok(ImportEvent::SignalsUpdated { candidate_key, signals, .. })) => {
                    if let Some(entry) = in_flight.get_mut(&candidate_key) {
                        entry.probed_total_duration_ms = signals.probed_total_duration_ms;
                    }
                }
                Some(Ok(ImportEvent::IdentifyStateChanged { candidate_key, state, .. })) => {
                    // Terminal only means the machine stopped moving; whether
                    // what it stopped on is storable is `finish_candidate`'s
                    // question. Either way the candidate's slot is free now.
                    let settled = state
                        .is_terminal()
                        .then(|| in_flight.remove(&candidate_key))
                        .flatten();
                    if let Some(entry) = settled {
                        let identity = entry.job.identity.clone();
                        finishing_members.insert(identity.clone(), Vec::new());
                        let representative_key = candidate_key.clone();
                        let context = context.clone();
                        let child = token.child_token();
                        finishing.spawn(async move {
                            let stored = finish_candidate(&context, &entry, state, &child).await;
                            let rehome = if stored
                                || usable_current_candidate(
                                    &context,
                                    &representative_key,
                                    &identity,
                                )
                            {
                                Vec::new()
                            } else {
                                entry
                                    .job
                                    .candidates
                                    .iter()
                                    .filter(|candidate| {
                                        candidate.path.to_string_lossy() != representative_key
                                            && usable_current_candidate(
                                                &context,
                                                candidate.path.to_string_lossy().as_ref(),
                                                &identity,
                                            )
                                    })
                                    .cloned()
                                    .collect()
                            };
                            Finished {
                                representative_key,
                                identity,
                                rehome,
                                stored,
                            }
                        });
                    }
                }
                // The folder was removed, renamed, or unmounted while we were
                // identifying it. Extraction is cancelled for us by the signal
                // service's own listener, so no further `Signals` will ever
                // arrive and the driver would sit in `Triangulating` forever —
                // holding a slot that never frees and stalling the pass, and
                // with it every later scan. The removal is an event, so react to
                // it rather than waiting out a clock.
                Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }))) => {
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
                Some(Ok(ImportEvent::Scan(ScanEvent::FolderCandidate {
                    candidate,
                    skipped,
                    is_added,
                }))) => {
                    let candidate_key = candidate.path.to_string_lossy().into_owned();
                    if skipped || is_added {
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
                    }
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
                    let stored_now = match current_stored_verdict(context, &candidate).await {
                        Ok(stored) => stored,
                        Err(error) => {
                            warn!(
                                "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                            );
                            for key in in_flight.keys() {
                                context.release(key);
                            }
                            finishing.shutdown().await;
                            return;
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
                        enqueue_candidate(&mut pending, candidate);
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
                    } else if let Some(candidate) = sweepable_candidate(context, &candidate_key) {
                        let identity = candidate_identity(&candidate);
                        if known_identities
                            .insert(candidate_key.clone(), identity.clone())
                            .is_none()
                        {
                            total = total.saturating_add(1);
                        }
                        let stored_now =
                            match current_stored_verdict(context, &candidate).await {
                                Ok(stored) => stored,
                                Err(error) => {
                                    warn!(
                                        "sweep: could not check current verdict for {candidate_key} ({error}); aborting pass"
                                    );
                                    for key in in_flight.keys() {
                                        context.release(key);
                                    }
                                    finishing.shutdown().await;
                                    return;
                                }
                            };
                        if stored_now {
                            answered_identities.insert(identity);
                            if answered_keys.insert(candidate_key.clone()) {
                                identified = identified.saturating_add(1).min(total);
                            }
                        } else {
                            enqueue_candidate(&mut pending, candidate);
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
                        enqueue_candidate(&mut pending, candidate);
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
                    context.library_manager.diagnostics().event(
                        crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        },
                    );
                    for (key, entry) in in_flight.drain() {
                        context.release(&key);
                        pending.push_back(entry.job);
                    }
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
    run_pass(context, token, &mut event_rx).await;
    relay.abort();
}

fn enqueue_candidate(pending: &mut VecDeque<IdentifyJob>, candidate: FolderCandidate) {
    let identity = candidate_identity(&candidate);
    if let Some(job) = pending.iter_mut().find(|job| job.identity == identity) {
        job.candidates.push(candidate);
    } else {
        pending.push_back(IdentifyJob {
            identity,
            candidates: vec![candidate],
        });
    }
}

fn detach_candidate(
    context: &SweepContext,
    candidate_key: &str,
    in_flight: &mut HashMap<String, InFlight>,
    pending: &mut VecDeque<IdentifyJob>,
) {
    let representative = in_flight.iter().find_map(|(representative, entry)| {
        entry
            .job
            .candidates
            .iter()
            .any(|member| member.path.to_string_lossy() == candidate_key)
            .then(|| representative.clone())
    });
    if let Some(representative) = representative {
        let mut entry = in_flight
            .remove(&representative)
            .expect("located in-flight job still exists");
        entry
            .job
            .candidates
            .retain(|member| member.path.to_string_lossy() != candidate_key);
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
        job.candidates
            .retain(|candidate| candidate.path.to_string_lossy() != candidate_key);
        !job.candidates.is_empty()
    });
}

fn remove_finishing_member(
    finishing_members: &mut HashMap<CandidateIdentity, Vec<FolderCandidate>>,
    candidate_key: &str,
) {
    for members in finishing_members.values_mut() {
        members.retain(|candidate| candidate.path.to_string_lossy() != candidate_key);
    }
}

fn forget_candidate(
    candidate_key: &str,
    known_identities: &mut HashMap<String, CandidateIdentity>,
    answered_keys: &mut HashSet<String>,
    answered_identities: &mut HashSet<CandidateIdentity>,
    identified: &mut u32,
    total: &mut u32,
) -> bool {
    let Some(identity) = known_identities.remove(candidate_key) else {
        return false;
    };
    *total = total.saturating_sub(1);
    if answered_keys.remove(candidate_key) {
        *identified = identified.saturating_sub(1);
    }
    if !known_identities
        .values()
        .any(|known_identity| known_identity == &identity)
    {
        answered_identities.remove(&identity);
    }
    true
}

fn candidate_identity(candidate: &FolderCandidate) -> CandidateIdentity {
    (candidate.files.content_hash(), candidate.file_edit_revision)
}

fn usable_stored_row<'a>(
    stored: &'a HashMap<String, DbImportCandidateState>,
    candidate: &FolderCandidate,
) -> Option<&'a DbImportCandidateState> {
    stored
        .get(&candidate.files.content_hash())
        .filter(|row| row.file_edits.revision == candidate.file_edit_revision)
        .filter(|row| {
            decode(row)
                .expect("stored verdicts are validated before sweep planning")
                .is_some()
        })
}

fn usable_current_candidate(
    context: &SweepContext,
    key: &str,
    identity: &CandidateIdentity,
) -> bool {
    sweepable_candidate(context, key)
        .is_some_and(|candidate| candidate_identity(&candidate) == *identity)
}

/// Whether this candidate, as it is on disk right now, already holds a stored
/// verdict for that shape.
async fn current_stored_verdict(
    context: &SweepContext,
    candidate: &FolderCandidate,
) -> Result<bool, String> {
    let Some(row) = context
        .library_manager
        .load_import_candidate_state(&candidate.files.content_hash())
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    if row.file_edits.revision != candidate.file_edit_revision {
        return Ok(false);
    }
    Ok(decode(&row).map_err(|error| error.to_string())?.is_some())
}

/// Split the queue against what is already stored.
fn plan(
    candidates: Vec<FolderCandidate>,
    stored: &HashMap<String, DbImportCandidateState>,
    total: u32,
) -> Plan {
    let mut identify = VecDeque::new();
    let mut identified = 0;
    let mut grouped = Vec::<IdentifyJob>::new();
    for candidate in candidates {
        let identity = candidate_identity(&candidate);
        if let Some(job) = grouped.iter_mut().find(|job| job.identity == identity) {
            job.candidates.push(candidate);
        } else {
            grouped.push(IdentifyJob {
                identity,
                candidates: vec![candidate],
            });
        }
    }
    for job in grouped {
        let candidate = job.representative();
        // A row with no identify result is a candidate nobody has answered —
        // either never, or not since a sheet binding changed what the folder is
        // and cleared the answer it had.
        //
        // Stored rows are decoded and validated before planning. A malformed
        // row fails the pass rather than being treated as no answer.
        if usable_stored_row(stored, candidate).is_none() {
            identify.push_back(job);
            continue;
        }
        identified += job.candidates.len() as u32;
    }
    Plan {
        identify,
        identified,
        total,
    }
}

/// Turn one candidate's terminal state into a stored row, or into nothing.
/// Returns whether a row was written.
async fn finish_candidate(
    context: &SweepContext,
    entry: &InFlight,
    state: IdentifyState,
    token: &CancellationToken,
) -> bool {
    // The one gate on storability. "Terminal" only means nothing is in flight;
    // a `Found` built from the one signal whose lookup survived, or a
    // `NotFoundAnywhere` where neither lookup ever answered, is terminal and is
    // not an answer. The conversion refuses those, and refusing means no row,
    // which means the next pass asks again.
    let Ok(mut verdict) = TerminalVerdict::try_from(state) else {
        return false;
    };

    if !settle_lead(context, &mut verdict, token).await {
        return false;
    }

    let candidate = entry.job.representative();
    save(
        context,
        token,
        &candidate.path.to_string_lossy(),
        &candidate.files.content_hash(),
        &candidate.path.to_string_lossy(),
        &verdict,
        entry.probed_total_duration_ms as i64,
        candidate.file_edit_revision,
    )
    .await
}

/// Write one row. Cancellation is re-checked immediately before the write, not
/// only before the lookup that precedes it: teardown during that lookup must
/// leave nothing behind, and "a cancelled candidate writes no row" is only true
/// if the last thing checked before writing is the token.
#[allow(clippy::too_many_arguments)]
async fn save(
    context: &SweepContext,
    token: &CancellationToken,
    candidate_key: &str,
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: i64,
    expected_edit_revision: u64,
) -> bool {
    if token.is_cancelled() {
        return false;
    }
    let verdict = match serde_json::to_string(verdict) {
        Ok(json) => json,
        Err(e) => {
            warn!("sweep: could not encode a verdict ({e}); writing no row");
            return false;
        }
    };
    let row = NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict,
        probed_total_duration_ms,
        expected_edit_revision,
    };
    let wrote = match context
        .import
        .save_candidate_verdict_if_current(candidate_key, &row)
        .await
    {
        Ok(wrote) => wrote,
        Err(e) => {
            warn!(
                "sweep: could not store the verdict for {} ({e}); it is retried next pass",
                row.folder_path
            );
            return false;
        }
    };
    if !wrote {
        debug!(
            "sweep: discarded stale verdict for {} at file-edit revision {}",
            row.folder_path, expected_edit_revision
        );
        return false;
    }
    super::handle::send_event(
        &context.import.event_tx,
        ImportEvent::Scan(ScanEvent::CandidateVerdictStored {
            candidate_key: candidate_key.to_string(),
        }),
    );
    true
}

/// Settle a candidate's lead: buy the documents that describe the release it
/// matched, store them, and read the source's own tracklist out of what came
/// back. Returns whether the verdict may now be stored.
///
/// **The documents land before the verdict does.** A stored verdict whose lead
/// carries a tracklist is the queue's promise that opening that candidate needs
/// no network, so the two are written in this order and a failure here writes
/// neither — the next pass asks again, exactly as a failed lookup does.
///
/// Only a single-match `Found` has a lead. Several matches and a conflict are
/// questions for a person, answered from the result rows the verdict already
/// carries, and a full fetch of every pressing on the list would buy a
/// classification that cannot change.
///
/// A release some other candidate already settled costs nothing: its documents
/// are read back and the tracklist re-derived from them.
async fn settle_lead(
    context: &SweepContext,
    verdict: &mut TerminalVerdict,
    token: &CancellationToken,
) -> bool {
    let TerminalVerdict::Found { matches, .. } = verdict else {
        return true;
    };
    let [only_match] = matches.as_mut_slice() else {
        return true;
    };
    let release = MetadataRef::new(&only_match.release_id, only_match.source);

    let settle = crate::import::service::prepare_release(
        &context.library_manager,
        &release,
        CallPriority::Background,
    );
    let payloads = tokio::select! {
        biased;
        // Shutdown mid-lookup is a transport failure by another name: nothing
        // was learned, so nothing is written and the next launch asks again.
        _ = token.cancelled() => return false,
        payloads = settle => payloads,
    };
    let payloads = match payloads {
        Ok(payloads) => payloads,
        Err(error) => {
            debug!(
                "sweep: could not settle {} ({error}); writing no row",
                only_match.release_id
            );
            return false;
        }
    };
    match payloads.source_tracks() {
        Ok(source_tracks) => {
            // `SourceTracks::Nothing` is an answer — this release states no
            // tracklist — so the verdict stores with the match unverifiable, and
            // the Ready rule lands it in Needs you rather than admitting it.
            only_match.source_tracks = Some(source_tracks);
            true
        }
        Err(error) => {
            debug!(
                "sweep: {} states no readable tracklist ({error}); writing no row",
                only_match.release_id
            );
            false
        }
    }
}

/// Watch a run a person started and store the first verdict it reaches.
///
/// **The first that stores, not the first that settles.** A terminal state
/// shaped by a lookup that never answered does not convert, so a re-run from the
/// toolbar after a network blip is still captured. Once one verdict is stored
/// the watch ends: a signal the user then toggles off is them filtering their
/// own view, not a durable fact about the folder, and persisting it would leave
/// the next launch showing a queue narrowed by exclusions nobody remembers
/// making.
async fn record_selection_verdict(
    context: &SweepContext,
    candidate_key: String,
    token: &CancellationToken,
) {
    // Subscribe before reading the candidate, so no state change can land in
    // between.
    let mut bus = context.import.subscribe_events();
    let Some(candidate) = folder_candidate(context, &candidate_key) else {
        // Not a scanned folder candidate — a library release being
        // re-identified. It has no content hash to key a row by.
        return;
    };
    let mut entry = SelectionInFlight {
        candidate,
        probed_total_duration_ms: 0,
    };

    loop {
        let event = tokio::select! {
            biased;
            _ = token.cancelled() => return,
            event = bus.recv() => event,
        };
        match event {
            Ok(ImportEvent::SignalsUpdated {
                candidate_key: key,
                signals,
                ..
            }) if key == candidate_key => {
                entry.probed_total_duration_ms = signals.probed_total_duration_ms;
            }
            Ok(ImportEvent::IdentifyStateChanged {
                candidate_key: key,
                state,
                ..
            }) if key == candidate_key => {
                if matches!(state, IdentifyState::Idle) {
                    // The run was cancelled — the user dismissed the candidate,
                    // or the sweep took it over. Either way this watch is done.
                    return;
                }
                if !state.is_terminal() {
                    continue;
                }
                let Ok(mut verdict) = TerminalVerdict::try_from(state) else {
                    // Terminal but shaped by a failed lookup. Keep watching: a
                    // re-run from the toolbar may still answer it.
                    continue;
                };
                // Settles here too: a row a person's own run wrote is a row the
                // queue treats as answered, and the promise that an answered
                // lead opens offline holds however the answer was reached.
                if !settle_lead(context, &mut verdict, token).await {
                    continue;
                }
                if save(
                    context,
                    token,
                    &candidate_key,
                    &entry.candidate.files.content_hash(),
                    &entry.candidate.path.to_string_lossy(),
                    &verdict,
                    entry.probed_total_duration_ms as i64,
                    entry.candidate.file_edit_revision,
                )
                .await
                {
                    return;
                }
            }
            // The candidate is gone, or is a different shape than the run was
            // answering. Either way this watch has nothing left to store: a
            // verdict written now would describe the folder as it was before
            // the binding changed, which is exactly the stale answer the change
            // cleared.
            Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key }))
                if key == candidate_key =>
            {
                return;
            }
            Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))
                if candidate.path.to_string_lossy() == candidate_key.as_str() =>
            {
                return;
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(
                    "sweep: selection recorder for {candidate_key} lagged by {n} events; writing no verdict"
                );
                return;
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// The scanned folder candidate behind a key, or `None` when the key names
/// something else (a library release being re-identified) or nothing.
fn folder_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    match context.import.get_candidate(key)? {
        ImportCandidateSnapshot::Folder {
            candidate,
            actionable: true,
            ..
        } => Some(candidate),
        ImportCandidateSnapshot::Folder {
            actionable: false, ..
        } => None,
        ImportCandidateSnapshot::Invalid(_) | ImportCandidateSnapshot::Runtime { .. } => None,
    }
}

/// The candidates the sweep is responsible for: New ones only.
///
/// Added candidates are already in the library and skipped candidates reflect
/// an explicit user decision, so neither belongs in automatic identification.
fn new_candidates(context: &SweepContext) -> Vec<FolderCandidate> {
    context
        .import
        .get_import_candidates()
        .folder_candidates
        .into_iter()
        .filter(|snapshot| {
            snapshot.actionable
                && !snapshot.skipped
                && !snapshot.is_added
                && snapshot.runtime.import_status.is_none()
        })
        .map(|snapshot| snapshot.candidate)
        .collect()
}

fn sweepable_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    match context.import.get_candidate(key) {
        Some(ImportCandidateSnapshot::Folder {
            candidate,
            runtime,
            actionable: true,
            skipped: false,
            is_added: false,
        }) if runtime.import_status.is_none() => Some(candidate),
        _ => None,
    }
}

/// Announce how much of the queue has been answered.
///
/// Both numbers are the sweep's, not the UI's: the total is how many candidates
/// the sweep is responsible for, which is a domain fact about the queue and not
/// something a view can infer from the rows it happens to be holding.
fn emit_progress(context: &SweepContext, identified: u32, total: u32) {
    super::handle::send_event(
        &context.import.event_tx,
        ImportEvent::QueueIdentifyProgress { identified, total },
    );
}

#[cfg(test)]
mod tests;
