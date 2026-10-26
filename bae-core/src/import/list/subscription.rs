//! The list's live query, with what the process holds in memory folded into
//! its request, delivered beside where the folder scans stand.
//!
//! Upload standing orders the Done tab — what is moving now, then what is
//! waiting, then what is settled — and the upload pipeline holds it, so the
//! request carries it. Where each candidate stands is the standings table's,
//! read with the rows, so every read carries it composed: what each row offers,
//! the state it is in, Found's filter counts, and what the selection offers.
//! Scrolling moves the windows, which is one more request to the same query.
//!
//! The folder scans are a second live query the subscription reads beside the
//! list: a scan moves its found count with every folder it walks, and that
//! count moves no row, so it never reruns the list.
//!
//! The subscription owns the list's selection, and the selection only ever
//! holds rows the list shows. It travels in the request, so the read marks
//! the rows it selects; the read that answers the current request takes every
//! selected row it does not show out of the selection before its snapshot is
//! delivered — a view that hides a row, or a row whose state moves it out of
//! the filter — and hands the query the request without it, so a row that
//! comes back later comes back unselected. What the selection offers rides on
//! every snapshot.

use super::{
    FolderScanProgress, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListView, UploadStatus,
};
use crate::import::selection::{SelectedRows, SelectionChange};
use crate::library::{LibraryPageWindows, OutboxState};
use crate::live_query::CancellableLiveQuery;
use std::collections::{BTreeSet, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum ImportListSubscriptionError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error("the list does not show both ends of the run of rows to select")]
    RunNotShown,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The live query ending is this subscription's cancellation.
impl From<crate::live_query::LiveQueryCancelled> for ImportListSubscriptionError {
    fn from(_: crate::live_query::LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// The request as it stands, and the query it reconfigures.
struct StandingRequest {
    standing: Mutex<Standing>,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

/// The request, the revision the query was last handed it at, and what the
/// read answering it showed.
struct Standing {
    request: ImportListRequest,
    revision: u64,
    /// The person's changes to the selection, and the reads that took rows
    /// they hide out of it.
    selection_revision: u64,
    /// What the read that answered the current request showed: every row's
    /// key in its order, and the selected rows among them.
    shown_keys: Vec<String>,
    selected_rows: SelectedRows,
}

impl StandingRequest {
    /// Replace part of the standing and hand the whole request to the query.
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
            request, revision, ..
        } = &mut *standing;
        *revision = self
            .query
            .set(request.clone())
            .map_err(ImportListSubscriptionError::from)?;
        Ok(*revision)
    }

    /// Make the selection what `next` says from the standing, and hand the
    /// query the request carrying it. Answers with the selection revision.
    fn select(
        &self,
        next: impl FnOnce(&Standing) -> Result<BTreeSet<String>, ImportListSubscriptionError>,
    ) -> Result<u64, ImportListSubscriptionError> {
        let mut refused = None;
        self.update(|standing| match next(standing) {
            Ok(next) => {
                if next != standing.request.selected {
                    standing.request.selected = next;
                    standing.selection_revision += 1;
                }
            }
            Err(error) => refused = Some(error),
        })?;
        match refused {
            Some(error) => Err(error),
            None => Ok(self.read(|standing| standing.selection_revision)),
        }
    }

    /// Take what the read answering `revision` showed as what the list
    /// shows, and, when it answers the current request, take every selected
    /// row it does not show out of the selection, handing the query the
    /// request without them.
    fn settle_selection(
        &self,
        revision: u64,
        projection: &super::ImportListProjection,
    ) -> Result<(), ImportListSubscriptionError> {
        let mut hid_selected = false;
        {
            let mut standing = self
                .standing
                .lock()
                .expect("import list request mutex poisoned");
            if standing.revision != revision {
                return Ok(());
            }
            let shown: HashSet<&str> = projection.shown_keys.iter().map(String::as_str).collect();
            let before = standing.request.selected.len();
            standing
                .request
                .selected
                .retain(|key| shown.contains(key.as_str()));
            if standing.request.selected.len() != before {
                standing.selection_revision += 1;
                hid_selected = true;
            }
            standing.shown_keys = projection.shown_keys.clone();
            standing.selected_rows = projection.selected.clone();
        }
        if hid_selected {
            self.update(|_| {})?;
        }
        Ok(())
    }

    fn read<R>(&self, read: impl FnOnce(&Standing) -> R) -> R {
        read(
            &self
                .standing
                .lock()
                .expect("import list request mutex poisoned"),
        )
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
    cause: coven::LiveQueryCause,
}

impl Delivered {
    /// The snapshot due once both queries have answered. The scans' value
    /// beside the list's last read is, after the first, the same request's
    /// value changing.
    fn snapshot(&mut self, beside_list: bool, selection_revision: u64) -> Option<ImportListSnapshot> {
        let (Some(list), Some(folder_scans)) = (&self.list, &self.folder_scans) else {
            return None;
        };
        let cause = if beside_list && self.sent {
            coven::LiveQueryCause::Write
        } else {
            list.cause
        };
        let snapshot = ImportListSnapshot {
            windows: list.projection.windows.clone(),
            total_count: list.projection.total_count,
            summary: list.projection.summary.clone(),
            pending_filter_entries: list.projection.pending_filter_entries.clone(),
            folder_scans: folder_scans.clone(),
            selection_revision,
            selection: list.projection.selection.clone(),
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
    /// Open the list for `view` through `windows` and start the merge that
    /// keeps its request current.
    ///
    /// The request's upload status is read here, from the same outbox watch
    /// the merge then follows, and `open` opens the query with that request:
    /// the query starts agreeing with what the merge would hand it, so the
    /// merge's first change to the request is a real change to the queue. A
    /// new list opens with nothing selected.
    pub(crate) fn start(
        open: impl FnOnce(
            ImportListRequest,
        ) -> coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection>,
        folder_scans: coven::LiveQuery<FolderScanProgress>,
        view: ImportListView,
        windows: LibraryPageWindows,
        mut outbox: watch::Receiver<Option<Result<OutboxState, String>>>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let initial = ImportListRequest {
            view,
            windows,
            upload_status: UploadStatus::of(&outbox.borrow_and_update()),
            selected: BTreeSet::new(),
        };
        let query = open(initial.clone());
        let request = Arc::new(StandingRequest {
            standing: Mutex::new(Standing {
                request: initial,
                // A query starts at revision zero.
                revision: 0,
                selection_revision: 0,
                shown_keys: Vec::new(),
                selected_rows: SelectedRows::default(),
            }),
            query: CancellableLiveQuery::new(query),
        });
        let merge = runtime_handle
            .spawn(merge(request.clone(), outbox))
            .abort_handle();
        Self {
            request,
            folder_scans: tokio::sync::Mutex::new(Some(folder_scans)),
            delivered: tokio::sync::Mutex::new(Delivered {
                list: None,
                folder_scans: None,
                sent: false,
            }),
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

    /// Apply a change a person made by pointing at rows of the list, and
    /// answer with the selection revision of the snapshot that reflects it.
    pub fn change_selection(
        &self,
        change: SelectionChange,
    ) -> Result<u64, ImportListSubscriptionError> {
        self.request.select(|standing| {
            change
                .applied(&standing.request.selected, &standing.shown_keys)
                .ok_or(ImportListSubscriptionError::RunNotShown)
        })
    }

    /// Select every row the list shows, whether or not a surface loaded it.
    pub fn select_all(&self) -> Result<u64, ImportListSubscriptionError> {
        self.request
            .select(|standing| Ok(standing.shown_keys.iter().cloned().collect()))
    }

    /// The selected rows, in the order the list shows them, as the read that
    /// answered the current request placed them: what a bulk action runs
    /// over.
    pub fn selected(&self) -> SelectedRows {
        self.request.read(|standing| standing.selected_rows.clone())
    }

    /// The next snapshot: the list's next value beside the scans as they
    /// stand, or the scans' next value beside the list's last one. The first
    /// waits for both queries.
    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let mut delivered = self.delivered.lock().await;
        loop {
            let arrival = tokio::select! {
                event = self.request.query.next() => {
                    Arrival::List(event.map_err(ImportListSubscriptionError::from)?)
                }
                value = self.next_folder_scans() => Arrival::FolderScans(value?),
            };
            let beside_list = match arrival {
                Arrival::List(event) => {
                    let request_revision = event.revision.0;
                    let cause = event.cause;
                    // A list read that fails takes the whole import tab with
                    // it — no rows, no watched folders — so the reason is
                    // worth a line whether or not anyone is on screen to be
                    // shown it.
                    let projection = match event.result {
                        Ok(projection) => projection,
                        Err(error) => {
                            tracing::error!(
                                "import list query failed at revision {request_revision} \
                                 ({cause:?}): {error}"
                            );
                            return Err(error.into());
                        }
                    };
                    self.request.settle_selection(request_revision, &projection)?;
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
            let selection_revision = self.request.read(|standing| standing.selection_revision);
            let snapshot = delivered.snapshot(beside_list, selection_revision);
            if let Some(snapshot) = snapshot {
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
/// Byte progress republishes the whole outbox snapshot several times a second;
/// one that moves no release between working, queued and settled hands the
/// query the request it already has, which reruns nothing.
///
/// The upload status comes from [`UploadStatus::of`], the same reading the
/// request opened with: a failed outbox read makes it unreadable, which the
/// Done tab says and orders by import time. The request opened with it as it
/// stood, so the merge first waits for it to change.
async fn merge(
    request: Arc<StandingRequest>,
    mut outbox: watch::Receiver<Option<Result<OutboxState, String>>>,
) {
    loop {
        tokio::select! {
            () = request.query.cancelled() => return,
            changed = outbox.changed() => if changed.is_err() {
                return;
            },
        }
        let next = UploadStatus::of(&outbox.borrow_and_update());
        if request
            .update(|standing| standing.request.upload_status = next)
            .is_err()
        {
            return;
        }
    }
}
