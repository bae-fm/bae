//! Shared in-memory serial queue for per-release background work, and the serial
//! worker that drains one.
//!
//! Pinning and exporting both process one release at a time, keep transient
//! in-memory rows for the queue pane, and support pause, cancel, and retry.
//! `Extra` carries the operation-specific row payload: downloads use `()`,
//! exports use their target directory. `Progress` carries the active-row
//! progress shape. [`run_serial_worker`] is the drain protocol both share — the
//! activate/cancel race in particular is written once here rather than once per
//! queue.

use std::future::Future;
use std::sync::Mutex;

use tokio::sync::Notify;
use tracing::{debug, error, warn};

use super::LibraryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseQueueState<Progress> {
    Queued,
    Active { progress: Progress },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct ReleaseQueueOp<Extra, Progress> {
    pub release_id: String,
    pub title: String,
    pub file_count: i64,
    pub total_size: i64,
    pub created_at: i64,
    pub payload: Extra,
    pub state: ReleaseQueueState<Progress>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// One part of a queue summary line: a catalog key and the count to render it
/// with (e.g. `core.queue.downloading` × 2). Core decides which parts appear, in
/// what order, and that a zero drops out; the UI resolves each key against its
/// catalog and joins the parts. Written five times across the apps before this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountLabel {
    pub key: String,
    pub count: u32,
}

impl ReleaseQueueProgress {
    /// The summary parts for a release queue: its active verb (`active_key`,
    /// e.g. downloading vs exporting), then failed, then queued — each part
    /// dropped when its count is zero. The order and the drop rule are the
    /// decision; only the active verb differs by queue, so the caller names it.
    pub fn summary_parts(&self, active_key: &str) -> Vec<CountLabel> {
        [
            (active_key, self.active),
            ("core.queue.failed", self.failed),
            ("core.queue.queued", self.queued),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(key, count)| CountLabel {
            key: key.to_string(),
            count,
        })
        .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReleaseQueueSnapshot<Extra, Progress> {
    pub ops: Vec<ReleaseQueueOp<Extra, Progress>>,
    pub total: ReleaseQueueProgress,
    pub paused: bool,
}

pub fn build_release_queue_snapshot<Extra: Clone, Progress: Clone>(
    ops: &[ReleaseQueueOp<Extra, Progress>],
    paused: bool,
) -> ReleaseQueueSnapshot<Extra, Progress> {
    let mut total = ReleaseQueueProgress::default();
    for op in ops {
        match &op.state {
            ReleaseQueueState::Queued => total.queued += 1,
            ReleaseQueueState::Active { .. } => total.active += 1,
            ReleaseQueueState::Failed { .. } => total.failed += 1,
        }
    }

    ReleaseQueueSnapshot {
        ops: ops.to_vec(),
        total,
        paused,
    }
}

pub struct ReleaseQueue<Extra, Progress> {
    state: Mutex<State<Extra, Progress>>,
    notify: Notify,
}

struct State<Extra, Progress> {
    ops: Vec<ReleaseQueueOp<Extra, Progress>>,
    paused: bool,
    active_abort: Option<tokio::task::AbortHandle>,
}

impl<Extra: Clone, Progress: Clone> ReleaseQueue<Extra, Progress> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                ops: Vec::new(),
                paused: false,
                active_abort: None,
            }),
            notify: Notify::new(),
        }
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    pub async fn wait(&self) {
        self.notify.notified().await;
    }

    pub fn ops(&self) -> Vec<ReleaseQueueOp<Extra, Progress>> {
        self.state.lock().unwrap().ops.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    pub fn enqueue(&self, op: ReleaseQueueOp<Extra, Progress>) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.ops.iter().any(|o| o.release_id == op.release_id) {
            return false;
        }
        state.ops.push(op);
        true
    }

    pub fn contains(&self, release_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.release_id == release_id)
    }

    pub fn next_queued(&self) -> Option<ReleaseQueueOp<Extra, Progress>> {
        let state = self.state.lock().unwrap();
        if state.paused {
            return None;
        }
        state
            .ops
            .iter()
            .find(|o| matches!(o.state, ReleaseQueueState::Queued))
            .cloned()
    }

    pub fn activate(
        &self,
        release_id: &str,
        abort: tokio::task::AbortHandle,
        progress: Progress,
    ) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(op) = state
            .ops
            .iter_mut()
            .find(|o| o.release_id == release_id && matches!(o.state, ReleaseQueueState::Queued))
        else {
            return false;
        };
        op.state = ReleaseQueueState::Active { progress };
        state.active_abort = Some(abort);
        true
    }

    pub fn remove(&self, release_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.ops.retain(|o| o.release_id != release_id);
    }

    pub fn mark_failed(&self, release_id: &str, error: String) {
        let mut state = self.state.lock().unwrap();
        if let Some(op) = state.ops.iter_mut().find(|o| o.release_id == release_id) {
            op.state = ReleaseQueueState::Failed { error };
        }
    }

    pub fn retry_failed(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut any = false;
        for op in state.ops.iter_mut() {
            if matches!(op.state, ReleaseQueueState::Failed { .. }) {
                op.state = ReleaseQueueState::Queued;
                any = true;
            }
        }
        any
    }

    pub fn set_paused(&self, paused: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        std::mem::replace(&mut state.paused, paused)
    }

    pub fn clear_active_abort(&self) {
        self.state.lock().unwrap().active_abort = None;
    }

    pub fn set_active_progress(&self, release_id: &str, progress: Progress) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(op) = state.ops.iter_mut().find(|o| {
            o.release_id == release_id && matches!(o.state, ReleaseQueueState::Active { .. })
        }) else {
            return false;
        };
        op.state = ReleaseQueueState::Active { progress };
        true
    }

    pub fn cancel(&self, release_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_active = state.ops.iter().any(|o| {
            o.release_id == release_id && matches!(o.state, ReleaseQueueState::Active { .. })
        });
        if was_active {
            if let Some(abort) = state.active_abort.take() {
                abort.abort();
            }
        }
        state.ops.retain(|o| o.release_id != release_id);
        was_active
    }
}

impl<Extra: Clone, Progress: Clone> Default for ReleaseQueue<Extra, Progress> {
    fn default() -> Self {
        Self::new()
    }
}

/// A queued operation that has been started: the handle a cancel aborts, and the
/// future that yields the operation's outcome.
pub struct RunningOp<Fut> {
    pub abort: tokio::task::AbortHandle,
    pub outcome: Fut,
}

/// Drain a queue serially, one release at a time, forever. The caller supplies
/// only `start` — resolve the release's initial progress, spawn its work, and hand
/// back the abort handle plus the future that yields its outcome — and the hooks to
/// re-emit the queue snapshot (`on_changed`) and to report the outcome to
/// diagnostics (`on_done`).
///
/// The protocol, which is why this is one function and not two:
///
/// - A cancel can land in the gap between picking a release and activating it, so
///   [`ReleaseQueue::activate`] flips to `Active` and registers the abort handle
///   under one lock: it fails exactly when the entry is already gone, and then the
///   work we just started is aborted and dropped.
/// - A cancel during the work removes the entry and aborts the task, so
///   `contains` after the await is what tells a cancel from a failure. A cancelled
///   release is neither re-added nor marked failed.
/// - Success removes the entry; failure marks it `Failed` and leaves it for retry.
///   A `start` that fails marks it failed the same way — nothing was spawned.
///
/// Every transition emits a fresh snapshot through `on_changed`.
pub async fn run_serial_worker<Extra, Progress, Start, StartFut, Fut, Changed, Done>(
    queue: &ReleaseQueue<Extra, Progress>,
    label: &'static str,
    mut start: Start,
    mut on_changed: Changed,
    mut on_done: Done,
) where
    Extra: Clone,
    Progress: Clone,
    Start: FnMut(ReleaseQueueOp<Extra, Progress>) -> StartFut,
    StartFut: Future<Output = Result<(Progress, RunningOp<Fut>), LibraryError>>,
    Fut: Future<Output = Result<(), LibraryError>>,
    Changed: FnMut(),
    Done: FnMut(&str, Result<(), &LibraryError>),
{
    loop {
        let Some(op) = queue.next_queued() else {
            queue.wait().await;
            continue;
        };
        let release_id = op.release_id.clone();

        let (progress, running) = match start(op).await {
            Ok(started) => started,
            Err(error) => {
                error!("{label} failed for release {release_id}: {error}");
                on_done(&release_id, Err(&error));
                queue.mark_failed(&release_id, error.to_string());
                on_changed();
                continue;
            }
        };
        let RunningOp { abort, outcome } = running;

        if !queue.activate(&release_id, abort.clone(), progress) {
            abort.abort();
            debug!("{label} for {release_id} cancelled before it started; aborting");
            continue;
        }
        on_changed();

        let result = outcome.await;
        queue.clear_active_abort();

        if !queue.contains(&release_id) {
            debug!("{label} for {release_id} ended after cancel; leaving queue as-is");
            continue;
        }

        match result {
            Ok(()) => {
                on_done(&release_id, Ok(()));
                queue.remove(&release_id);
            }
            Err(error) => {
                error!("{label} failed for release {release_id}: {error}");
                on_done(&release_id, Err(&error));
                queue.mark_failed(&release_id, error.to_string());
            }
        }
        on_changed();
    }
}

impl<Extra: Clone> ReleaseQueue<Extra, u8> {
    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
        if !self.set_active_progress(release_id, percent) {
            warn!(
                release_id,
                percent, "ignored active queue percent for missing active release"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// The summary parts read active-then-failed-then-queued, drop any zero, and
    /// take the active verb the caller names — so download and export share this
    /// one decision and differ only in that verb.
    #[test]
    fn summary_parts_order_drop_and_active_verb() {
        let progress = ReleaseQueueProgress {
            queued: 3,
            active: 2,
            failed: 0,
        };
        assert_eq!(
            progress.summary_parts("core.queue.downloading"),
            vec![
                CountLabel {
                    key: "core.queue.downloading".to_string(),
                    count: 2,
                },
                CountLabel {
                    key: "core.queue.queued".to_string(),
                    count: 3,
                },
            ],
            "failed is zero and drops; the active verb is the caller's"
        );
        assert!(
            ReleaseQueueProgress::default()
                .summary_parts("core.queue.exporting")
                .is_empty(),
            "an idle queue has no parts"
        );
    }

    fn op(release_id: &str) -> ReleaseQueueOp<(), u8> {
        ReleaseQueueOp {
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            file_count: 3,
            total_size: 350_000_000,
            created_at: 0,
            payload: (),
            state: ReleaseQueueState::Queued,
        }
    }

    fn dummy_abort() -> tokio::task::AbortHandle {
        tokio::spawn(std::future::ready(())).abort_handle()
    }

    #[test]
    fn enqueue_dedups_by_release_id() {
        let q = ReleaseQueue::new();
        assert!(q.enqueue(op("rel-a")));
        assert!(!q.enqueue(op("rel-a")));
        assert!(q.contains("rel-a"));
        assert_eq!(q.ops().len(), 1);

        assert!(q.enqueue(op("rel-b")));
        assert_eq!(q.ops().len(), 2);
    }

    #[tokio::test]
    async fn next_queued_picks_head_and_respects_pause() {
        let q = ReleaseQueue::new();
        q.enqueue(op("rel-a"));
        q.enqueue(op("rel-b"));

        assert_eq!(q.next_queued().map(|o| o.release_id), Some("rel-a".into()));
        assert!(q.activate("rel-a", dummy_abort(), 0));
        let ops = q.ops();
        assert_eq!(ops[0].state, ReleaseQueueState::Active { progress: 0 });
        assert_eq!(ops[1].state, ReleaseQueueState::Queued);

        assert_eq!(q.next_queued().map(|o| o.release_id), Some("rel-b".into()));
        q.set_paused(true);
        assert!(q.next_queued().is_none());
    }

    #[tokio::test]
    async fn activate_fails_when_entry_cancelled_in_the_gap() {
        let q = ReleaseQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(q.next_queued().map(|o| o.release_id), Some("rel-a".into()));
        q.remove("rel-a");
        assert!(!q.activate("rel-a", dummy_abort(), 0));
    }

    #[tokio::test]
    async fn percent_remove_fail_retry_transitions() {
        let q = ReleaseQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(q.next_queued().map(|o| o.release_id), Some("rel-a".into()));
        assert!(q.activate("rel-a", dummy_abort(), 0));

        q.set_active_percent("rel-a", 50);
        assert_eq!(q.ops()[0].state, ReleaseQueueState::Active { progress: 50 });

        q.mark_failed("rel-a", "boom".to_string());
        assert_eq!(
            q.ops()[0].state,
            ReleaseQueueState::Failed {
                error: "boom".to_string()
            }
        );

        assert!(q.retry_failed());
        assert_eq!(q.ops()[0].state, ReleaseQueueState::Queued);
        assert!(!q.retry_failed());

        q.remove("rel-a");
        assert!(q.ops().is_empty());
    }

    #[test]
    fn cancel_queued_entry_drops_it_without_active_abort() {
        let q = ReleaseQueue::new();
        q.enqueue(op("rel-a"));
        assert!(!q.cancel("rel-a"));
        assert!(q.ops().is_empty());
    }

    #[test]
    fn pause_returns_prior_value() {
        let q: ReleaseQueue<(), u8> = ReleaseQueue::new();
        assert!(!q.set_paused(true));
        assert!(q.is_paused());
        assert!(q.set_paused(false));
        assert!(!q.is_paused());
    }

    /// A queued operation whose task panics must leave the release `Failed` with
    /// the panic in its message — a panicked task says so, rather than surfacing
    /// as whatever its progress channel looked like on the way down.
    #[tokio::test]
    async fn a_panicking_operation_marks_the_release_failed_with_the_panic() {
        let queue: Arc<ReleaseQueue<(), u8>> = Arc::new(ReleaseQueue::new());
        queue.enqueue(op("rel-a"));

        let worker_queue = Arc::clone(&queue);
        let worker = tokio::spawn(async move {
            run_serial_worker(
                &worker_queue,
                "Test",
                |_op| async move {
                    let task = tokio::spawn(async { panic!("boom") });
                    let abort = task.abort_handle();
                    let outcome = async move {
                        match task.await {
                            Ok(()) => Ok(()),
                            Err(join_error) if join_error.is_cancelled() => Ok(()),
                            Err(join_error) => Err(LibraryError::Storage(format!(
                                "task panicked: {join_error}"
                            ))),
                        }
                    };
                    Ok((0, RunningOp { abort, outcome }))
                },
                || {},
                |_, _| {},
            )
            .await;
        });

        let error = loop {
            if let Some(ReleaseQueueState::Failed { error }) =
                queue.ops().first().map(|o| o.state.clone())
            {
                break error;
            }
            tokio::task::yield_now().await;
        };
        worker.abort();

        assert!(
            error.contains("panicked"),
            "the failure names the panic, got: {error}"
        );
    }

    #[test]
    fn snapshot_counts_roll_up_to_total() {
        let mut active = op("rel-a");
        active.state = ReleaseQueueState::Active { progress: 42 };
        let queued = op("rel-b");
        let mut failed = op("rel-c");
        failed.state = ReleaseQueueState::Failed {
            error: "boom".to_string(),
        };

        let snap = build_release_queue_snapshot(&[active, queued, failed], false);
        assert_eq!(snap.total.active, 1);
        assert_eq!(snap.total.queued, 1);
        assert_eq!(snap.total.failed, 1);
        assert_eq!(snap.ops.len(), 3);
        assert_eq!(snap.ops[0].release_id, "rel-a");
    }
}
