//! The list's live query, with what this process holds and no table does
//! folded into its request, delivered beside where the folder scans stand.
//!
//! Three such facts. Two come from
//! [`CandidateRuntime`](crate::import::CandidateRuntime): whether an import has
//! claimed the candidate, which places its row, and how far identification has
//! got, which the row shows. The third orders a row: where an imported
//! release's cloud upload stands, from the outbox. The subscription owns the
//! merges — it keeps the current request, applies each change that moves a
//! row, and hands the query the new request once the read under way has
//! answered. The bridge and the UIs never see any of it.
//!
//! The folder scans are a second live query the subscription reads beside the
//! list: a scan moves its found count with every folder it walks, and that
//! count moves no row, so it never reruns the list.

use super::{
    FolderScanProgress, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListView, UploadStanding,
};
use crate::import::triage::TriageRuntimeFacts;
use crate::import::{CandidateRuntimeChange, CandidateRuntimeSnapshot};
use crate::library::{LibraryPageWindows, OutboxSnapshot};
use crate::live_query::CancellableLiveQuery;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

#[derive(Debug, thiserror::Error)]
pub enum ImportListSubscriptionError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The request as it stands, and the query it reconfigures.
///
/// Two kinds of change reach the request, and they reach the query
/// differently. A person's — a view, a set of windows — replaces the request
/// at once: the read under way answers a question nobody is asking any more.
/// A merge's — runtime facts, upload standing — waits for the read under way
/// to answer, and goes in with the next one. Runtime facts move on every
/// state a batch of runs passes through, and a large queue takes longer to
/// read than that: a read restarted by each of them never answers, and the
/// rows show none of the batch until it is over.
struct StandingRequest {
    standing: Mutex<Standing>,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

/// The request, and where the query stands with it.
struct Standing {
    request: ImportListRequest,
    /// The revision of the last value the query delivered.
    answered: Option<u64>,
    /// The read the query owes, while it owes one. A merge's change waits for
    /// it.
    reading: Option<Reading>,
}

/// A request the query was handed and has not answered yet.
struct Reading {
    revision: u64,
    /// A merge has changed the request since it was handed over.
    held: bool,
}

impl Standing {
    /// Hand the query the whole request, and wait for the revision that
    /// answers it — unless the query already answered that request.
    fn hand_over(
        &mut self,
        query: &CancellableLiveQuery<ImportListRequest, ImportListProjection>,
    ) -> Result<u64, ImportListSubscriptionError> {
        let revision = query
            .set(self.request.clone())
            .map_err(|_| ImportListSubscriptionError::Cancelled)?;
        self.reading = self
            .answered
            .is_none_or(|answered| revision > answered)
            .then_some(Reading {
                revision,
                held: false,
            });
        Ok(revision)
    }
}

impl StandingRequest {
    /// A person's change: replace part of the request and hand the whole
    /// thing to the query now.
    ///
    /// Handed over under the lock, so two changes reach the query in the order
    /// they were made to the request.
    fn update(
        &self,
        change: impl FnOnce(&mut ImportListRequest),
    ) -> Result<u64, ImportListSubscriptionError> {
        let mut standing = self
            .standing
            .lock()
            .expect("import list request mutex poisoned");
        change(&mut standing.request);
        standing.hand_over(&self.query)
    }

    /// A merge's change: apply it, and hand the request over only when no read
    /// is waiting to answer. `change` reports whether it moved anything.
    fn merge(
        &self,
        change: impl FnOnce(&mut ImportListRequest) -> bool,
    ) -> Result<(), ImportListSubscriptionError> {
        let mut standing = self
            .standing
            .lock()
            .expect("import list request mutex poisoned");
        if !change(&mut standing.request) {
            return Ok(());
        }
        if let Some(reading) = &mut standing.reading {
            reading.held = true;
            return Ok(());
        }
        standing.hand_over(&self.query).map(|_| ())
    }

    /// The query delivered `revision`. A read that answers the revision it
    /// was waiting on hands over whatever the merges held back meanwhile.
    fn answered(&self, revision: u64) -> Result<(), ImportListSubscriptionError> {
        let mut standing = self
            .standing
            .lock()
            .expect("import list request mutex poisoned");
        standing.answered = Some(revision);
        let Some(reading) = standing
            .reading
            .take_if(|reading| revision >= reading.revision)
        else {
            return Ok(());
        };
        if reading.held {
            standing.hand_over(&self.query)?;
        }
        Ok(())
    }
}

pub struct ImportListSubscription {
    request: Arc<StandingRequest>,
    /// Where the folder scans stand. Taken on close, like the list's query.
    folder_scans: tokio::sync::Mutex<Option<coven::LiveQuery<FolderScanProgress>>>,
    /// The last value of each query, so a change to one delivers beside the
    /// other. A snapshot goes out once both have answered.
    delivered: tokio::sync::Mutex<Delivered>,
    merges: Mutex<Vec<tokio::task::AbortHandle>>,
}

#[derive(Default)]
struct Delivered {
    list: Option<AnsweredList>,
    folder_scans: Option<FolderScanProgress>,
    /// Whether a snapshot has gone out: until one has, the list's own cause
    /// names it, whichever query answered last.
    sent: bool,
}

/// The list's last value, the request revision it answered and why it was
/// read.
struct AnsweredList {
    projection: ImportListProjection,
    request_revision: u64,
    cause: coven::ReconfigurableLiveQueryCause,
}

impl Delivered {
    /// The snapshot due once both queries have answered. A change to the
    /// scans alone, after the first, is a database change beside the list's
    /// last read.
    fn snapshot(&mut self, scans_only: bool) -> Option<ImportListSnapshot> {
        let (Some(list), Some(folder_scans)) = (&self.list, &self.folder_scans) else {
            return None;
        };
        let cause = if scans_only && self.sent {
            coven::ReconfigurableLiveQueryCause::DatabaseChanged
        } else {
            list.cause
        };
        let snapshot = ImportListSnapshot {
            windows: list.projection.windows.clone(),
            total_count: list.projection.total_count,
            summary: list.projection.summary.clone(),
            folder_scans: folder_scans.clone(),
            request_revision: list.request_revision,
            cause,
        };
        self.sent = true;
        Some(snapshot)
    }
}

/// What one wait produced: a list event or a folder-scan value.
enum Arrival {
    List(coven::ReconfigurableLiveQueryEvent<ImportListRequest, ImportListProjection>),
    FolderScans(coven::CovenResult<FolderScanProgress>),
}

impl ImportListSubscription {
    /// Start the subscription and the two merges behind it.
    ///
    /// `initial` must already carry the runtime facts the caller read before it
    /// took `changes`, so no change lands between the two. `outbox` needs no
    /// such care: a watch channel always holds its current value, so the merge
    /// reads it once before it waits.
    pub(crate) fn start(
        query: coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>,
        folder_scans: coven::LiveQuery<FolderScanProgress>,
        initial: ImportListRequest,
        changes: broadcast::Receiver<CandidateRuntimeChange>,
        reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot> + Send + 'static,
        outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        // The query reads `initial` as revision 0 on its own.
        let request = Arc::new(StandingRequest {
            standing: Mutex::new(Standing {
                request: initial,
                answered: None,
                reading: Some(Reading {
                    revision: 0,
                    held: false,
                }),
            }),
            query: CancellableLiveQuery::new(query),
        });
        let merges = vec![
            runtime_handle
                .spawn(merge_runtime(request.clone(), changes, reread))
                .abort_handle(),
            runtime_handle
                .spawn(merge_outbox(request.clone(), outbox))
                .abort_handle(),
        ];
        Self {
            request,
            folder_scans: tokio::sync::Mutex::new(Some(folder_scans)),
            delivered: tokio::sync::Mutex::new(Delivered::default()),
            merges: Mutex::new(merges),
        }
    }

    /// Show a different tab, filter, order, or set of folded groups. The
    /// windows are kept: the query reruns and the list re-ingests them.
    pub fn set_view(&self, view: ImportListView) -> Result<u64, ImportListSubscriptionError> {
        self.request.update(|request| request.view = view)
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), ImportListSubscriptionError> {
        self.request
            .update(|request| request.windows = windows)
            .map(|_| ())
    }

    /// The next snapshot: the list's next value beside the scans as they
    /// stand, or the scans' next value beside the list's last one. The first
    /// waits for both.
    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let mut delivered = self.delivered.lock().await;
        loop {
            let arrival = tokio::select! {
                event = self.request.query.next() => {
                    Arrival::List(event.map_err(|_| ImportListSubscriptionError::Cancelled)?)
                }
                value = self.next_folder_scans() => Arrival::FolderScans(value?),
            };
            let scans_only = match arrival {
                Arrival::List(event) => {
                    let request_revision = event.revision().get();
                    let cause = event.cause();
                    self.request.answered(request_revision)?;
                    // A list read that fails takes the whole import tab with
                    // it — no rows, no watched folders — so the reason is
                    // worth a line whether or not anyone is on screen to be
                    // shown it.
                    let projection = match event.into_result() {
                        Ok(projection) => projection,
                        Err(error) => {
                            tracing::error!(
                                "import list query failed at revision {request_revision} \
                                 ({cause:?}): {error}"
                            );
                            return Err(error.into());
                        }
                    };
                    delivered.list = Some(AnsweredList {
                        projection,
                        request_revision,
                        cause,
                    });
                    false
                }
                Arrival::FolderScans(value) => {
                    let folder_scans = match value {
                        Ok(folder_scans) => folder_scans,
                        Err(error) => {
                            tracing::error!("folder scan progress query failed: {error}");
                            return Err(error.into());
                        }
                    };
                    delivered.folder_scans = Some(folder_scans);
                    true
                }
            };
            if let Some(snapshot) = delivered.snapshot(scans_only) {
                return Ok(snapshot);
            }
        }
    }

    /// The scans' next value, or the end of the subscription.
    async fn next_folder_scans(
        &self,
    ) -> Result<coven::CovenResult<FolderScanProgress>, ImportListSubscriptionError> {
        tokio::select! {
            biased;
            () = self.request.query.cancelled() => Err(ImportListSubscriptionError::Cancelled),
            value = async {
                let mut query = self.folder_scans.lock().await;
                let query = query.as_mut().ok_or(ImportListSubscriptionError::Cancelled)?;
                Ok(query.next().await)
            } => value,
        }
    }

    pub async fn cancel(&self) {
        self.stop();
        self.request.query.close().await;
        self.folder_scans.lock().await.take();
    }

    fn stop(&self) {
        self.request.query.cancel();
        for task in self
            .merges
            .lock()
            .expect("import list merge task mutex poisoned")
            .drain(..)
        {
            task.abort();
        }
    }
}

impl Drop for ImportListSubscription {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Apply every runtime change that moves a row to the standing request.
///
/// A progress tick within a running import changes nothing a row shows, so it
/// reconfigures nothing; a run reaching a phase, an import claimed, and an
/// import finishing all do.
async fn merge_runtime(
    request: Arc<StandingRequest>,
    mut changes: broadcast::Receiver<CandidateRuntimeChange>,
    reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot>,
) {
    let idle = TriageRuntimeFacts::default();
    loop {
        let change = tokio::select! {
            () = request.query.cancelled() => return,
            change = changes.recv() => change,
        };
        let merged = match change {
            Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                let next = TriageRuntimeFacts::of(&runtime);
                request.merge(|current| {
                    let moved = current.runtime_facts.get(&key).unwrap_or(&idle) != &next;
                    if next == idle {
                        current.runtime_facts.remove(&key);
                    } else {
                        current.runtime_facts.insert(key, next);
                    }
                    moved
                })
            }
            Ok(CandidateRuntimeChange::Removed { key }) => {
                request.merge(|current| current.runtime_facts.remove(&key).is_some())
            }
            Ok(CandidateRuntimeChange::Reset { runtimes }) => {
                let facts = facts_of(&runtimes);
                request.merge(|current| replace_facts(current, facts))
            }
            Err(broadcast::error::RecvError::Lagged(count)) => {
                tracing::warn!(
                    "the import list dropped {count} runtime changes; \
                     re-reading every candidate's runtime"
                );
                let facts = facts_of(&reread());
                request.merge(|current| replace_facts(current, facts))
            }
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if merged.is_err() {
            return;
        }
    }
}

/// Put `facts` in place of the request's, and report whether that moved any.
fn replace_facts(
    request: &mut ImportListRequest,
    facts: BTreeMap<String, TriageRuntimeFacts>,
) -> bool {
    let moved = request.runtime_facts != facts;
    request.runtime_facts = facts;
    moved
}

/// The placement-relevant facts of every key that has any, keyed the way the
/// request holds them: an idle key is absent rather than present and default.
pub(crate) fn facts_of(
    runtime: &HashMap<String, CandidateRuntimeSnapshot>,
) -> BTreeMap<String, TriageRuntimeFacts> {
    let idle = TriageRuntimeFacts::default();
    runtime
        .iter()
        .map(|(key, runtime)| (key.clone(), TriageRuntimeFacts::of(runtime)))
        .filter(|(_, facts)| facts != &idle)
        .collect()
}

/// Keep the request's upload standing current with the cloud outbox.
///
/// Only the Done tab's order reads it, so a snapshot that moves no release
/// between working, queued and settled reconfigures nothing — byte progress
/// republishes the whole snapshot several times a second.
///
/// A failed outbox read says nothing about where an upload stands, so the order
/// keeps what it had rather than reporting everything settled.
async fn merge_outbox(
    request: Arc<StandingRequest>,
    mut outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
) {
    loop {
        let next = match &*outbox.borrow_and_update() {
            Some(Ok(snapshot)) => Some(UploadStanding::of_outbox(snapshot)),
            Some(Err(_)) | None => None,
        };
        if let Some(next) = next {
            let merged = request.merge(|current| {
                let moved = current.upload_standing != next;
                current.upload_standing = next;
                moved
            });
            if merged.is_err() {
                return;
            }
        }
        let changed = tokio::select! {
            () = request.query.cancelled() => return,
            changed = outbox.changed() => changed,
        };
        if changed.is_err() {
            return;
        }
    }
}
