//! The list's live query, with what the process holds in memory folded into
//! its request, delivered beside where the folder scans stand.
//!
//! Upload standing orders the Done tab — what is moving now, then what is
//! waiting, then what is settled — and the upload pipeline holds it. What is
//! running for each candidate is the candidate runtime's; the request carries
//! the state it puts each candidate in only while a state narrows the view,
//! so a run starting or ending reruns the list only then. The bridge and the
//! UIs never see either.
//!
//! The folder scans are a second live query the subscription reads beside the
//! list: a scan moves its found count with every folder it walks, and that
//! count moves no row, so it never reruns the list.

use super::{
    FolderScanProgress, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListView, UploadStanding,
};
use crate::import::candidate_runtime::RuntimeFactsWatch;
use crate::import::triage::TriageRuntimeFacts;
use crate::library::{LibraryPageWindows, OutboxSnapshot};
use crate::live_query::CancellableLiveQuery;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum ImportListSubscriptionError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The request as it stands, and the query it reconfigures.
struct StandingRequest {
    standing: Mutex<Standing>,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

/// The request, and every candidate's runtime facts its live standings are
/// read from.
struct Standing {
    request: ImportListRequest,
    runtime_facts: HashMap<String, TriageRuntimeFacts>,
}

impl StandingRequest {
    /// Replace part of the standing and hand the whole request to the query,
    /// its live standings read afresh.
    ///
    /// Handed over under the lock, so two changes reach the query in the order
    /// they were made to the request. Repeating the request the query already
    /// has keeps its revision and reruns nothing.
    fn update(
        &self,
        change: impl FnOnce(&mut Standing),
    ) -> Result<u64, ImportListSubscriptionError> {
        let mut standing = self
            .standing
            .lock()
            .expect("import list request mutex poisoned");
        change(&mut standing);
        let Standing {
            request,
            runtime_facts,
        } = &mut *standing;
        request.live_standings = request
            .view
            .pending_filters
            .live_standings(runtime_facts.iter());
        self.query
            .set(request.clone())
            .map_err(|_| ImportListSubscriptionError::Cancelled)
    }
}

pub struct ImportListSubscription {
    request: Arc<StandingRequest>,
    /// Where the folder scans stand. Taken on close, like the list's query.
    folder_scans: tokio::sync::Mutex<Option<coven::LiveQuery<FolderScanProgress>>>,
    /// The last value of each query, so a change to one delivers beside the
    /// other. A snapshot goes out once both have answered.
    delivered: tokio::sync::Mutex<Delivered>,
    merge: tokio::task::AbortHandle,
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
            selection_revision: list.projection.selection_revision,
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
    /// Start the subscription and the merges that keep its upload standing
    /// and live standings current. A watch channel always holds its current
    /// value, so the outbox merge reads it once before it waits; `initial`'s
    /// live standings were read from `runtime_facts` as it stands.
    pub(crate) fn start(
        query: coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>,
        folder_scans: coven::LiveQuery<FolderScanProgress>,
        initial: ImportListRequest,
        outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
        runtime_facts: RuntimeFactsWatch,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let request = Arc::new(StandingRequest {
            standing: Mutex::new(Standing {
                request: initial,
                runtime_facts: runtime_facts.facts().clone(),
            }),
            query: CancellableLiveQuery::new(query),
        });
        let merge = runtime_handle
            .spawn(merge(request.clone(), outbox, runtime_facts))
            .abort_handle();
        Self {
            request,
            folder_scans: tokio::sync::Mutex::new(Some(folder_scans)),
            delivered: tokio::sync::Mutex::new(Delivered::default()),
            merge,
        }
    }

    /// Show a different tab, filter, order, or set of folded groups. The
    /// windows are kept: the query reruns and the list re-ingests them.
    pub fn set_view(&self, view: ImportListView) -> Result<u64, ImportListSubscriptionError> {
        self.request.update(|standing| standing.request.view = view)
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), ImportListSubscriptionError> {
        self.request
            .update(|standing| standing.request.windows = windows)
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
        self.merge.abort();
    }
}

impl Drop for ImportListSubscription {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Keep the request's upload standing current with the cloud outbox, and its
/// live standings with the candidate runtime.
///
/// Byte progress republishes the whole outbox snapshot several times a second;
/// one that moves no release between working, queued and settled hands the
/// query the request it already has, which reruns nothing — as does a run
/// starting or ending while no state narrows the view.
///
/// A failed outbox read says nothing about where an upload stands, so the order
/// keeps what it had rather than reporting everything settled.
async fn merge(
    request: Arc<StandingRequest>,
    mut outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
    mut runtime_facts: RuntimeFactsWatch,
) {
    let mut outbox_changed = true;
    loop {
        let result = if outbox_changed {
            match &*outbox.borrow_and_update() {
                Some(Ok(snapshot)) => {
                    let next = UploadStanding::of_outbox(snapshot);
                    request.update(|standing| standing.request.upload_standing = next)
                }
                Some(Err(_)) | None => Ok(0),
            }
        } else {
            let facts = runtime_facts.facts().clone();
            request.update(|standing| standing.runtime_facts = facts)
        };
        if result.is_err() {
            return;
        }
        outbox_changed = tokio::select! {
            () = request.query.cancelled() => return,
            changed = outbox.changed() => match changed {
                Ok(()) => true,
                Err(_) => return,
            },
            changed = runtime_facts.changed() => match changed {
                true => false,
                false => return,
            },
        };
    }
}
