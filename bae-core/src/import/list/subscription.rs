//! The list's live query, with what this process holds and no table does
//! folded into its request.
//!
//! Three such facts. Two place a row: whether an import has claimed the
//! candidate and how far actual identification work has got, both from
//! [`CandidateRuntime`](crate::import::CandidateRuntime). The third orders one:
//! where an imported release's cloud upload stands, from the outbox. The
//! subscription owns the merges — it keeps the current request,
//! applies each change that moves a row, and hands the query a new request. The
//! bridge and the UIs never see any of it.

use super::{
    ImportListProjection, ImportListRequest, ImportListSnapshot, ImportListView, UploadStanding,
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
struct StandingRequest {
    current: Mutex<ImportListRequest>,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

impl StandingRequest {
    /// Replace part of the request and hand the whole thing to the query.
    fn update(
        &self,
        change: impl FnOnce(&mut ImportListRequest),
    ) -> Result<u64, ImportListSubscriptionError> {
        let next = {
            let mut current = self
                .current
                .lock()
                .expect("import list request mutex poisoned");
            change(&mut current);
            current.clone()
        };
        self.query
            .set(next)
            .map_err(|_| ImportListSubscriptionError::Cancelled)
    }
}

pub struct ImportListSubscription {
    request: Arc<StandingRequest>,
    merges: Mutex<Vec<tokio::task::AbortHandle>>,
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
        initial: ImportListRequest,
        changes: broadcast::Receiver<CandidateRuntimeChange>,
        reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot> + Send + 'static,
        outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let request = Arc::new(StandingRequest {
            current: Mutex::new(initial),
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

    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let event = self
            .request
            .query
            .next()
            .await
            .map_err(|_| ImportListSubscriptionError::Cancelled)?;
        let request_revision = event.revision().get();
        let cause = event.cause();
        // A list read that fails takes the whole import tab with it — no rows,
        // no watched folders, no scan statuses — so the reason is worth a line
        // whether or not anyone is on screen to be shown it.
        let projection = match event.into_result() {
            Ok(projection) => projection,
            Err(error) => {
                tracing::error!(
                    "import list query failed at revision {request_revision} ({cause:?}): {error}"
                );
                return Err(error.into());
            }
        };
        Ok(ImportListSnapshot {
            windows: projection.windows,
            total_count: projection.total_count,
            summary: projection.summary,
            request_revision,
            cause,
        })
    }

    pub async fn cancel(&self) {
        self.stop();
        self.request.query.close().await;
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

/// Apply every runtime change that moves a placement to the standing request.
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
        let moved = match change {
            Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                let next = TriageRuntimeFacts::of(&runtime);
                let mut current = request
                    .current
                    .lock()
                    .expect("import list request mutex poisoned");
                let moved = current.runtime_facts.get(&key).unwrap_or(&idle) != &next;
                if next == idle {
                    current.runtime_facts.remove(&key);
                } else {
                    current.runtime_facts.insert(key, next);
                }
                moved
            }
            Ok(CandidateRuntimeChange::Removed { key }) => request
                .current
                .lock()
                .expect("import list request mutex poisoned")
                .runtime_facts
                .remove(&key)
                .is_some(),
            Ok(CandidateRuntimeChange::Reset { runtimes }) => {
                let facts = facts_of(&runtimes);
                let mut current = request
                    .current
                    .lock()
                    .expect("import list request mutex poisoned");
                let moved = current.runtime_facts != facts;
                current.runtime_facts = facts;
                moved
            }
            Err(broadcast::error::RecvError::Lagged(count)) => {
                tracing::warn!(
                    "the import list dropped {count} runtime changes; \
                     re-reading every candidate's runtime"
                );
                let facts = facts_of(&reread());
                let mut current = request
                    .current
                    .lock()
                    .expect("import list request mutex poisoned");
                let moved = current.runtime_facts != facts;
                current.runtime_facts = facts;
                moved
            }
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if moved && request.update(|_| {}).is_err() {
            return;
        }
    }
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
            let moved = {
                let mut current = request
                    .current
                    .lock()
                    .expect("import list request mutex poisoned");
                let moved = current.upload_standing != next;
                current.upload_standing = next;
                moved
            };
            if moved && request.update(|_| {}).is_err() {
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
