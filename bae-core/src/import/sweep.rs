//! Queue-wide identification: every candidate the scan produced acquires a
//! verdict, without anyone clicking it.
//!
//! The sweep owns no pipeline of its own. It walks the candidates the scan
//! already found, drives each through the existing extraction → identify pair
//! at [`CallPriority::Background`], and writes the terminal verdict to
//! `import_candidate_state`. What it adds over the per-selection path is
//! scheduling: which candidates still need answering, how many at once, and one
//! paid lookup for the single matches that cannot otherwise be checked.
//!
//! **It starts and stops with the library, not with a view.**
//! [`crate::library::AppServices`] constructs one and its `Drop` stops it, so
//! the queue is identified whether or not anyone has the Import section open.
//! Opening a view triggers nothing.
//!
//! **It is also the one writer of `import_candidate_state`**, including for
//! runs it did not start: [`QueueSweepHandle::record_selection`] hangs a
//! recorder off a candidate a person opened, so their answer persists too.
//! Everything that decides what to store lives here rather than being spread
//! across the two producers.
//!
//! **A candidate whose content hash already holds a finished verdict is
//! skipped**, which is what makes every launch after the first instant. A row
//! that is a verdict but still owes the paid tracklist lookup is topped up
//! instead — one lookup, no re-identification.
//!
//! **Nothing durable is written for work that did not complete.** A transport
//! failure, a cancelled shutdown, a paid lookup that never answered, a
//! candidate that vanished mid-flight — each leaves no row, and absence is the
//! retry signal. There are no attempt counters and no backoff, because a stored
//! failure is a stored answer, and the retry that would have succeeded then
//! never happens.

use super::folder_scanner::FolderCandidate;
use super::handle::{ImportCandidateSnapshot, ImportEvent, ImportServiceHandle, ScanEvent};
use crate::db::{DbImportCandidateState, NewImportCandidateState};
use crate::identify::{IdentifyServiceHandle, IdentifyState, TerminalVerdict};
use crate::import::search::{fetch_source_tracks, MetadataResult};
use crate::library::LibraryManager;
use crate::signals::{ExtractionServiceHandle, ExtractionSource};
use crate::util::rate_limiter::CallPriority;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
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
    /// No paid lookup happens here: nothing on the path of an opened candidate
    /// buys the source's tracklist. The row lands with `source_tracks: None`,
    /// and the next sweep tops it up.
    /// Identify a folder candidate for a person who is looking at it.
    ///
    /// The three steps have to happen together and in this order: the recorder
    /// has to be watching before a candidate with no signals settles on its
    /// first step, and identify takes its bus subscription synchronously so
    /// extraction cannot start ahead of it. Composed here rather than at each
    /// surface so there is one ordering to get right.
    pub fn identify_for_selection(&self, candidate_key: String, folder: std::path::PathBuf) {
        self.record_selection(candidate_key.clone());
        self.context
            .identify
            .start(candidate_key.clone(), CallPriority::Interactive);
        self.context.extraction.start(
            candidate_key,
            ExtractionSource::Folder(folder),
            CallPriority::Interactive,
        );
    }

    pub fn record_selection(&self, candidate_key: String) {
        let context = self.context.clone();
        let token = self.token.child_token();
        // The library's own runtime, not an ambient one: the bridge calls this
        // from a plain synchronous method, exactly as `identify.start` is
        // called.
        context
            .library_manager
            .runtime_handle()
            .clone()
            .spawn(async move {
                record_selection_verdict(&context, candidate_key, &token).await;
            });
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

/// Start the queue sweep. It plans a pass whenever a folder scan finishes —
/// which includes the scan the app runs at launch — and does nothing in
/// between.
pub fn start(
    runtime_handle: &tokio::runtime::Handle,
    import: ImportServiceHandle,
    identify: IdentifyServiceHandle,
    extraction: ExtractionServiceHandle,
    library_manager: LibraryManager,
) -> QueueSweepHandle {
    let token = CancellationToken::new();
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
    let loop_token = token.clone();
    let loop_context = context.clone();
    runtime_handle.spawn(async move {
        loop {
            let event = tokio::select! {
                biased;
                _ = loop_token.cancelled() => return,
                event = bus.recv() => event,
            };
            // A pass over an already-answered queue is one DB read and one
            // event, so a scan that changed nothing costs nothing and there is
            // no debounce to get wrong. Scans finishing while a pass runs queue
            // up behind it and produce another pass, which is what makes a
            // folder added mid-sweep get picked up.
            match event {
                Ok(ImportEvent::Scan(ScanEvent::Finished)) => {
                    run_pass(&loop_context, &loop_token).await;
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("sweep: import bus lagged by {n} events; planning a pass in case a scan finished inside the gap");
                    run_pass(&loop_context, &loop_token).await;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    QueueSweepHandle { context, token }
}

/// One candidate the pass is driving: what it will need once a verdict lands.
struct InFlight {
    candidate: FolderCandidate,
    /// The probed total from the candidate's latest `SignalsUpdated`. `0` until
    /// the fast pass reports one, and `0` forever for audio that would not
    /// probe — see [`crate::signals::Signals::probed_total_duration_ms`].
    probed_total_duration_ms: u64,
}

/// What a finished candidate reports back to the pass loop.
struct Finished {
    key: String,
    stored: bool,
}

/// What a pass has to do, decided against the stored rows before any of it
/// starts.
struct Plan {
    /// Candidates with no usable stored verdict: identify them.
    identify: VecDeque<FolderCandidate>,
    /// Rows that are already a verdict but still owe the paid tracklist lookup.
    /// Topping one up is one provider call over a stored row — no folder walk,
    /// no OCR, no identify run.
    top_up: Vec<DbImportCandidateState>,
    /// How many of `total` already hold a verdict, top-up owing or not.
    identified: u32,
    total: u32,
}

/// Walk the queue once: plan what still needs answering, drive it under the
/// concurrency cap, and report progress as verdicts land.
async fn run_pass(context: &SweepContext, token: &CancellationToken) {
    let candidates = new_candidates(context);
    let total = candidates.len() as u32;

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

    let Plan {
        identify: mut pending,
        top_up,
        mut identified,
        total,
    } = plan(candidates, &stored, total);
    emit_progress(context, identified, total);

    // The top-ups are provider calls over rows that already exist, so they
    // neither occupy an identify slot nor change the identified count. They run
    // first: they are the cheapest thing that can promote a candidate to Ready.
    for row in top_up {
        if token.is_cancelled() {
            return;
        }
        top_up_row(context, row, token).await;
    }

    if pending.is_empty() {
        return;
    }

    let mut in_flight: HashMap<String, InFlight> = HashMap::new();
    let mut outstanding: usize = 0;
    let (finished_tx, mut finished_rx) = mpsc::unbounded_channel::<Finished>();
    let mut bus = context.import.subscribe_events();

    loop {
        while in_flight.len() < MAX_IN_FLIGHT {
            let Some(candidate) = pending.pop_front() else {
                break;
            };
            let key = candidate.path.to_string_lossy().into_owned();
            if context.owned_elsewhere(&key) {
                debug!("sweep: {key} is already being identified elsewhere; leaving it");
                continue;
            }
            context.ours.lock().unwrap().insert(key.clone());
            // Identify first: it takes its bus subscription synchronously, so
            // extraction's first snapshot cannot be emitted into a void.
            context
                .identify
                .start(key.clone(), CallPriority::Background);
            context.extraction.start(
                key.clone(),
                ExtractionSource::Folder(candidate.path.clone()),
                CallPriority::Background,
            );
            in_flight.insert(
                key,
                InFlight {
                    candidate,
                    probed_total_duration_ms: 0,
                },
            );
        }

        if in_flight.is_empty() && pending.is_empty() && outstanding == 0 {
            return;
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                for key in in_flight.keys() {
                    context.release(key);
                }
                return;
            }
            Some(done) = finished_rx.recv() => {
                outstanding -= 1;
                context.release(&done.key);
                if done.stored {
                    identified += 1;
                    emit_progress(context, identified, total);
                } else {
                    debug!("sweep: {} learned nothing; it is retried next pass", done.key);
                }
            }
            event = bus.recv() => match event {
                Ok(ImportEvent::SignalsUpdated { candidate_key, signals, .. }) => {
                    if let Some(entry) = in_flight.get_mut(&candidate_key) {
                        entry.probed_total_duration_ms = signals.probed_total_duration_ms;
                    }
                }
                Ok(ImportEvent::IdentifyStateChanged { candidate_key, state, .. }) => {
                    // Terminal only means the machine stopped moving; whether
                    // what it stopped on is storable is `finish_candidate`'s
                    // question. Either way the candidate's slot is free now.
                    let settled = state
                        .is_terminal()
                        .then(|| in_flight.remove(&candidate_key))
                        .flatten();
                    if let Some(entry) = settled {
                        outstanding += 1;
                        let context = context.clone();
                        let finished_tx = finished_tx.clone();
                        let child = token.child_token();
                        tokio::spawn(async move {
                            let stored = finish_candidate(&context, &entry, state, &child).await;
                            let _ = finished_tx.send(Finished {
                                key: candidate_key,
                                stored,
                            });
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
                Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })) => {
                    if in_flight.remove(&candidate_key).is_some() {
                        debug!("sweep: {candidate_key} was removed mid-identification; dropping it");
                        context.release(&candidate_key);
                    }
                    pending.retain(|candidate| {
                        candidate.path.to_string_lossy() != candidate_key.as_str()
                    });
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => {
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
                        pending.push_back(entry.candidate);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

/// Split the queue against what is already stored.
fn plan(
    candidates: Vec<FolderCandidate>,
    stored: &HashMap<String, DbImportCandidateState>,
    total: u32,
) -> Plan {
    let mut identify = VecDeque::new();
    let mut top_up = Vec::new();
    let mut identified = 0;
    for candidate in candidates {
        // A row this build can no longer decode is treated as **absent**, not as
        // an error: the shape of a stored verdict changes with the code that
        // writes it, and this is pre-1.0 — so the honest response to a row this
        // build cannot read is to identify the candidate again and overwrite it.
        // No fallback decoder, no migration.
        let row = stored
            .get(&candidate.files.content_hash())
            .filter(|row| decode(row).is_some());
        let Some(row) = row else {
            identify.push_back(candidate);
            continue;
        };
        identified += 1;
        // A verdict a person's own run wrote never paid for the tracklist, so
        // without this it would sit at "cannot verify" forever — skipped as
        // answered, and never promoted to Ready by the one lookup that would do
        // it.
        if decode(row).is_some_and(|verdict| owes_source_tracks(&verdict)) {
            top_up.push(row.clone());
        }
    }
    Plan {
        identify,
        top_up,
        identified,
        total,
    }
}

fn decode(row: &DbImportCandidateState) -> Option<TerminalVerdict> {
    match serde_json::from_str::<TerminalVerdict>(&row.verdict) {
        Ok(verdict) => Some(verdict),
        Err(e) => {
            debug!(
                "sweep: stored verdict for {} no longer decodes ({e}); re-identifying",
                row.content_hash
            );
            None
        }
    }
}

/// Whether a stored verdict is still waiting on the paid tracklist lookup: a
/// single match nobody has asked the source about. `Some(SourceTracks::Nothing)`
/// is an answer — the source has no tracklist — so it does not qualify, which is
/// what stops the top-up re-buying the same empty answer every launch.
fn owes_source_tracks(verdict: &TerminalVerdict) -> bool {
    single_match(verdict).is_some_and(|only| only.source_tracks.is_none())
}

/// The one match of a single-match `Found`, or `None`. Several matches or a
/// conflict go to Needs you whatever a tracklist would say, so paying for one
/// buys a classification that cannot change.
fn single_match(verdict: &TerminalVerdict) -> Option<&MetadataResult> {
    let TerminalVerdict::Found { matches, .. } = verdict else {
        return None;
    };
    match matches.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

/// Buy the tracklist for one stored row and write it back. Everything else about
/// the row — the verdict's identity, the probed total, the folder path — is
/// unchanged, so a failure leaves the row exactly as it was and the next pass
/// tries again.
async fn top_up_row(
    context: &SweepContext,
    row: DbImportCandidateState,
    token: &CancellationToken,
) {
    let Some(mut verdict) = decode(&row) else {
        return;
    };
    if !fill_source_tracks(context, &mut verdict, token).await {
        return;
    }
    save(
        context,
        token,
        &row.content_hash,
        &row.folder_path,
        &verdict,
        row.probed_total_duration_ms,
    )
    .await;
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

    if !fill_source_tracks(context, &mut verdict, token).await {
        return false;
    }

    save(
        context,
        token,
        &entry.candidate.files.content_hash(),
        &entry.candidate.path.to_string_lossy(),
        &verdict,
        entry.probed_total_duration_ms as i64,
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
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: i64,
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
    let row = NewImportCandidateState {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict,
        probed_total_duration_ms,
    };
    if let Err(e) = context
        .library_manager
        .save_import_candidate_state(&row)
        .await
    {
        warn!(
            "sweep: could not store the verdict for {} ({e}); it is retried next pass",
            row.folder_path
        );
        return false;
    }
    true
}

/// Fill in the source's tracklist for a single match that arrived without one —
/// the roadmap's one paid lookup. Returns whether the verdict is still storable.
///
/// A disc-ID match already carries its tracklist and costs nothing here.
///
/// This is background-only by construction: its two callers are the sweep's own
/// pass and its top-up, both of which run after the candidate's identification
/// has already settled, so nothing a person opened ever waits on it.
async fn fill_source_tracks(
    context: &SweepContext,
    verdict: &mut TerminalVerdict,
    token: &CancellationToken,
) -> bool {
    if !owes_source_tracks(verdict) {
        return true;
    }
    let TerminalVerdict::Found { matches, .. } = verdict else {
        return true;
    };
    let [only_match] = matches.as_mut_slice() else {
        return true;
    };

    let discogs = match context.library_manager.discogs_client() {
        Ok(client) => client,
        Err(e) => {
            debug!("sweep: no Discogs client for the paid lookup: {e}");
            None
        }
    };
    let lookup = fetch_source_tracks(only_match, discogs.as_ref(), CallPriority::Background);
    let fetched = tokio::select! {
        biased;
        // Shutdown mid-lookup is a transport failure by another name: nothing
        // was learned, so nothing is written and the next launch asks again.
        _ = token.cancelled() => return false,
        fetched = lookup => fetched,
    };
    match fetched {
        Ok(source_tracks) => {
            // `SourceTracks::Nothing` is an answer — this release states no
            // tracklist — so the verdict stores with the match unverifiable, and
            // the Ready rule lands it in Needs you rather than admitting it.
            // Storing the answer is what stops the top-up asking again.
            only_match.source_tracks = Some(source_tracks);
            true
        }
        Err(failure) => {
            debug!(
                "sweep: the paid lookup for {} never answered ({failure:?}); writing no row",
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
    let mut entry = InFlight {
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
                let Ok(verdict) = TerminalVerdict::try_from(state) else {
                    // Terminal but shaped by a failed lookup. Keep watching: a
                    // re-run from the toolbar may still answer it.
                    continue;
                };
                // No paid lookup on this path, deliberately. The row lands with
                // `source_tracks` unset when the match came from a search, and
                // the next sweep tops it up in the background.
                if save(
                    context,
                    token,
                    &entry.candidate.files.content_hash(),
                    &entry.candidate.path.to_string_lossy(),
                    &verdict,
                    entry.probed_total_duration_ms as i64,
                )
                .await
                {
                    return;
                }
            }
            Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key }))
                if key == candidate_key =>
            {
                return;
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("sweep: selection recorder for {candidate_key} lagged by {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// The scanned folder candidate behind a key, or `None` when the key names
/// something else (a library release being re-identified) or nothing.
fn folder_candidate(context: &SweepContext, key: &str) -> Option<FolderCandidate> {
    match context.import.get_candidate(key)? {
        ImportCandidateSnapshot::Folder { candidate, .. } => Some(candidate),
        ImportCandidateSnapshot::Invalid(_) | ImportCandidateSnapshot::Runtime { .. } => None,
    }
}

/// The candidates the sweep is responsible for: New ones only.
///
/// Added candidates are already in the library and Skipped ones are a decision
/// the user has made, so identifying either spends the rate limit on work
/// nobody asked for. Whether Skipped should eventually be swept is an open
/// question in the roadmap; until it is answered this is the conservative half.
fn new_candidates(context: &SweepContext) -> Vec<FolderCandidate> {
    context
        .import
        .get_import_candidates()
        .folder_candidates
        .into_iter()
        .map(|snapshot| snapshot.candidate)
        .filter(|candidate| !candidate.skipped && !candidate.is_added)
        .collect()
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
