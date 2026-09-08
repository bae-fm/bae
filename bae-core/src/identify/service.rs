//! Stateful identify driver. Wraps the pure reducer with I/O, runtime, and
//! per-candidate cancellation. One `IdentifyServiceHandle` per app; each
//! candidate runs in its own spawned driver task.

use super::annotate_with_library_status;
use super::code::{lookup_code, PrintedCode};
use super::discid::lookup_and_resolve;
use super::state::{step, Effect, IdentifyEvent, IdentifyState, LookupOutcome};
use crate::import::search::SourceLookup;
use crate::import::{ImportEvent, LookupChoices, MetadataSource};
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

/// The providers a run asks: every source this library has switched on and can
/// reach. A projection of `metadata_sources()`, read once when the run starts,
/// so a key added or a source switched on since joins the next run rather than
/// this one.
fn run_providers(library_manager: &LibraryManager) -> Vec<MetadataSource> {
    crate::import::asked_sources(&library_manager.metadata_sources())
}

/// Pair one provider's results with live library status. The library check
/// is bae's own work rather than the provider's, so a check that fails names
/// this provider's lookup as producing nothing usable and leaves every other
/// provider's answer alone.
async fn annotate_lookup(lookup: SourceLookup, library_manager: &LibraryManager) -> LookupOutcome {
    let results = lookup?;
    if results.is_empty() {
        return Ok(Vec::new());
    }
    annotate_with_library_status(results, library_manager)
        .await
        .map_err(|detail| crate::signals::LookupFailure::Diagnostic { detail })
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
/// time, but a run that ends and the run that replaces it broadcast on the
/// same bus under the same key, so a consumer waiting on one of them tells
/// the two apart by this id rather than by the candidate.
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
    /// Which run this driver is. A driver deregisters itself on its way out
    /// only while it is still the registered one, so a run a later `start`
    /// superseded cannot evict its successor.
    run: IdentifyRunId,
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
    /// A run with no source to ask does not start. Every source is switched
    /// off or missing its credential, so there is nothing to dispatch — and a
    /// run that dispatched nothing would settle as "found nothing anywhere",
    /// storing a verdict about a lookup that never happened. The candidate is
    /// left as it was; what to do about it is a settings question, and the
    /// surface reads the availability list to say so.
    pub fn start(
        &self,
        run: IdentifyRunId,
        key: String,
        priority: CallPriority,
        choices: LookupChoices,
    ) {
        // A restart (the user re-selects after a scan refresh, or changes what
        // the run asks about) supersedes the prior run — a candidate is
        // identified once at a time.
        self.cancel(&key);

        if run_providers(&self.inner.library_manager).is_empty() {
            debug!("identify: {key} has no source to ask; not starting a run");
            return;
        }

        let token = CancellationToken::new();
        self.inner.drivers.lock().unwrap().insert(
            key.clone(),
            CandidateDriver {
                token: token.clone(),
                run,
            },
        );

        // Subscribe before returning, so the extraction service (started right
        // after) can't emit its first `SignalsUpdated` into a void.
        let bus_rx = self.inner.event_tx.subscribe();

        let inner = self.inner.clone();
        self.inner.runtime_handle.spawn(async move {
            run_driver(inner, run, key, priority, choices, token, bus_rx).await;
        });
    }

    /// Whether a run is in flight for `key`. A run that reached its verdict
    /// and one that was cancelled are both gone: the driver deregisters
    /// itself the moment it stops working.
    ///
    /// The queue sweep asks before starting one, because
    /// [`IdentifyServiceHandle::start`] supersedes: sweeping a candidate the
    /// user has open would cancel their interactive run and restart it at
    /// background priority, which is the opposite of what the priority is for.
    pub fn is_running(&self, key: &str) -> bool {
        self.inner.drivers.lock().unwrap().contains_key(key)
    }

    /// Every key with a run in flight right now. What a change to the inputs
    /// every run reads — the library's provider list — has to act on: those
    /// runs answer the list as it was, and nothing else does.
    pub fn running_keys(&self) -> Vec<String> {
        self.inner.drivers.lock().unwrap().keys().cloned().collect()
    }

    /// Cancel an in-flight identify. Drops the driver task on the next
    /// await point.
    pub fn cancel(&self, key: &str) {
        let driver = self.inner.drivers.lock().unwrap().remove(key);
        if let Some(driver) = driver {
            driver.token.cancel();
        }
    }
}

fn remove_driver_if_current(inner: &IdentifyServiceInner, key: &str, run: IdentifyRunId) {
    let mut drivers = inner.drivers.lock().unwrap();
    if drivers.get(key).is_some_and(|driver| driver.run == run) {
        drivers.remove(key);
    }
}

/// The driver loop for one candidate. Each iteration pops an event, feeds it to
/// the pure reducer, broadcasts the new state, and spawns the effects the reducer
/// asked for — whose results come back as further events. Ends when the reducer
/// stops moving: on the run's terminal state, or on cancellation.
async fn run_driver(
    inner: Arc<IdentifyServiceInner>,
    run: IdentifyRunId,
    key: String,
    priority: CallPriority,
    choices: LookupChoices,
    token: CancellationToken,
    mut bus_rx: broadcast::Receiver<ImportEvent>,
) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<IdentifyEvent>();

    // Relay this candidate's `Signals` snapshots off the import bus into the
    // reducer, which turns the disc ID and barcodes into lookups and narrows by
    // catalog. Fire-and-forget: a missed snapshot delays a signal, never breaks
    // the pipeline.
    //
    // It holds a broadcast receiver every import event is cloned into, so it
    // stops the moment the loop it feeds does — on its own closed channel as
    // well as on the token, because a run that reached its verdict ends
    // without one.
    let relay_token = token.clone();
    let relay_event_tx = event_tx.clone();
    let relay_key = key.clone();
    inner.runtime_handle.spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = relay_token.cancelled() => return,
                _ = relay_event_tx.closed() => return,
                msg = bus_rx.recv() => match msg {
                    Ok(ImportEvent::SignalsUpdated {
                        candidate_key,
                        signals,
                        artwork,
                        priority: _,
                    }) if candidate_key == relay_key => {
                        if relay_event_tx
                            .send(IdentifyEvent::SignalsUpdated { signals, artwork })
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

    emit_step(
        &event_tx,
        IdentifyEvent::Started {
            providers: run_providers(&inner.library_manager),
            choices,
        },
    );

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

        // The run is over the moment the reducer stops moving: a terminal state
        // is its answer, `Idle` is its cancellation. The driver deregisters and
        // returns either way — a run reads its inputs once, at its start, so
        // anything a person asks for afterwards is a new run with inputs of its
        // own rather than a message to this one.
        if state.is_terminal() || matches!(state, IdentifyState::Idle) {
            remove_driver_if_current(&inner, &key, run);
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

        // Each provider is asked on its own task and answers for itself, so
        // one landing never waits on the other.
        Effect::LookupBarcode { source, barcode } => {
            let library_manager = inner.library_manager.clone();
            runtime.spawn(async move {
                let lookup = lookup_code(
                    source,
                    PrintedCode::Barcode,
                    &barcode,
                    &library_manager,
                    priority,
                )
                .await;
                if token.is_cancelled() {
                    return;
                }
                let outcome = annotate_lookup(lookup, &library_manager).await;
                if let Err(failure) = &outcome {
                    debug!(
                        "{} barcode lookup failed for {barcode}: {failure:?}",
                        source.as_str()
                    );
                }
                emit_step(
                    &event_tx,
                    IdentifyEvent::BarcodeLookupAnswered {
                        source,
                        for_barcode: barcode,
                        outcome,
                    },
                );
            });
        }

        Effect::LookupCatalog { source, catalog } => {
            let library_manager = inner.library_manager.clone();
            runtime.spawn(async move {
                let lookup = lookup_code(
                    source,
                    PrintedCode::CatalogNumber,
                    &catalog,
                    &library_manager,
                    priority,
                )
                .await;
                if token.is_cancelled() {
                    return;
                }
                let outcome = annotate_lookup(lookup, &library_manager).await;
                if let Err(failure) = &outcome {
                    debug!(
                        "{} catalog lookup failed for {catalog}: {failure:?}",
                        source.as_str()
                    );
                }
                emit_step(
                    &event_tx,
                    IdentifyEvent::CatalogLookupAnswered {
                        source,
                        for_catalog: catalog,
                        outcome,
                    },
                );
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
    async fn remove_driver_if_current_only_evicts_the_registered_run() {
        let (inner, _tmp) = setup_inner().await;
        let registered = IdentifyRunId(1);
        let superseded = IdentifyRunId(2);

        inner.drivers.lock().unwrap().insert(
            "k".to_string(),
            CandidateDriver {
                token: CancellationToken::new(),
                run: registered,
            },
        );

        // A run a later `start` superseded must not evict the one currently
        // registered — that's the "if current" guard.
        remove_driver_if_current(&inner, "k", superseded);
        assert!(inner.drivers.lock().unwrap().contains_key("k"));

        // Removing an unknown key is a no-op.
        remove_driver_if_current(&inner, "absent", registered);
        assert!(inner.drivers.lock().unwrap().contains_key("k"));

        // The registered run evicts it.
        remove_driver_if_current(&inner, "k", registered);
        assert!(!inner.drivers.lock().unwrap().contains_key("k"));
    }

    /// Wait for `k`'s driver to leave the registry, so a test asserts against
    /// the state a returned driver left rather than racing it.
    async fn await_deregistered(inner: &Arc<IdentifyServiceInner>) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while inner.drivers.lock().unwrap().contains_key("k") {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the driver deregisters itself");
    }

    /// Read `k`'s broadcast states until `wanted` answers one, or the wait
    /// runs out.
    async fn await_state<T>(
        bus_rx: &mut broadcast::Receiver<ImportEvent>,
        wanted: impl Fn(&IdentifyState) -> Option<T>,
    ) -> Option<T> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match bus_rx.recv().await {
                    Ok(ImportEvent::IdentifyStateChanged {
                        candidate_key,
                        state,
                        ..
                    }) if candidate_key == "k" => {
                        if let Some(found) = wanted(&state) {
                            return Some(found);
                        }
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    fn settled_signals_event() -> ImportEvent {
        ImportEvent::SignalsUpdated {
            candidate_key: "k".to_string(),
            signals: absent_signals(),
            artwork: crate::signals::ArtworkScan::Absent,
            priority: CallPriority::Interactive,
        }
    }

    /// The run's verdict is where the run ends. Nobody cancels it and no
    /// `Idle` follows: the driver deregisters itself on the terminal state,
    /// so "a driver is registered" means "work is in flight" and nothing else.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_driver_that_reaches_its_verdict_is_gone_without_a_cancel() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.subscribe();

        handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            LookupChoices::default(),
        );
        // Feed the signals over the bus, as the extraction service would.
        inner.event_tx.send(settled_signals_event()).unwrap();

        assert!(
            await_state(&mut bus_rx, |state| {
                matches!(state, IdentifyState::ManualOnly { .. }).then_some(())
            })
            .await
            .is_some(),
            "the run broadcasts its terminal ManualOnly state"
        );
        await_deregistered(&inner).await;
        assert!(
            !handle.is_running("k"),
            "the run that answered is not still in flight"
        );

        // And nothing follows it: a terminal state is not chased by the `Idle`
        // a teardown would broadcast.
        assert!(
            await_state(&mut bus_rx, |state| {
                matches!(state, IdentifyState::Idle).then_some(())
            })
            .await
            .is_none(),
            "the settled run broadcast nothing after its verdict"
        );
    }

    /// A cancel mid-run is the other way out: `Idle` says the run wrote
    /// nothing, and the driver is gone behind it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancel_mid_run_broadcasts_idle_and_deregisters() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.subscribe();

        handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            LookupChoices::default(),
        );
        assert!(handle.is_running("k"), "the run is in flight");
        assert_eq!(handle.running_keys(), vec!["k".to_string()]);

        // No signals: the run sits in `Triangulating` until the cancel lands.
        handle.cancel("k");

        assert!(
            await_state(&mut bus_rx, |state| {
                matches!(state, IdentifyState::Idle).then_some(())
            })
            .await
            .is_some(),
            "the cancelled run broadcasts Idle"
        );
        await_deregistered(&inner).await;
        assert!(handle.running_keys().is_empty());
    }
}
