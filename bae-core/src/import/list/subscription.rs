//! The list's live query, with where imported releases' uploads stand folded
//! into its request, delivered beside where the folder scans stand.
//!
//! Upload standing orders the Done tab — what is moving now, then what is
//! waiting, then what is settled — and the upload pipeline holds it in memory
//! rather than in a table the query reads, so the subscription keeps it in the
//! request. Nothing else the process holds is: what is running for a candidate
//! moves no row, and each row reads it from its own subscription. The bridge
//! and the UIs never see the request's upload standing.
//!
//! The folder scans are a second live query the subscription reads beside the
//! list: a scan moves its found count with every folder it walks, and that
//! count moves no row, so it never reruns the list.

use super::{
    FolderScanProgress, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListView, UploadStanding,
};
use crate::library::{LibraryPageWindows, OutboxSnapshot};
use crate::live_query::CancellableLiveQuery;
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
    request: Mutex<ImportListRequest>,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

impl StandingRequest {
    /// Replace part of the request and hand the whole of it to the query.
    ///
    /// Handed over under the lock, so two changes reach the query in the order
    /// they were made to the request. Repeating the request the query already
    /// has keeps its revision and reruns nothing.
    fn update(
        &self,
        change: impl FnOnce(&mut ImportListRequest),
    ) -> Result<u64, ImportListSubscriptionError> {
        let mut request = self
            .request
            .lock()
            .expect("import list request mutex poisoned");
        change(&mut request);
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
    /// Start the subscription and the merge that keeps its upload standing
    /// current. A watch channel always holds its current value, so the merge
    /// reads it once before it waits.
    pub(crate) fn start(
        query: coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>,
        folder_scans: coven::LiveQuery<FolderScanProgress>,
        initial: ImportListRequest,
        outbox: watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let request = Arc::new(StandingRequest {
            request: Mutex::new(initial),
            query: CancellableLiveQuery::new(query),
        });
        let merge = runtime_handle
            .spawn(merge_outbox(request.clone(), outbox))
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

/// Keep the request's upload standing current with the cloud outbox.
///
/// Byte progress republishes the whole snapshot several times a second; one
/// that moves no release between working, queued and settled hands the query
/// the request it already has, which reruns nothing.
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
            if request
                .update(|current| current.upload_standing = next)
                .is_err()
            {
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
