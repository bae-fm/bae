//! Stateful identify driver. Wraps the pure reducer with I/O, runtime, and
//! per-candidate cancellation. One `IdentifyServiceHandle` per app; each
//! candidate runs in its own spawned driver task.

use super::annotate_lookups;
use super::barcode::lookup_barcode;
use super::catalog::lookup_catalog;
use super::discid::lookup_and_resolve;
use super::state::{step, Effect, IdentifyEvent, IdentifyState, SignalToggle};
use crate::import::search::SourceFailure;
use crate::import::ImportEvent;
use crate::library::LibraryManager;
use crate::util::rate_limiter::CallPriority;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Forward an event back to the driver loop. Fire-and-forget: the driver owns the
/// only receiver and handles events serially. A closed channel means the driver
/// already exited (cancelled, or an effect raced past it), so the event is
/// dropped — warned, because it means a result went nowhere.
fn emit_step(tx: &mpsc::UnboundedSender<IdentifyEvent>, event: IdentifyEvent) {
    if let Err(err) = tx.send(event) {
        warn!("identify step channel closed; dropped {:?}", err.0);
    }
}

/// Broadcast an identify state change on the import bus. The bus lives as long as
/// the app, so having no subscribers is odd enough to warn about.
fn broadcast_state_change(tx: &broadcast::Sender<ImportEvent>, event: ImportEvent) {
    if let Err(err) = tx.send(event) {
        warn!("identify state-change broadcast had no subscribers: {err}");
    }
}

fn barcode_lookup_failed(barcode: String, failures: Vec<SourceFailure>) -> IdentifyEvent {
    debug!("Barcode lookup failed for {barcode}: {failures:?}");
    IdentifyEvent::BarcodeLookupFailed {
        for_barcode: barcode,
        failures,
    }
}

fn catalog_lookup_failed(catalog: String, failures: Vec<SourceFailure>) -> IdentifyEvent {
    debug!("Catalog lookup failed for {catalog}: {failures:?}");
    IdentifyEvent::CatalogLookupFailed {
        for_catalog: catalog,
        failures,
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
    drivers: Mutex<HashMap<String, CandidateDriver>>,
    /// Source of [`IdentifyRunId`]s: every run this service starts is told
    /// apart from every other, including earlier runs of the same candidate.
    next_run: std::sync::atomic::AtomicU64,
}

/// One identify run of one candidate. A candidate is identified once at a
/// time, but a settled run's driver stays alive for the toolbar and still
/// broadcasts its state, so a consumer waiting on a later run of the same
/// candidate tells the two apart by this id rather than by the candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentifyRunId(u64);

impl IdentifyRunId {
    /// A run id for an event a test forges onto the bus; nothing a service
    /// allocates collides with it.
    #[cfg(test)]
    pub(crate) fn for_test(run: u64) -> Self {
        Self(run)
    }
}

struct CandidateDriver {
    token: CancellationToken,
    /// Into the running driver's event channel — where `toggle_signal` and
    /// `rerun` push the bridge's events.
    inbox: mpsc::UnboundedSender<IdentifyEvent>,
}

impl IdentifyServiceHandle {
    pub fn new(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        event_tx: broadcast::Sender<ImportEvent>,
    ) -> IdentifyServiceHandle {
        let inner = Arc::new(IdentifyServiceInner {
            library_manager,
            runtime_handle,
            event_tx,
            drivers: Mutex::new(HashMap::new()),
            next_run: std::sync::atomic::AtomicU64::new(1),
        });
        let mut removal_rx = inner.event_tx.subscribe();
        let removal_inner = inner.clone();
        inner.runtime_handle.spawn(async move {
            loop {
                let key = match removal_rx.recv().await {
                    Ok(ImportEvent::Scan(crate::import::ScanEvent::CandidateRemoved {
                        candidate_key,
                    })) => candidate_key,
                    Ok(ImportEvent::Scan(crate::import::ScanEvent::CandidateBindingChanged {
                        candidate,
                    })) => candidate.path.to_string_lossy().into_owned(),
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("identify: candidate-removal listener lagged by {n} import events");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if let Some(driver) = removal_inner.drivers.lock().unwrap().remove(&key) {
                    driver.token.cancel();
                }
            }
        });
        IdentifyServiceHandle { inner }
    }

    /// Allocate the id for a run about to be started. Separate from
    /// [`Self::start`] so a consumer can subscribe to the bus knowing which
    /// run it is waiting for before that run's first state is broadcast.
    pub fn new_run(&self) -> IdentifyRunId {
        IdentifyRunId(
            self.inner
                .next_run
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Start identifying `key` as `run`. Fire-and-forget; states come back as
    /// `ImportEvent::IdentifyStateChanged` carrying `run`.
    ///
    /// `priority` is the run's, not the call's: every provider lookup this run
    /// dispatches is admitted under it, so a candidate a person opened outranks
    /// one a sweep picked up.
    ///
    /// Identify consumes the `Signals` extraction streams, so the caller must
    /// start identify *before* extraction for `key`: the bus subscription is
    /// taken synchronously here, so no early snapshot can be missed.
    pub fn start(&self, run: IdentifyRunId, key: String, priority: CallPriority) {
        // A restart (the user re-selects after a scan refresh) supersedes the
        // prior run — a candidate is identified once at a time.
        self.cancel(&key);

        let token = CancellationToken::new();
        // Create the inbox up front so a `toggle_signal` / `rerun` arriving
        // before the driver task lands still finds somewhere to go.
        let (event_tx, event_rx) = mpsc::unbounded_channel::<IdentifyEvent>();
        self.inner.drivers.lock().unwrap().insert(
            key.clone(),
            CandidateDriver {
                token: token.clone(),
                inbox: event_tx.clone(),
            },
        );

        // Subscribe before returning, so the extraction service (started right
        // after) can't emit its first `SignalsUpdated` into a void.
        let bus_rx = self.inner.event_tx.subscribe();

        let inner = self.inner.clone();
        self.inner.runtime_handle.spawn(async move {
            run_driver(inner, run, key, priority, token, event_tx, event_rx, bus_rx).await;
        });
    }

    /// Whether a driver is registered for `key` — a run is in flight, or has
    /// settled and is still alive to receive a toggle or a re-run.
    ///
    /// The queue sweep asks before starting one, because
    /// [`IdentifyServiceHandle::start`] supersedes: sweeping a candidate the
    /// user has open would cancel their interactive run and restart it at
    /// background priority, which is the opposite of what the priority is for.
    pub fn is_running(&self, key: &str) -> bool {
        self.inner.drivers.lock().unwrap().contains_key(key)
    }

    /// Cancel an in-flight identify. Drops the driver task on the next
    /// await point.
    pub fn cancel(&self, key: &str) {
        let driver = self.inner.drivers.lock().unwrap().remove(key);
        if let Some(driver) = driver {
            driver.token.cancel();
        }
    }

    /// Include or exclude one of a candidate's signals from triangulation. The
    /// driver re-combines over the surviving signals and emits the result. A no-op
    /// when the candidate isn't running.
    pub fn toggle_signal(&self, key: &str, signal: SignalToggle) {
        self.push_event(
            key,
            IdentifyEvent::SignalToggled { signal },
            "toggle_signal",
        );
    }

    /// Re-run a candidate's lookups: the driver resets to `Triangulating` and
    /// re-dispatches from the retained signals, keeping the user's exclusions. A
    /// no-op when the candidate isn't running.
    pub fn rerun(&self, key: &str) {
        self.push_event(key, IdentifyEvent::ReRun, "rerun");
    }

    /// Push an event into a running driver's inbox. With no live driver the event
    /// is dropped, which is the right no-op for a stale UI action.
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

/// The driver loop for one candidate. Each iteration pops an event, feeds it to
/// the pure reducer, broadcasts the new state, and spawns the effects the reducer
/// asked for — whose results come back as further events. Ends on cancellation.
#[allow(clippy::too_many_arguments)]
async fn run_driver(
    inner: Arc<IdentifyServiceInner>,
    run: IdentifyRunId,
    key: String,
    priority: CallPriority,
    token: CancellationToken,
    event_tx: mpsc::UnboundedSender<IdentifyEvent>,
    mut event_rx: mpsc::UnboundedReceiver<IdentifyEvent>,
    mut bus_rx: broadcast::Receiver<ImportEvent>,
) {
    // Relay this candidate's `Signals` snapshots off the import bus into the
    // reducer, which turns the disc ID and barcodes into lookups and narrows by
    // catalog. Fire-and-forget: a missed snapshot delays a signal, never breaks
    // the pipeline.
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
                        priority: _,
                    }) if candidate_key == relay_key => {
                        if relay_event_tx
                            .send(IdentifyEvent::SignalsUpdated { signals })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Ok(_) => continue,
                    // Lagged: keep listening. A snapshot we fell behind on is
                    // superseded by the next one anyway.
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

        // Every state `step` returns is broadcast, including one identical to the
        // last (a stale response the reducer's `for_barcode` guard dropped). The
        // signals toolbar is a projection of the state, so a consumer that draws
        // the badge row derives it from this same value.
        broadcast_state_change(
            &inner.event_tx,
            ImportEvent::IdentifyStateChanged {
                candidate_key: key.clone(),
                run,
                state: state.clone(),
                priority,
            },
        );

        // The driver ends only on `Idle`, which is reached only via `Cancelled`.
        // `Found` / `Conflict` / `NotFoundAnywhere` / `ManualOnly` do NOT end it:
        // the user can still toggle a signal or re-run from the toolbar, and the
        // driver has to be alive to receive that.
        if matches!(state, IdentifyState::Idle) {
            remove_driver_if_current(&inner, &key, &event_tx);
            return;
        }

        for effect in effects {
            dispatch_effect(
                inner.clone(),
                effect,
                priority,
                event_tx.clone(),
                token.clone(),
            );
        }
    }
}

fn dispatch_effect(
    inner: Arc<IdentifyServiceInner>,
    effect: Effect,
    priority: CallPriority,
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
            runtime.spawn(async move {
                let outcome = lookup_and_resolve(&disc_id, &library_manager, priority).await;
                if token.is_cancelled() {
                    return;
                }
                match outcome {
                    Ok(results) => {
                        emit_step(
                            &event_tx,
                            IdentifyEvent::DiscidLookupCompleted {
                                results,
                                track_count,
                            },
                        );
                    }
                    Err(failure) => {
                        emit_step(
                            &event_tx,
                            IdentifyEvent::DiscidLookupFailed {
                                failure,
                                track_count,
                            },
                        );
                    }
                }
            });
        }

        // Both providers are asked at once and answer for themselves. Results
        // from either make the lookup a match, with the provider that failed
        // named alongside them; only a lookup nothing answered is a failure.
        Effect::LookupBarcode { barcode } => {
            let library_manager = inner.library_manager.clone();
            runtime.spawn(async move {
                let lookups = lookup_barcode(&barcode, &library_manager, priority).await;
                if token.is_cancelled() {
                    return;
                }
                let (results, failures) = annotate_lookups(lookups, &library_manager).await;
                let event = if !results.is_empty() {
                    IdentifyEvent::BarcodeLookupMatched {
                        for_barcode: barcode,
                        results,
                        failures,
                    }
                } else if failures.is_empty() {
                    IdentifyEvent::BarcodeLookupMissed {
                        for_barcode: barcode,
                    }
                } else {
                    barcode_lookup_failed(barcode, failures)
                };
                emit_step(&event_tx, event);
            });
        }

        Effect::LookupCatalog { catalog } => {
            let library_manager = inner.library_manager.clone();
            runtime.spawn(async move {
                let lookups = lookup_catalog(&catalog, &library_manager, priority).await;
                if token.is_cancelled() {
                    return;
                }
                let (results, failures) = annotate_lookups(lookups, &library_manager).await;
                let event = if results.is_empty() && !failures.is_empty() {
                    catalog_lookup_failed(catalog, failures)
                } else {
                    IdentifyEvent::CatalogLookupCompleted {
                        for_catalog: catalog,
                        results,
                        failures,
                    }
                };
                emit_step(&event_tx, event);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigHandle};
    use crate::db::Database;
    use crate::identify::IdentifyState;
    use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
    use std::time::Duration;

    async fn setup_inner() -> (Arc<IdentifyServiceInner>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().unwrap(),
            Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let library_dir = coven::StoreDir::new(temp_dir.path());
        let library_id = format!("test-{}", temp_dir.path().display());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir,
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        let manager = LibraryManager::new(
            database,
            Arc::new(ConfigHandle::new(config)),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(),
        );
        let (event_tx, _) = broadcast::channel(64);
        let inner = Arc::new(IdentifyServiceInner {
            library_manager: manager,
            runtime_handle: tokio::runtime::Handle::current(),
            event_tx,
            drivers: Mutex::new(HashMap::new()),
            next_run: std::sync::atomic::AtomicU64::new(1),
        });
        (inner, temp_dir)
    }

    /// Neither a disc-ID artifact nor a barcode source, so the reducer settles on
    /// `ManualOnly` with no network effect — the driver loop runs end to end
    /// without touching MB or Discogs.
    fn absent_signals() -> Signals {
        Signals {
            disc_id: DiscIdSignal::Absent { track_count: 7 },
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Settled {
                catalogs: vec![],
                free_text: vec![],
            },
            durations: crate::import::probe::SourceDurations::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn remove_driver_if_current_only_evicts_the_registered_inbox() {
        let (inner, _tmp) = setup_inner().await;
        let (tx_registered, _rx_registered) = mpsc::unbounded_channel::<IdentifyEvent>();
        let (tx_stale, _rx_stale) = mpsc::unbounded_channel::<IdentifyEvent>();

        inner.drivers.lock().unwrap().insert(
            "k".to_string(),
            CandidateDriver {
                token: CancellationToken::new(),
                inbox: tx_registered.clone(),
            },
        );

        // A superseding driver's inbox (different channel) must not evict the
        // one currently registered — that's the "if current" guard.
        remove_driver_if_current(&inner, "k", &tx_stale);
        assert!(inner.drivers.lock().unwrap().contains_key("k"));

        // Removing an unknown key is a no-op.
        remove_driver_if_current(&inner, "absent", &tx_registered);
        assert!(inner.drivers.lock().unwrap().contains_key("k"));

        // The registered inbox evicts it.
        remove_driver_if_current(&inner, "k", &tx_registered);
        assert!(!inner.drivers.lock().unwrap().contains_key("k"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn driver_settles_manual_only_and_cancel_deregisters() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.subscribe();

        handle.start(handle.new_run(), "k".to_string(), CallPriority::Interactive);

        // Feed the signals over the bus, as the extraction service would.
        inner
            .event_tx
            .send(ImportEvent::SignalsUpdated {
                candidate_key: "k".to_string(),
                signals: absent_signals(),
                priority: CallPriority::Interactive,
            })
            .unwrap();

        let mut saw_manual_only = false;
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_secs(2), bus_rx.recv()).await {
                Ok(Ok(ImportEvent::IdentifyStateChanged {
                    candidate_key,
                    state,
                    ..
                })) if candidate_key == "k" => {
                    if matches!(state, IdentifyState::ManualOnly { .. }) {
                        saw_manual_only = true;
                        break;
                    }
                }
                Ok(Ok(_)) => continue,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
                Err(_) => break,
            }
        }
        assert!(
            saw_manual_only,
            "driver should broadcast a terminal ManualOnly state"
        );

        // ManualOnly doesn't end the driver: it stays alive for a toggle or re-run.
        assert!(inner.drivers.lock().unwrap().contains_key("k"));

        handle.cancel("k");
        assert!(!inner.drivers.lock().unwrap().contains_key("k"));
    }
}
