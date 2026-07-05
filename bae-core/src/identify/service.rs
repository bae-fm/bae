//! Stateful identify driver. Wraps the pure reducer with I/O, runtime, and
//! per-candidate cancellation. One `IdentifyService` per app; each candidate
//! runs in its own spawned driver task.

use super::barcode::{annotate_with_library_status, lookup_barcode};
use super::discid::lookup_and_resolve;
use super::state::{step, Effect, ExcludedSignal, IdentifyEvent, IdentifyState};
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::ImportEvent;
use crate::library::LibraryManager;
use crate::signals::LookupFailure;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Forward an event back to the driver loop. Logs at warn-level if the loop
/// has already exited (receiver dropped) — an effect raced past the
/// terminal-state check or the driver was cancelled before this task finished,
/// so the event is discarded. Otherwise fire-and-forget; the driver owns the
/// only receiver and processes events serially.
fn emit_step(tx: &mpsc::UnboundedSender<IdentifyEvent>, event: IdentifyEvent) {
    if let Err(err) = tx.send(event) {
        warn!("identify step channel closed; dropped {:?}", err.0);
    }
}

/// Broadcast an identify state change on the import event bus. Logs at
/// warn-level if no subscribers remain — the bus is alive for the lifetime of
/// the app, so empty subscribers is unusual and worth a trace.
fn broadcast_state_change(tx: &broadcast::Sender<ImportEvent>, event: ImportEvent) {
    if let Err(err) = tx.send(event) {
        warn!("identify state-change broadcast had no subscribers: {err}");
    }
}

fn barcode_lookup_failed(barcode: String, message: String) -> IdentifyEvent {
    debug!("Barcode lookup failed for {barcode}: {message}");
    IdentifyEvent::BarcodeLookupFailed {
        for_barcode: barcode,
        failure: LookupFailure::Diagnostic { detail: message },
    }
}

/// Thread-safe handle to the running identify service.
#[derive(Clone)]
pub struct IdentifyServiceHandle {
    inner: Arc<IdentifyServiceInner>,
}

struct IdentifyServiceInner {
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    event_tx: broadcast::Sender<ImportEvent>,
    cover_art_archive: CoverArtArchiveClient,
    drivers: Mutex<HashMap<String, CandidateDriver>>,
}

struct CandidateDriver {
    token: CancellationToken,
    /// Sender into the running driver's internal event channel. External
    /// callers (the bridge) push events here via methods like `toggle_signal`
    /// and `rerun`.
    inbox: mpsc::UnboundedSender<IdentifyEvent>,
}

/// Builder / entry point for constructing and running the service.
pub struct IdentifyService;

impl IdentifyService {
    pub fn start(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        event_tx: broadcast::Sender<ImportEvent>,
        cover_art_archive: CoverArtArchiveClient,
    ) -> IdentifyServiceHandle {
        IdentifyServiceHandle {
            inner: Arc::new(IdentifyServiceInner {
                library_manager,
                runtime_handle,
                event_tx,
                cover_art_archive,
                drivers: Mutex::new(HashMap::new()),
            }),
        }
    }
}

impl IdentifyServiceHandle {
    /// Start identifying `key`. Fire-and-forget — events emit through the
    /// import event channel as `ImportEvent::IdentifyStateChanged`. Identify
    /// consumes the `Signals` the extraction service streams, so the caller
    /// must start identify *before* extraction for `key` — the bus
    /// subscription is taken synchronously here so no early snapshot is missed.
    pub fn start(&self, key: String) {
        // Cancel any in-flight identify for this key. Candidates are identified
        // at most once at a time; restarting (e.g. user re-selects after a scan
        // refresh) supersedes the prior run.
        self.cancel(&key);

        let token = CancellationToken::new();
        // Pre-create the driver's inbox so external pokes (`toggle_signal` /
        // `rerun`) race-free find it even before the driver task lands the
        // receiver.
        let (event_tx, event_rx) = mpsc::unbounded_channel::<IdentifyEvent>();
        self.inner.drivers.lock().unwrap().insert(
            key.clone(),
            CandidateDriver {
                token: token.clone(),
                inbox: event_tx.clone(),
            },
        );

        // Subscribe to the import bus synchronously, before this returns, so
        // the extraction service (started right after) can't emit its first
        // `SignalsUpdated` before the driver is listening.
        let bus_rx = self.inner.event_tx.subscribe();

        let inner = self.inner.clone();
        self.inner.runtime_handle.spawn(async move {
            run_driver(inner, key, token, event_tx, event_rx, bus_rx).await;
        });
    }

    /// Cancel an in-flight identify. Drops the driver task on the next
    /// await point.
    pub fn cancel(&self, key: &str) {
        let driver = self.inner.drivers.lock().unwrap().remove(key);
        if let Some(driver) = driver {
            driver.token.cancel();
        }
    }

    /// Toggle a signal in a candidate's toolbar — include or exclude it from
    /// triangulation. The driver flips the signal and re-combines over the
    /// surviving signals, emitting the resulting state. No-op when the
    /// candidate isn't running (the reducer drops unknown (state, event)
    /// pairs).
    pub fn toggle_signal(&self, key: &str, signal: ExcludedSignal) {
        self.push_event(
            key,
            IdentifyEvent::SignalToggled { signal },
            "toggle_signal",
        );
    }

    /// Re-run a candidate's lookups. The driver resets to `Triangulating` and
    /// re-dispatches the disc-ID / barcode lookups from the retained signals,
    /// preserving the user's exclusions. No-op when the candidate isn't
    /// running.
    pub fn rerun(&self, key: &str) {
        self.push_event(key, IdentifyEvent::ReRun, "rerun");
    }

    /// Push an event into a running driver's inbox. Logs (debug) when the
    /// candidate has no live driver — the event is dropped, which is the
    /// correct no-op for a stale UI action.
    fn push_event(&self, key: &str, event: IdentifyEvent, op: &str) {
        let inbox = self
            .inner
            .drivers
            .lock()
            .unwrap()
            .get(key)
            .map(|driver| driver.inbox.clone());
        if let Some(tx) = inbox {
            if tx.send(event).is_err() {
                debug!("{op}: driver for {key} already stopped");
            }
        }
    }
}

fn remove_driver_if_current(
    inner: &IdentifyServiceInner,
    key: &str,
    inbox: &mpsc::UnboundedSender<IdentifyEvent>,
) {
    let mut drivers = inner.drivers.lock().unwrap();
    if drivers
        .get(key)
        .is_some_and(|driver| driver.inbox.same_channel(inbox))
    {
        drivers.remove(key);
    }
}

/// Main driver loop for a single candidate.
///
/// Each iteration:
///   1. Pop an event from the internal channel (or a forwarded
///      catalog-candidates update from the import bus).
///   2. Feed it to the pure reducer, get (new state, effects).
///   3. Emit the state change over the import event channel.
///   4. Dispatch effects to spawned tasks; their results feed back as events.
///   5. Terminate on terminal states or cancellation.
async fn run_driver(
    inner: Arc<IdentifyServiceInner>,
    key: String,
    token: CancellationToken,
    event_tx: mpsc::UnboundedSender<IdentifyEvent>,
    mut event_rx: mpsc::UnboundedReceiver<IdentifyEvent>,
    mut bus_rx: broadcast::Receiver<ImportEvent>,
) {
    // Identify no longer scans or OCRs anything: the extraction service
    // produces the candidate's `Signals` (disc ID, barcodes, classified text)
    // and streams them on the import bus. The driver relays each snapshot in
    // as `SignalsUpdated`; the reducer turns the disc ID and barcodes into
    // network lookups and narrows by catalog candidates. Fire-and-forget: a
    // missed snapshot only delays a signal, never breaks the pipeline.
    let relay_token = token.clone();
    let relay_event_tx = event_tx.clone();
    let relay_key = key.clone();
    inner.runtime_handle.spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = relay_token.cancelled() => return,
                msg = bus_rx.recv() => match msg {
                    Ok(ImportEvent::SignalsUpdated {
                        candidate_key,
                        signals,
                    }) if candidate_key == relay_key => {
                        if relay_event_tx
                            .send(IdentifyEvent::SignalsUpdated { signals })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Ok(_) => continue,
                    // Lagged: skip and keep listening — old catalog snapshots
                    // are useless anyway. Closed: the bus is gone, no more
                    // updates coming.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    });

    emit_step(&event_tx, IdentifyEvent::Started);

    let mut state = IdentifyState::Idle;

    loop {
        let event = tokio::select! {
            biased;
            _ = token.cancelled() => IdentifyEvent::Cancelled,
            event = event_rx.recv() => match event {
                Some(e) => e,
                None => return,
            },
        };

        let (next_state, effects) = step(state.clone(), event);
        state = next_state;

        // Every state after `step` is broadcast; duplicate-identical states
        // can occur for stale responses (gated out by the reducer's
        // for_barcode guard) and are a small cost of the reducer-plus-effect
        // model. The toolbar projection rides on the same event so the UI
        // updates its badge row from the same transition.
        broadcast_state_change(
            &inner.event_tx,
            ImportEvent::IdentifyStateChanged {
                candidate_key: key.clone(),
                toolbar: state.toolbar(),
                state: state.clone(),
            },
        );

        // The driver ends only on `Idle` (reached via `Cancelled`).
        // `Found` / `Conflict` / `NotFoundAnywhere` / `ManualOnly` are *not*
        // terminal: the user can toggle a signal or re-run from the toolbar,
        // which re-derives the state. The driver stays alive to receive those.
        if matches!(state, IdentifyState::Idle) {
            // Idle only happens via Cancelled; Cancelled is terminal too.
            remove_driver_if_current(&inner, &key, &event_tx);
            return;
        }

        // Dispatch effects.
        for effect in effects {
            dispatch_effect(inner.clone(), effect, event_tx.clone(), token.clone());
        }
    }
}

fn dispatch_effect(
    inner: Arc<IdentifyServiceInner>,
    effect: Effect,
    event_tx: mpsc::UnboundedSender<IdentifyEvent>,
    token: CancellationToken,
) {
    let runtime = inner.runtime_handle.clone();
    match effect {
        Effect::LookupDiscid {
            disc_id,
            track_count,
        } => {
            let library_manager = inner.library_manager.clone();
            let cover_art_archive = inner.cover_art_archive.clone();
            runtime.spawn(async move {
                let outcome =
                    lookup_and_resolve(&cover_art_archive, &disc_id, &library_manager).await;
                if token.is_cancelled() {
                    return;
                }
                match outcome {
                    Ok((matches, library_statuses)) => {
                        let results: Vec<_> = matches.into_iter().zip(library_statuses).collect();
                        emit_step(
                            &event_tx,
                            IdentifyEvent::DiscidLookupCompleted {
                                results,
                                track_count,
                            },
                        );
                    }
                    Err(failure) => {
                        emit_step(&event_tx, IdentifyEvent::DiscidLookupFailed { failure });
                    }
                }
            });
        }

        Effect::LookupBarcode { barcode } => {
            let library_manager = inner.library_manager.clone();
            let cover_art_archive = inner.cover_art_archive.clone();
            runtime.spawn(async move {
                let discogs = match library_manager.discogs_client() {
                    Ok(c) => c,
                    Err(e) => {
                        emit_step(
                            &event_tx,
                            barcode_lookup_failed(
                                barcode,
                                format!("Failed to read Discogs key: {e}"),
                            ),
                        );
                        return;
                    }
                };
                let lookup = lookup_barcode(&cover_art_archive, &barcode, discogs.as_ref()).await;
                if token.is_cancelled() {
                    return;
                }
                let event = match lookup {
                    Err(message) => barcode_lookup_failed(barcode, message),
                    Ok(results) if results.is_empty() => IdentifyEvent::BarcodeLookupMissed {
                        for_barcode: barcode,
                    },
                    Ok(results) => {
                        match annotate_with_library_status(results, &library_manager).await {
                            Err(message) => barcode_lookup_failed(barcode, message),
                            Ok((matches, library_statuses)) => {
                                let results: Vec<_> =
                                    matches.into_iter().zip(library_statuses).collect();
                                IdentifyEvent::BarcodeLookupMatched {
                                    for_barcode: barcode,
                                    results,
                                }
                            }
                        }
                    }
                };
                emit_step(&event_tx, event);
            });
        }
    }
}
