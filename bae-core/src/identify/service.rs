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
use crate::signals::{ExtractionWatch, Failure, InternalFailure, SignalsSnapshot};
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

/// Pair one provider's results with live library status: the provider's
/// answer, or how bae broke on its way there or back — a request it could not
/// make, an answer it could not read, the library check, which is a read of
/// bae's own store.
async fn annotate_lookup(
    lookup: SourceLookup,
    library_manager: &LibraryManager,
) -> Result<LookupOutcome, InternalFailure> {
    let results = match lookup {
        Ok(results) => results,
        Err(Failure::Lookup(failure)) => return Ok(Err(failure)),
        Err(Failure::Internal(failure)) => return Err(failure),
    };
    if results.is_empty() {
        return Ok(Ok(Vec::new()));
    }
    annotate_with_library_status(results, library_manager)
        .await
        .map(Ok)
        .map_err(|detail| {
            InternalFailure::logged("checking the library for the releases found", detail)
        })
}

/// Send what a lookup came to: `answered` with the provider's answer, or the
/// run's end where bae broke.
fn emit_answer(
    tx: &mpsc::UnboundedSender<IdentifyEvent>,
    answer: Result<LookupOutcome, InternalFailure>,
    answered: impl FnOnce(LookupOutcome) -> IdentifyEvent,
) {
    match answer {
        Ok(outcome) => emit_step(tx, answered(outcome)),
        Err(failure) => emit_step(tx, IdentifyEvent::Broke { failure }),
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
    event_tx: ImportEventBus,
    /// The record `event_tx` keeps each candidate in, which holds the
    /// candidate's run in flight.
    candidates: CandidateRuntime,
    /// Source of [`IdentifyRunId`]s: every run this service starts is told
    /// apart from every other, including earlier runs of the same candidate.
    next_run: std::sync::atomic::AtomicU64,
    /// Every driver task, and every write of what a run keeps, for a test to
    /// wait until they have all returned.
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
    let driver = Driver {
        inner,
        run,
        key,
        priority,
        event_tx,
        token,
    };

    let mut state = IdentifyState::Idle;
    // The run's first step is its start, ahead of anything the extraction has
    // already said: a snapshot taken in `Idle` leaves the run there, which is
    // what a cancelled run looks like.
    let mut start = Some(IdentifyEvent::Started {
        providers: run_providers(&driver.inner.library_manager),
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
                _ = driver.token.cancelled() => IdentifyEvent::Cancelled,
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

        state = driver.advance(state, event);

        // The run is over the moment the reducer stops moving: a terminal state
        // is its answer, `Idle` is its cancellation. The driver deregisters and
        // returns either way — a run reads its inputs once, at its start, so
        // anything a person asks for afterwards is a new run with inputs of its
        // own rather than a message to this one.
        if state.is_terminal() || matches!(state, IdentifyState::Idle) {
            driver
                .inner
                .candidates
                .release_work(CandidateWork::Identify, &driver.key, generation);
            return;
        }
    }
}

/// What one driver holds for its whole run.
struct Driver {
    inner: Arc<IdentifyServiceInner>,
    run: IdentifyRunId,
    key: String,
    priority: CallPriority,
    /// Where the effects' answers come back to the driver loop.
    event_tx: mpsc::UnboundedSender<IdentifyEvent>,
    token: CancellationToken,
}

impl Driver {
    /// Feed `event` to the reducer, send the state it lands on, and dispatch
    /// the effects it asked for. A terminal state's effects go out as well:
    /// what a run keeps beyond itself is asked for in the step that ends it.
    fn advance(&self, state: IdentifyState, event: IdentifyEvent) -> IdentifyState {
        let (state, effects) = step(state, event);

        // Every state `step` returns is sent, including one identical to the
        // last (a stale response the reducer's `for_barcode` guard dropped).
        // The signals toolbar is a projection of the state, so a consumer
        // that draws the badge row derives it from this same value.
        self.inner.event_tx.send(ImportEvent::IdentifyStateChanged {
            candidate_key: self.key.clone(),
            run: self.run,
            state: state.clone(),
            priority: self.priority,
        });

        for effect in effects {
            dispatch_effect(
                self.inner.clone(),
                effect,
                self.priority,
                self.event_tx.clone(),
                self.token.clone(),
            );
        }
        state
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
                let event = match lookup_and_resolve(&disc_id, &library_manager, priority).await {
                    Ok(results) => IdentifyEvent::DiscidLookupCompleted { results },
                    Err(Failure::Lookup(failure)) => IdentifyEvent::DiscidLookupFailed { failure },
                    Err(Failure::Internal(failure)) => IdentifyEvent::Broke { failure },
                };
                emit_step(&event_tx, event);
            });
        }

        Effect::LookupIsrcs { isrcs } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let lookup = library_manager
                    .lookup_musicbrainz_isrcs(&isrcs, priority)
                    .await;
                let answer = annotate_lookup(lookup, &library_manager).await;
                if let Ok(Err(failure)) = &answer {
                    debug!("ISRC lookup failed for {isrcs:?}: {failure:?}");
                }
                emit_answer(&event_tx, answer, |outcome| {
                    IdentifyEvent::IsrcLookupAnswered { outcome }
                });
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
                let answer = annotate_lookup(lookup, &library_manager).await;
                if let Ok(Err(failure)) = &answer {
                    debug!(
                        "{} barcode lookup failed for {barcode}: {failure:?}",
                        source.as_str()
                    );
                }
                emit_answer(&event_tx, answer, |outcome| {
                    IdentifyEvent::BarcodeLookupAnswered {
                        source,
                        for_barcode: barcode,
                        outcome,
                    }
                });
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
                let answer = annotate_lookup(lookup, &library_manager).await;
                if let Ok(Err(failure)) = &answer {
                    debug!(
                        "{} title search failed for {}: {failure:?}",
                        source.as_str(),
                        query.album
                    );
                }
                emit_answer(&event_tx, answer, |outcome| IdentifyEvent::SearchAnswered {
                    source,
                    outcome,
                });
            });
        }

        // Each record is fetched through the one place a pick reads it from,
        // so picking an offered row later asks for nothing again. A twin is
        // fetched the same way: the MusicBrainz document that names it was
        // fetched with the twin's own documents, so the providers' response
        // caches answer it. It is checked against the library the way a
        // lookup's answers are.
        Effect::ReadReleases {
            releases,
            twins,
            track_lengths_ms,
        } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let mut read = Vec::with_capacity(releases.len() + twins.len());
                for release in releases {
                    let Some(stored) =
                        read_release(&library_manager, &release, priority, &event_tx).await
                    else {
                        return;
                    };
                    let document = stored.map(|stored| {
                        crate::identify::documents::ReleaseDocument::of(&stored, &track_lengths_ms)
                    });
                    read.push(crate::identify::documents::ReleaseReading { release, document });
                }
                let mut stored_twins = Vec::with_capacity(twins.len());
                for twin in twins {
                    let Some(stored) =
                        read_release(&library_manager, &twin.release, priority, &event_tx).await
                    else {
                        return;
                    };
                    let document = match stored {
                        Ok(stored) => {
                            let document = crate::identify::documents::ReleaseDocument::of(
                                &stored,
                                &track_lengths_ms,
                            );
                            stored_twins.push((stored, twin.named_by));
                            Ok(document)
                        }
                        Err(failure) => Err(failure),
                    };
                    read.push(crate::identify::documents::ReleaseReading {
                        release: twin.release,
                        document,
                    });
                }
                let checks: Vec<crate::db::LibraryCheck> = stored_twins
                    .iter()
                    .map(|(stored, _)| stored.library_check())
                    .collect();
                let statuses = if checks.is_empty() {
                    Vec::new()
                } else {
                    match library_manager.check_releases_in_library(&checks).await {
                        Ok(statuses) => statuses,
                        Err(error) => {
                            emit_step(
                                &event_tx,
                                IdentifyEvent::Broke {
                                    failure: crate::signals::InternalFailure::logged(
                                        "checking the library for the twins a run read",
                                        error.to_string(),
                                    ),
                                },
                            );
                            return;
                        }
                    }
                };
                let twins = stored_twins
                    .into_iter()
                    .zip(statuses)
                    .map(
                        |((stored, named_by), status)| crate::identify::documents::Twin {
                            result: crate::import::search::MetadataResult::of_release(&stored),
                            named_by,
                            status,
                        },
                    )
                    .collect();
                emit_step(&event_tx, IdentifyEvent::ReleasesRead { read, twins });
            });
        }

        // What was read stays true whether or not the run stays current, so
        // cancelling the run does not stop it being kept.
        Effect::KeepAlbumLinks { kept } => {
            let library_manager = inner.library_manager.clone();
            let keep = async move {
                library_manager.keep_album_links(kept).await;
            };
            #[cfg(test)]
            let keep = inner.driver_tasks.track_future(keep);
            runtime.spawn(keep);
        }

        Effect::LookupCatalog { source, catalog } => {
            let library_manager = inner.library_manager.clone();
            spawn_until_cancelled(&runtime, &token, async move {
                let query = SearchQuery::CatalogNumber {
                    catalog_number: catalog.clone(),
                };
                let lookup = search_source(&library_manager, source, &query, priority).await;
                let answer = annotate_lookup(lookup, &library_manager).await;
                if let Ok(Err(failure)) = &answer {
                    debug!(
                        "{} catalog lookup failed for {catalog}: {failure:?}",
                        source.as_str()
                    );
                }
                emit_answer(&event_tx, answer, |outcome| {
                    IdentifyEvent::CatalogLookupAnswered {
                        source,
                        for_catalog: catalog,
                        outcome,
                    }
                });
            });
        }
    }
}

/// `release` from storage or fetched and stored now, or why its catalog could
/// not give it. `None` when bae broke reading it: the run has then been told,
/// and ends.
async fn read_release(
    library_manager: &LibraryManager,
    release: &crate::import::MetadataRef,
    priority: CallPriority,
    event_tx: &mpsc::UnboundedSender<IdentifyEvent>,
) -> Option<Result<crate::import::source_release::SourceRelease, crate::signals::LookupFailure>> {
    let error =
        match crate::import::service::prepare_release(library_manager, release, priority).await {
            Ok(stored) => return Some(Ok(stored)),
            Err(error) => error,
        };
    match crate::import::search::failure_of(
        &error,
        &format!(
            "reading {} release {}",
            release.catalog.as_str(),
            release.key
        ),
    ) {
        Failure::Lookup(failure) => {
            debug!(
                "{} release {} could not be read in full: {error}",
                release.catalog.as_str(),
                release.key
            );
            Some(Err(failure))
        }
        Failure::Internal(failure) => {
            emit_step(event_tx, IdentifyEvent::Broke { failure });
            None
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
