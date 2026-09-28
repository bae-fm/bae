//! Stateful identify driver. Wraps the pure reducer with I/O, runtime, and
//! per-candidate cancellation. One `IdentifyServiceHandle` per app; each
//! candidate runs in its own spawned driver task.

use super::annotate_with_library_status;
use super::discid::lookup_and_resolve;
use super::state::{step, Effect, IdentifyEvent, IdentifyState, LookupOutcome, TitleSearch};
use crate::config::IdentificationSteps;
use crate::import::search::{search_source, SearchQuery, SourceLookup};
use crate::import::{
    CandidateRuntime, CandidateWork, Catalog, ImportEvent, ImportEventBus, LookupChoices,
};
use crate::library::LibraryManager;
use crate::signals::{ExtractionWatch, SignalsSnapshot};
use crate::util::rate_limiter::CallPriority;
use std::sync::Arc;
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

/// The providers a run asks: every source this library has switched on and can
/// reach. A projection of `metadata_sources()`, read once when the run starts,
/// so a key added or a source switched on since joins the next run rather than
/// this one.
fn run_providers(library_manager: &LibraryManager) -> Vec<Catalog> {
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
    event_tx: ImportEventBus,
    /// The record `event_tx` keeps each candidate in, which holds the
    /// candidate's run in flight.
    candidates: CandidateRuntime,
    /// Source of [`IdentifyRunId`]s: every run this service starts is told
    /// apart from every other, including earlier runs of the same candidate.
    next_run: std::sync::atomic::AtomicU64,
    /// Every driver task, for a test to wait until they have all returned.
    #[cfg(test)]
    driver_tasks: tokio_util::task::TaskTracker,
}

/// One identify run of one candidate. A candidate is identified once at a
/// time, but a run that ends and the run that replaces it report on the
/// same bus under the same key, so a consumer waiting on one of them tells
/// the two apart by this id rather than by the candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentifyRunId(u64);

impl IdentifyRunId {
    /// A run id for an event a test forges onto the bus, or an extraction a
    /// test starts without the run it normally feeds; nothing a service
    /// allocates collides with it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(run: u64) -> Self {
        Self(run)
    }
}

impl IdentifyServiceHandle {
    /// `candidates` is the runtime `event_tx` records into, which ends a
    /// removed or rebound candidate's run in the send that says so.
    pub fn new(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        event_tx: ImportEventBus,
        candidates: CandidateRuntime,
    ) -> IdentifyServiceHandle {
        let inner = Arc::new(IdentifyServiceInner {
            library_manager,
            runtime_handle,
            event_tx,
            candidates,
            next_run: std::sync::atomic::AtomicU64::new(1),
            #[cfg(test)]
            driver_tasks: tokio_util::task::TaskTracker::new(),
        });
        IdentifyServiceHandle { inner }
    }

    /// Allocate the id for a run about to be started. Separate from
    /// [`Self::start`] so a consumer can subscribe to the bus knowing which
    /// run it is waiting for before that run's first state is sent.
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
    /// one the automatic admission picked up.
    ///
    /// `steps` is which of its steps the run takes, read by the caller once
    /// for this run and the extraction feeding it alike.
    ///
    /// `title_search` is what the candidate's draft says about the release,
    /// which the run asks every provider once its identifiers have named
    /// nothing. `None` where the draft states no title.
    ///
    /// `snapshots` is the watch the extraction feeding this run handed out
    /// at its start. It holds the extraction's latest snapshot, so the driver
    /// reads what was last said whenever it looks — nothing is queued and
    /// nothing is missed — and reads only its own extraction.
    /// A run with no source to ask does not start, and this reports that it
    /// did not. Every source is switched off or missing its credential, so
    /// there is nothing to dispatch — and a run that dispatched nothing would
    /// settle as "found nothing anywhere", storing a verdict about a lookup
    /// that never happened. The candidate is left as it was; what to do about
    /// it is a settings question, and the surface reads the availability list
    /// to say so. Nothing will report on `run`, so a caller waiting on it
    /// stops waiting.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        run: IdentifyRunId,
        key: String,
        priority: CallPriority,
        steps: IdentificationSteps,
        choices: LookupChoices,
        title_search: Option<TitleSearch>,
        snapshots: ExtractionWatch,
    ) -> bool {
        // A restart (the user re-selects after a scan refresh, or changes what
        // the run asks about) supersedes the prior run — a candidate is
        // identified once at a time.
        self.cancel(&key);

        if run_providers(&self.inner.library_manager).is_empty() {
            debug!("identify: {key} has no source to ask; not starting a run");
            return false;
        }

        let inner = self.inner.clone();
        self.inner.candidates.start_work(
            CandidateWork::Identify,
            key.clone(),
            |token, generation| {
                let driver = async move {
                    run_driver(
                        inner,
                        run,
                        key,
                        generation,
                        priority,
                        steps,
                        choices,
                        title_search,
                        token,
                        snapshots,
                    )
                    .await;
                };
                #[cfg(test)]
                let driver = self.inner.driver_tasks.track_future(driver);
                self.inner.runtime_handle.spawn(driver);
            },
        );
        true
    }

    /// Whether a run is in flight for `key`. A run that reached its verdict
    /// and one that was cancelled are both gone: the driver deregisters
    /// itself the moment it stops working.
    ///
    /// Nothing in the app asks: the identification queue is the only thing
    /// that starts a candidate's run, and its own entry says what that run is
    /// doing. A test asks to check that from the outside.
    pub fn is_running(&self, key: &str) -> bool {
        self.inner
            .candidates
            .is_working(CandidateWork::Identify, key)
    }

    /// Every key with a run in flight right now. What a change to the inputs
    /// every run reads — the library's provider list — has to act on: those
    /// runs answer the list as it was, and nothing else does.
    pub fn running_keys(&self) -> Vec<String> {
        self.inner.candidates.working_keys(CandidateWork::Identify)
    }

    /// Cancel an in-flight identify. Drops the driver task on the next
    /// await point.
    pub fn cancel(&self, key: &str) {
        self.inner
            .candidates
            .cancel_work(CandidateWork::Identify, key);
    }
}

/// The driver loop for one candidate. Each iteration takes the extraction's
/// next snapshot or a lookup's completion, feeds it to the pure reducer,
/// sends the new state, and spawns the effects the reducer asked for —
/// whose results come back as further events. Ends when the reducer stops
/// moving: on the run's terminal state, or on cancellation.
///
/// The snapshots come off the watch the extraction handed out, which holds
/// only the latest: the reducer turns each into lookups and a catalog filter,
/// and every snapshot is the whole of what was read so far, so the latest is
/// the only one it needs.
#[allow(clippy::too_many_arguments)]
async fn run_driver(
    inner: Arc<IdentifyServiceInner>,
    run: IdentifyRunId,
    key: String,
    // Whether this is still the key's current run, so a run a later start
    // superseded cannot deregister its successor.
    generation: u64,
    priority: CallPriority,
    steps: IdentificationSteps,
    choices: LookupChoices,
    title_search: Option<TitleSearch>,
    token: CancellationToken,
    mut snapshots: ExtractionWatch,
) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<IdentifyEvent>();

    let mut state = IdentifyState::Idle;
    // The run's first step is its start, ahead of anything the extraction has
    // already said: a snapshot taken in `Idle` leaves the run there, which is
    // what a cancelled run looks like.
    let mut start = Some(IdentifyEvent::Started {
        providers: run_providers(&inner.library_manager),
        steps,
        choices,
        title_search,
    });
    // Whether the extraction is still going. Its sender goes with it, and
    // once that is gone its last snapshot has been read: there is nothing
    // further to wait on there, only the lookups it started.
    let mut extracting = true;

    loop {
        let event = match start.take() {
            Some(start) => start,
            None => tokio::select! {
                biased;
                _ = token.cancelled() => IdentifyEvent::Cancelled,
                changed = snapshots.changed(), if extracting => match changed {
                    Ok(()) => match snapshots.borrow_and_update().clone() {
                        Some(SignalsSnapshot {
                            signals,
                            audio,
                            artwork,
                        }) => IdentifyEvent::SignalsUpdated {
                            signals,
                            audio,
                            artwork,
                        },
                        None => continue,
                    },
                    Err(_) => {
                        extracting = false;
                        continue;
                    }
                },
                event = event_rx.recv() => match event {
                    Some(e) => e,
                    None => return,
                },
            },
        };

        let (next_state, effects) = step(state.clone(), event);
        state = next_state;

        // Every state `step` returns is sent, including one identical to the
        // last (a stale response the reducer's `for_barcode` guard dropped). The
        // signals toolbar is a projection of the state, so a consumer that draws
        // the badge row derives it from this same value.
        inner.event_tx.send(ImportEvent::IdentifyStateChanged {
            candidate_key: key.clone(),
            run,
            state: state.clone(),
            priority,
        });

        // The run is over the moment the reducer stops moving: a terminal state
        // is its answer, `Idle` is its cancellation. The driver deregisters and
        // returns either way — a run reads its inputs once, at its start, so
        // anything a person asks for afterwards is a new run with inputs of its
        // own rather than a message to this one.
        if state.is_terminal() || matches!(state, IdentifyState::Idle) {
            inner
                .candidates
                .release_work(CandidateWork::Identify, &key, generation);
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

/// Run one effect's work on the runtime until it ends or the run is cancelled.
///
/// A cancelled run's lookup is dropped where it stands — mid-request, or
/// between a provider's retries while it waits out a busy server — so it
/// neither keeps asking a provider nobody is waiting on nor takes the
/// rate-limit slots the next run's requests need.
fn spawn_until_cancelled(
    runtime: &tokio::runtime::Handle,
    token: &CancellationToken,
    work: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let token = token.clone();
    runtime.spawn(async move {
        token.run_until_cancelled(work).await;
    });
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
        Effect::LookupDiscid { disc_id } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let outcome = lookup_and_resolve(&disc_id, &library_manager, priority).await;
                match outcome {
                    Ok(results) => {
                        emit_step(&event_tx, IdentifyEvent::DiscidLookupCompleted { results });
                    }
                    Err(failure) => {
                        emit_step(&event_tx, IdentifyEvent::DiscidLookupFailed { failure });
                    }
                }
            });
        }

        Effect::LookupIsrcs { isrcs } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let lookup = library_manager
                    .lookup_musicbrainz_isrcs(&isrcs, priority)
                    .await;
                let outcome = annotate_lookup(lookup, &library_manager).await;
                if let Err(failure) = &outcome {
                    debug!("ISRC lookup failed for {isrcs:?}: {failure:?}");
                }
                emit_step(&event_tx, IdentifyEvent::IsrcLookupAnswered { outcome });
            });
        }

        // Each provider is asked on its own task and answers for itself, so
        // one landing never waits on the other.
        Effect::LookupBarcode { source, barcode } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let query = SearchQuery::Barcode {
                    barcode: barcode.clone(),
                };
                let lookup = search_source(&library_manager, source, &query, priority).await;
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

        // The one query every provider is asked, once the identifiers have
        // come back with nothing — the same query the Search section's General
        // tab sends.
        Effect::SearchTitle { source, query } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let search = SearchQuery::General {
                    artist: query.artist.clone(),
                    album: query.album.clone(),
                };
                let lookup = search_source(&library_manager, source, &search, priority).await;
                let outcome = annotate_lookup(lookup, &library_manager).await;
                if let Err(failure) = &outcome {
                    debug!(
                        "{} title search failed for {}: {failure:?}",
                        source.as_str(),
                        query.album
                    );
                }
                emit_step(&event_tx, IdentifyEvent::SearchAnswered { source, outcome });
            });
        }

        // Each record is fetched through the one place a pick reads it from,
        // so picking an offered row later asks for nothing again.
        Effect::ReadReleases {
            releases,
            track_lengths_ms,
        } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let mut read = Vec::with_capacity(releases.len());
                for release in releases {
                    let document = crate::import::service::prepare_release(
                        &library_manager,
                        &release,
                        priority,
                    )
                    .await
                    .map(|stored| {
                        crate::identify::documents::ReleaseDocument::of(&stored, &track_lengths_ms)
                    })
                    .map_err(|error| {
                        debug!(
                            "{} release {} could not be read in full: {error}",
                            release.catalog.as_str(),
                            release.key
                        );
                        crate::import::search::import_error_to_lookup_failure(&error)
                    });
                    read.push(crate::identify::documents::ReleaseReading { release, document });
                }
                emit_step(&event_tx, IdentifyEvent::ReleasesRead { read });
            });
        }

        // What was read stays true whether or not the run stays current, so
        // cancelling the run does not stop it being kept.
        Effect::KeepAlbumLinks { kept } => {
            let library_manager = inner.library_manager.clone();
            runtime.spawn(async move {
                library_manager.keep_album_links(kept).await;
            });
        }

        Effect::ReadAlbumLinks { to_read } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let read = library_manager.read_album_links(&to_read, priority).await;
                emit_step(&event_tx, IdentifyEvent::AlbumLinksRead { read });
            });
        }

        Effect::LookupCatalog { source, catalog } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let query = SearchQuery::CatalogNumber {
                    catalog_number: catalog.clone(),
                };
                let lookup = search_source(&library_manager, source, &query, priority).await;
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
    use tokio::sync::mpsc::UnboundedReceiver;

    async fn setup_inner() -> (Arc<IdentifyServiceInner>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
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
            crate::config::AppDir::under_home(temp_dir.path()),
            Arc::new(ConfigHandle::new(config)),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(
                crate::util::http::Http::for_test(),
            ),
            crate::providers::Providers::offline(),
        );
        let candidates = CandidateRuntime::default();
        let event_tx = ImportEventBus::new(candidates.clone());
        let handle = IdentifyServiceHandle::new(
            manager,
            tokio::runtime::Handle::current(),
            event_tx,
            candidates,
        );
        (handle.inner, temp_dir)
    }

    /// A removal sent while a run is in flight ends it by the time the send
    /// returns.
    #[tokio::test]
    async fn a_removed_candidates_run_ends_in_the_send_that_removes_it() {
        let (inner, _tmp) = setup_inner().await;
        let token =
            inner
                .candidates
                .start_work(CandidateWork::Identify, "k".to_string(), |token, _| token);

        inner.event_tx.send(ImportEvent::Scan(
            crate::import::ScanEvent::CandidateRemoved {
                candidate_key: "k".to_string(),
            },
        ));

        assert!(
            token.is_cancelled(),
            "the removed candidate's run is cancelled"
        );
        assert!(
            !inner.candidates.is_working(CandidateWork::Identify, "k"),
            "and no longer registered"
        );
    }

    /// Neither a disc-ID artifact nor a barcode source, so the reducer settles on
    /// `ManualOnly` with no network effect — the driver loop runs end to end
    /// without touching MB or Discogs.
    fn absent_signals() -> Signals {
        Signals {
            origin: crate::signals::AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Settled {
                catalogs: vec![],
                free_text: vec![],
            },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        }
    }

    /// Wait until every driver task has returned, so everything they report
    /// is already on the bus.
    async fn drivers_ended(inner: &Arc<IdentifyServiceInner>) {
        inner.driver_tasks.close();
        tokio::time::timeout(Duration::from_secs(30), inner.driver_tasks.wait())
            .await
            .expect("every driver returns");
    }

    /// Read `k`'s states until `wanted` answers one, or the wait runs out.
    async fn await_state<T>(
        events: &mut UnboundedReceiver<ImportEvent>,
        wanted: impl Fn(&IdentifyState) -> Option<T>,
    ) -> Option<T> {
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.recv().await {
                if let ImportEvent::IdentifyStateChanged {
                    candidate_key,
                    state,
                    ..
                } = event
                {
                    if candidate_key == "k" {
                        if let Some(found) = wanted(&state) {
                            return Some(found);
                        }
                    }
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    /// `k`'s states already reported.
    fn reported_states(events: &mut UnboundedReceiver<ImportEvent>) -> Vec<IdentifyState> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                ImportEvent::IdentifyStateChanged {
                    candidate_key,
                    state,
                    ..
                } if candidate_key == "k" => Some(state),
                _ => None,
            })
            .collect()
    }

    fn settled_snapshot() -> SignalsSnapshot {
        SignalsSnapshot {
            signals: absent_signals(),
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Absent,
        }
    }

    /// What extraction says while its artwork pass is still going: nothing a
    /// run can settle on.
    fn scanning_snapshot() -> SignalsSnapshot {
        SignalsSnapshot {
            signals: Signals {
                barcode: BarcodeSignal::Scanning { codes: vec![] },
                text: TextSignal::Scanning {
                    catalogs: vec![],
                    free_text: vec![],
                },
                ..absent_signals()
            },
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Reading {
                current: None,
                position: 1,
                total: 1,
            },
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
        let mut bus_rx = inner.event_tx.every_event();

        let (snapshots, watch) = tokio::sync::watch::channel(None);
        assert!(handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            IdentificationSteps::default(),
            LookupChoices::default(),
            None,
            watch,
        ));
        // Feed the signals over the watch, as the extraction service would.
        snapshots.send_replace(Some(settled_snapshot()));

        assert!(
            await_state(&mut bus_rx, |state| {
                matches!(state, IdentifyState::ManualOnly { .. }).then_some(())
            })
            .await
            .is_some(),
            "the run reports its terminal ManualOnly state"
        );
        drivers_ended(&inner).await;
        assert!(
            !handle.is_running("k"),
            "the run that answered is not still in flight"
        );

        // And nothing follows it: a terminal state is not chased by the `Idle`
        // a teardown would report.
        let after = reported_states(&mut bus_rx);
        assert!(
            after.is_empty(),
            "the settled run reported nothing after its verdict: {after:?}"
        );
    }

    /// The watch holds the latest snapshot rather than queueing them. Two
    /// land before the driver is even up — as they do when extraction is
    /// quick, or the runtime is busy — and the driver reads the settled one,
    /// which is the whole of what was read and the only one it needs.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_run_reads_the_latest_snapshot_however_many_landed_before_it_looked() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.every_event();

        let (snapshots, watch) = tokio::sync::watch::channel(None);
        snapshots.send_replace(Some(scanning_snapshot()));
        snapshots.send_replace(Some(settled_snapshot()));
        assert!(handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            IdentificationSteps::default(),
            LookupChoices::default(),
            None,
            watch,
        ));

        assert!(
            await_state(&mut bus_rx, |state| {
                matches!(state, IdentifyState::ManualOnly { .. }).then_some(())
            })
            .await
            .is_some(),
            "the run settled on the snapshot that was current when it looked"
        );
    }

    /// The extraction's watch closing is the extraction ending, not the run:
    /// the lookups it started still answer, and the run waits on them — or
    /// on its cancel.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_extraction_ending_does_not_end_the_run() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.every_event();

        let (snapshots, watch) = tokio::sync::watch::channel(None);
        assert!(handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            IdentificationSteps::default(),
            LookupChoices::default(),
            None,
            watch,
        ));
        snapshots.send_replace(Some(scanning_snapshot()));
        drop(snapshots);

        assert!(
            await_state(&mut bus_rx, |state| state.is_terminal().then_some(()))
                .await
                .is_none(),
            "no verdict comes of an extraction that said nothing settled"
        );
        assert!(handle.is_running("k"), "the run is still in flight");

        handle.cancel("k");
        drivers_ended(&inner).await;
        assert!(
            reported_states(&mut bus_rx)
                .iter()
                .any(|state| matches!(state, IdentifyState::Idle)),
            "and its cancel still lands"
        );
    }

    /// A cancel mid-run is the other way out: `Idle` says the run wrote
    /// nothing, and the driver is gone behind it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancel_mid_run_reports_idle_and_deregisters() {
        let (inner, _tmp) = setup_inner().await;
        let handle = IdentifyServiceHandle {
            inner: inner.clone(),
        };
        let mut bus_rx = inner.event_tx.every_event();

        let (_snapshots, watch) = tokio::sync::watch::channel(None);
        assert!(handle.start(
            handle.new_run(),
            "k".to_string(),
            CallPriority::Interactive,
            IdentificationSteps::default(),
            LookupChoices::default(),
            None,
            watch,
        ));
        assert!(handle.is_running("k"), "the run is in flight");
        assert_eq!(handle.running_keys(), vec!["k".to_string()]);

        // No signals: the run sits in `Triangulating` until the cancel lands.
        handle.cancel("k");

        drivers_ended(&inner).await;
        assert!(
            reported_states(&mut bus_rx)
                .iter()
                .any(|state| matches!(state, IdentifyState::Idle)),
            "the cancelled run reports Idle"
        );
        assert!(handle.running_keys().is_empty());
    }

    /// A cancelled run's lookup ends where it stands — here, waiting between
    /// retries on a provider that never answers — rather than running on.
    #[tokio::test]
    async fn a_cancelled_run_drops_the_lookup_it_was_waiting_on() {
        struct Dropped(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                let _ = self.0.take().map(|tx| tx.send(()));
            }
        }
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let token = CancellationToken::new();
        spawn_until_cancelled(&tokio::runtime::Handle::current(), &token, async move {
            let _guard = Dropped(Some(dropped_tx));
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
        });
        started_rx.await.expect("the lookup starts");
        token.cancel();
        tokio::time::timeout(Duration::from_secs(2), dropped_rx)
            .await
            .expect("the lookup is dropped once its run is cancelled")
            .expect("the guard reports its drop");
    }
}
