//! The list's live query, with the runtime this process holds folded into its
//! request.
//!
//! Two of the four facts a row's placement reads are not in a table: whether
//! an import has claimed the candidate, and how far identification has got for
//! one with no stored verdict. They live in
//! [`CandidateRuntime`](crate::import::CandidateRuntime), so the subscription
//! owns the merge — it keeps the current request, applies each runtime change
//! that moves a placement, and hands the query a new request. The bridge and
//! the UIs never see the runtime facts at all.

use super::{ImportListProjection, ImportListRequest, ImportListSnapshot, ImportListView};
use crate::import::triage::TriageRuntimeFacts;
use crate::import::{CandidateRuntimeChange, CandidateRuntimeSnapshot};
use crate::library::LibraryPageWindows;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum ImportListSubscriptionError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The request as it stands, and the handle that reconfigures the query.
struct Requests {
    requests: Mutex<Option<coven::LiveQueryRequests<ImportListRequest>>>,
    current: Mutex<ImportListRequest>,
    cancellation: CancellationToken,
}

impl Requests {
    /// Replace part of the request and hand the whole thing to the query.
    fn update(
        &self,
        change: impl FnOnce(&mut ImportListRequest),
    ) -> Result<(), ImportListSubscriptionError> {
        if self.cancellation.is_cancelled() {
            return Err(ImportListSubscriptionError::Cancelled);
        }
        let next = {
            let mut current = self
                .current
                .lock()
                .expect("import list request mutex poisoned");
            change(&mut current);
            current.clone()
        };
        self.requests
            .lock()
            .expect("import list request mutex poisoned")
            .as_ref()
            .ok_or(ImportListSubscriptionError::Cancelled)?
            .set(next)
            .expect("the import list subscription owns its live query");
        Ok(())
    }

    fn cancel(&self) {
        self.cancellation.cancel();
        self.requests
            .lock()
            .expect("import list request mutex poisoned")
            .take();
    }
}

pub struct ImportListSubscription {
    requests: Arc<Requests>,
    query: tokio::sync::Mutex<
        Option<coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>>,
    >,
    runtime_task: Mutex<Option<tokio::task::AbortHandle>>,
}

impl ImportListSubscription {
    /// Start the subscription and the runtime merge behind it.
    ///
    /// `initial` must already carry the runtime facts the caller read before
    /// it took `changes`, so no change lands between the two.
    pub(crate) fn start(
        query: coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>,
        initial: ImportListRequest,
        changes: broadcast::Receiver<CandidateRuntimeChange>,
        reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot> + Send + 'static,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let requests = Arc::new(Requests {
            requests: Mutex::new(Some(query.requests())),
            current: Mutex::new(initial),
            cancellation: CancellationToken::new(),
        });
        let task = runtime_handle.spawn(merge_runtime(requests.clone(), changes, reread));
        Self {
            requests,
            query: tokio::sync::Mutex::new(Some(query)),
            runtime_task: Mutex::new(Some(task.abort_handle())),
        }
    }

    /// Show a different tab, filter, order, or set of folded groups. The
    /// windows are kept: the query reruns and the list re-ingests them.
    pub fn set_view(&self, view: ImportListView) -> Result<(), ImportListSubscriptionError> {
        self.requests.update(|request| request.view = view)
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), ImportListSubscriptionError> {
        self.requests.update(|request| request.windows = windows)
    }

    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let event = tokio::select! {
            biased;
            () = self.requests.cancellation.cancelled() => {
                return Err(ImportListSubscriptionError::Cancelled);
            }
            event = async {
                let mut query = self.query.lock().await;
                let query = query
                    .as_mut()
                    .ok_or(ImportListSubscriptionError::Cancelled)?;
                Ok::<_, ImportListSubscriptionError>(query.next().await)
            } => event?,
        };
        let request_revision = event.revision().get();
        let cause = event.cause();
        let projection = event.into_result()?;
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
        self.query.lock().await.take();
    }

    fn stop(&self) {
        self.requests.cancel();
        if let Some(task) = self
            .runtime_task
            .lock()
            .expect("import list runtime task mutex poisoned")
            .take()
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
/// A progress tick or a signals update changes nothing a row shows, so it
/// reconfigures nothing; a run reaching a phase, an import claimed, and an
/// import finishing all do.
async fn merge_runtime(
    requests: Arc<Requests>,
    mut changes: broadcast::Receiver<CandidateRuntimeChange>,
    reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot>,
) {
    let idle = TriageRuntimeFacts::default();
    loop {
        let change = tokio::select! {
            () = requests.cancellation.cancelled() => return,
            change = changes.recv() => change,
        };
        let moved = match change {
            Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                let next = TriageRuntimeFacts::of(&runtime);
                let mut current = requests
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
            Ok(CandidateRuntimeChange::Removed { key }) => requests
                .current
                .lock()
                .expect("import list request mutex poisoned")
                .runtime_facts
                .remove(&key)
                .is_some(),
            Err(broadcast::error::RecvError::Lagged(count)) => {
                tracing::warn!(
                    "the import list dropped {count} runtime changes; \
                     re-reading every candidate's runtime"
                );
                let facts = facts_of(&reread());
                let mut current = requests
                    .current
                    .lock()
                    .expect("import list request mutex poisoned");
                let moved = current.runtime_facts != facts;
                current.runtime_facts = facts;
                moved
            }
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if moved && requests.update(|_| {}).is_err() {
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
