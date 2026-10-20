//! The list's live query, with what the process holds in memory folded into
//! its request and joined to its rows, delivered beside where the folder scans
//! stand.
//!
//! Upload standing orders the Done tab — what is moving now, then what is
//! waiting, then what is settled — and the upload pipeline holds it. What is
//! running for each candidate is the candidate runtime's; the request carries
//! the state it puts each candidate in only while a filter entry past All
//! narrows the view, so a run starting or ending reruns the list only then.
//! The bridge and the UIs never see either.
//!
//! What is running for a candidate is also joined to its row on the page, in
//! memory, as the row's
//! [`CandidateLiveState`](crate::import::CandidateLiveState): a change to it
//! for a key on the page delivers the page again with no read, and one for a
//! key off the page delivers nothing. A progress tick within a running import
//! changes no row's facts, so it delivers nothing either. Scrolling moves the
//! windows, which is one more request to the same query.
//!
//! The folder scans are a second live query the subscription reads beside the
//! list: a scan moves its found count with every folder it walks, and that
//! count moves no row, so it never reruns the list.
//!
//! Every snapshot carries Found's filter entries with their counts: the
//! states the list's last read placed Found's rows in, each overridden by what
//! is running for its candidate, joined in the same read as the page's rows.
//! A change that moves a count — a run starting for a row off the page under
//! All, or a read that changed only rows off the page — delivers the snapshot
//! again even though nothing on the page moved.

use super::{
    FolderScanProgress, FoundStates, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListView, PendingFilterEntry, UploadStanding,
};
use crate::import::candidate_runtime::RuntimeFactsWatch;
use crate::import::triage::{LiveStanding, TriageRuntimeFacts};
use crate::library::{LibraryPageWindows, OutboxSnapshot};
use crate::live_query::CancellableLiveQuery;
use std::collections::{BTreeMap, HashMap};
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
    /// Told each time the runtime facts change, after they are stored.
    facts_changed: watch::Sender<()>,
}

/// The request, the revision the query was last handed it at, and every
/// candidate's runtime facts: what its live standings are read from, and what
/// is joined to the rows it reads.
struct Standing {
    request: ImportListRequest,
    revision: u64,
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
            revision,
            runtime_facts,
        } = &mut *standing;
        request.live_standings = request
            .view
            .pending_filter
            .live_standings(runtime_facts.iter());
        *revision = self
            .query
            .set(request.clone())
            .map_err(|_| ImportListSubscriptionError::Cancelled)?;
        Ok(*revision)
    }

    /// Store every candidate's runtime facts and read the request afresh,
    /// then tell the delivery the facts changed.
    fn set_runtime_facts(
        &self,
        facts: HashMap<String, TriageRuntimeFacts>,
    ) -> Result<u64, ImportListSubscriptionError> {
        let revision = self.update(|standing| standing.runtime_facts = facts)?;
        self.facts_changed.send_replace(());
        Ok(revision)
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
    /// The runtime facts of the candidates on the last snapshot's page that
    /// have any, by key: what its rows were joined with.
    page_facts: BTreeMap<String, TriageRuntimeFacts>,
    /// The filter entries the last snapshot carried.
    pending_filter_entries: Vec<PendingFilterEntry>,
    /// Told when the runtime facts change.
    facts_changed: watch::Receiver<()>,
}

/// The list's last value, the states it placed Found's rows in, the request
/// revision it answered and why it was read.
struct AnsweredList {
    projection: ImportListProjection,
    found_states: FoundStates,
    request_revision: u64,
    cause: coven::LiveQueryCause,
}

impl AnsweredList {
    /// Every entry of Found's filter, with how many of Found's rows it holds:
    /// each row in the state what is running for its candidate puts it in,
    /// or else in the one this read placed it in.
    fn pending_filter_entries(
        &self,
        facts: &HashMap<String, TriageRuntimeFacts>,
    ) -> Vec<PendingFilterEntry> {
        self.found_states.entries(&LiveStanding::of_each(facts.iter()))
    }

    /// The runtime facts of the candidates on this read's page that have any.
    fn page_facts(
        &self,
        facts: &HashMap<String, TriageRuntimeFacts>,
    ) -> BTreeMap<String, TriageRuntimeFacts> {
        self.projection
            .windows
            .iter()
            .flat_map(|window| &window.items)
            .filter_map(|item| item.candidate_key())
            .filter_map(|key| facts.get(key).map(|facts| (key.to_string(), facts.clone())))
            .collect()
    }
}

impl Delivered {
    /// The snapshot due once both queries have answered, its rows joined with
    /// `facts` and its filter entries counted with them. A value beside the
    /// list's last read — the scans alone, what is running for its rows, or
    /// counts that moved off the page — is, after the first, the same
    /// request's value changing.
    fn snapshot(
        &mut self,
        beside_list: bool,
        facts: &HashMap<String, TriageRuntimeFacts>,
    ) -> Option<ImportListSnapshot> {
        let (Some(list), Some(folder_scans)) = (&self.list, &self.folder_scans) else {
            return None;
        };
        let cause = if beside_list && self.sent {
            coven::LiveQueryCause::Write
        } else {
            list.cause
        };
        self.page_facts = list.page_facts(facts);
        self.pending_filter_entries = list.pending_filter_entries(facts);
        let snapshot = ImportListSnapshot {
            windows: list
                .projection
                .windows
                .iter()
                .cloned()
                .map(|window| window.with_live(facts))
                .collect(),
            total_count: list.projection.total_count,
            summary: list.projection.summary.clone(),
            pending_filter_entries: self.pending_filter_entries.clone(),
            folder_scans: folder_scans.clone(),
            selection_revision: list.projection.selection_revision,
            request_revision: list.request_revision,
            cause,
        };
        self.sent = true;
        Some(snapshot)
    }
}

/// What one wait produced: a list event, a folder-scan value, or a change to
/// what is running for the candidates.
enum Arrival {
    List(coven::ReconfigurableLiveQueryEvent<ImportListRequest, ImportListProjection>),
    FolderScans(coven::CovenResult<FolderScanProgress>),
    RuntimeFacts,
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
        let (facts_changed, facts_changed_rx) = watch::channel(());
        let request = Arc::new(StandingRequest {
            standing: Mutex::new(Standing {
                request: initial,
                // A query starts at revision zero.
                revision: 0,
                runtime_facts: runtime_facts.facts().clone(),
            }),
            query: CancellableLiveQuery::new(query),
            facts_changed,
        });
        let merge = runtime_handle
            .spawn(merge(request.clone(), outbox, runtime_facts))
            .abort_handle();
        Self {
            request,
            folder_scans: tokio::sync::Mutex::new(Some(folder_scans)),
            delivered: tokio::sync::Mutex::new(Delivered {
                list: None,
                folder_scans: None,
                sent: false,
                page_facts: BTreeMap::new(),
                pending_filter_entries: Vec::new(),
                facts_changed: facts_changed_rx,
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

    /// The next snapshot: the list's next value beside the scans as they
    /// stand, the scans' next value beside the list's last one, or the list's
    /// last one again once what is running for a row on its page, or the
    /// filter's counts, changed. The first waits for both queries.
    ///
    /// A runtime change while the query owes a read of a newer request
    /// delivers nothing: that read is joined with the facts as they stand when
    /// it answers.
    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let mut delivered = self.delivered.lock().await;
        loop {
            let arrival = tokio::select! {
                event = self.request.query.next() => {
                    Arrival::List(event.map_err(|_| ImportListSubscriptionError::Cancelled)?)
                }
                value = self.next_folder_scans() => Arrival::FolderScans(value?),
                changed = delivered.facts_changed.changed() => {
                    changed.map_err(|_| ImportListSubscriptionError::Cancelled)?;
                    Arrival::RuntimeFacts
                }
            };
            let beside_list = match arrival {
                Arrival::List(event) => {
                    let request_revision = event.revision.0;
                    let cause = event.cause;
                    // A list read that fails takes the whole import tab with
                    // it — no rows, no watched folders — so the reason is
                    // worth a line whether or not anyone is on screen to be
                    // shown it.
                    let mut projection = match event.result {
                        Ok(projection) => projection,
                        Err(error) => {
                            tracing::error!(
                                "import list query failed at revision {request_revision} \
                                 ({cause:?}): {error}"
                            );
                            return Err(error.into());
                        }
                    };
                    let found_states = std::mem::take(&mut projection.found_states);
                    // A read that changed only the states of rows off the page
                    // moves nothing on screen: it is the same request's value
                    // again, delivered only when it moves the filter's counts.
                    let Delivered {
                        list: answered,
                        sent,
                        pending_filter_entries: delivered_entries,
                        ..
                    } = &mut *delivered;
                    if let Some(list) = answered.as_mut().filter(|list| {
                        list.request_revision == request_revision && list.projection == projection
                    }) {
                        list.found_states = found_states;
                        let entries = self
                            .request
                            .read(|standing| list.pending_filter_entries(&standing.runtime_facts));
                        if !*sent || entries == *delivered_entries {
                            continue;
                        }
                        true
                    } else {
                        *answered = Some(AnsweredList {
                            projection,
                            found_states,
                            request_revision,
                            cause,
                        });
                        false
                    }
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
                Arrival::RuntimeFacts => {
                    let Some(list) = &delivered.list else { continue };
                    let (owed, page_facts, entries) = self.request.read(|standing| {
                        (
                            standing.revision != list.request_revision,
                            list.page_facts(&standing.runtime_facts),
                            list.pending_filter_entries(&standing.runtime_facts),
                        )
                    });
                    if !delivered.sent
                        || owed
                        || (page_facts == delivered.page_facts
                            && entries == delivered.pending_filter_entries)
                    {
                        continue;
                    }
                    true
                }
            };
            let snapshot = self
                .request
                .read(|standing| delivered.snapshot(beside_list, &standing.runtime_facts));
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

/// Keep the request's upload standing current with the cloud outbox, and its
/// live standings with the candidate runtime.
///
/// Byte progress republishes the whole outbox snapshot several times a second;
/// one that moves no release between working, queued and settled hands the
/// query the request it already has, which reruns nothing — as does a run
/// starting or ending while Found's filter is on All.
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
            request.set_runtime_facts(runtime_facts.facts().clone())
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
