//! In-memory serial queue for "Export…" (copy a release's files out to a folder).
//!
//! Exporting a release copies its whole file set — read verbatim through coven's
//! locality-aware read (a Remote release fetches from cloud/cache and decrypts) —
//! to a user-chosen directory. This queue orders those exports so one release
//! copies out at a time and the rest wait, drained by a single serial worker;
//! the user can pause, cancel, or retry.
//!
//! The queue is NOT persisted — on restart it's empty. Export changes no release
//! state: it only reads and writes to a user directory, leaving the release
//! Remote/Local exactly as it was. This type owns only the shared state and the
//! worker's wake signal; the worker loop and snapshot emission live on
//! `LibraryManager`, which has the coven read path and event bus the worker needs.

use std::sync::Mutex;

use tokio::sync::Notify;

use super::export_snapshot::{ExportOp, ExportState};

/// Shared, in-memory export-queue state plus the worker's wake signal.
///
/// `LibraryManager` holds one `Arc<ExportQueue>` (shared across its clones). The
/// state-mutation methods here are pure and synchronous (lock, mutate, drop);
/// the manager calls them, then builds a fresh snapshot from [`ExportQueue::ops`]
/// and emits it. `notify` wakes the single worker task after any change that
/// could give it work (a new enqueue, a resume, a retry).
pub struct ExportQueue {
    state: Mutex<State>,
    /// Wakes the serial worker when there may be work to pick up. The worker
    /// parks on this whenever the queue is paused or holds nothing `Queued`.
    notify: Notify,
}

struct State {
    /// Queue order, preserved for both processing and display: the worker takes
    /// the first `Queued` op; the pane renders the list top-to-bottom.
    ops: Vec<ExportOp>,
    /// User-driven pause flag. While set, the worker parks instead of picking up
    /// the next `Queued` op; the in-flight one runs to completion unless cancelled.
    paused: bool,
    /// `true` once the worker task has been spawned, so the first enqueue spawns
    /// it exactly once across all manager clones.
    worker_spawned: bool,
    /// Abort handle for the in-flight export task, set while a release is copying
    /// out. `cancel` aborts it; the worker clears it when the export settles.
    active_abort: Option<tokio::task::AbortHandle>,
}

impl ExportQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                ops: Vec::new(),
                paused: false,
                worker_spawned: false,
                active_abort: None,
            }),
            notify: Notify::new(),
        }
    }

    /// Wake the worker — called after any state change that could give it work.
    pub fn wake(&self) {
        self.notify.notify_one();
    }

    /// Park until the worker is woken. Used by the worker loop when the queue is
    /// paused or holds nothing `Queued`.
    pub async fn wait(&self) {
        self.notify.notified().await;
    }

    /// A clone of the current ordered op list, for building a snapshot.
    pub fn ops(&self) -> Vec<ExportOp> {
        self.state.lock().unwrap().ops.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    /// Mark the worker spawned, returning whether THIS call is the one that should
    /// spawn it. Lets the first enqueue across all manager clones spawn the single
    /// worker exactly once.
    pub fn claim_worker_spawn(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.worker_spawned {
            false
        } else {
            state.worker_spawned = true;
            true
        }
    }

    /// Enqueue a release as `Queued` if it isn't already in the queue. Returns
    /// `true` when it was added (so the caller knows to wake the worker). A
    /// release already `Queued`/`Active`/`Failed` here is left as-is — re-enqueue
    /// is a no-op, never a duplicate row or a reset of in-flight progress.
    pub fn enqueue(&self, op: ExportOp) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.ops.iter().any(|o| o.release_id == op.release_id) {
            return false;
        }
        state.ops.push(op);
        true
    }

    /// True when `release_id` is already in the queue in any state.
    pub fn contains(&self, release_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.release_id == release_id)
    }

    /// The next `Queued` op to process, or `None` when the queue is paused or
    /// holds nothing queued. Returns the op (which carries its `target_dir`) so
    /// the worker knows where to write. Does NOT flip the entry — the worker
    /// spawns the export task first, then calls [`activate`](Self::activate) to
    /// flip it and register the abort handle in one step. The pause check lives
    /// here (under the lock) so a pause set between picking and activating can't
    /// sneak a new export past it.
    pub fn next_queued(&self) -> Option<ExportOp> {
        let state = self.state.lock().unwrap();
        if state.paused {
            return None;
        }
        state
            .ops
            .iter()
            .find(|o| matches!(o.state, ExportState::Queued))
            .cloned()
    }

    /// Flip a still-`Queued` release to `Active { percent: 0 }` and register the
    /// in-flight export task's abort handle, both under one lock. Returns `false`
    /// when the entry is no longer `Queued` — a cancel removed it in the window
    /// between picking it and spawning its task, so the caller must abort the task
    /// it just spawned. Registering the handle atomically with the flip closes the
    /// race where a cancel sees `Active` but no handle yet.
    pub fn activate(&self, release_id: &str, abort: tokio::task::AbortHandle) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(op) = state
            .ops
            .iter_mut()
            .find(|o| o.release_id == release_id && matches!(o.state, ExportState::Queued))
        else {
            return false;
        };
        op.state = ExportState::Active { percent: 0 };
        state.active_abort = Some(abort);
        true
    }

    /// Update the in-flight percent for the active release. No-op if the release
    /// is no longer in the queue (cancelled mid-export).
    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
        let mut state = self.state.lock().unwrap();
        if let Some(op) = state.ops.iter_mut().find(|o| o.release_id == release_id) {
            op.state = ExportState::Active { percent };
        }
    }

    /// Remove a release from the queue (on successful export or on cancel of a
    /// queued/failed entry).
    pub fn remove(&self, release_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.ops.retain(|o| o.release_id != release_id);
    }

    /// Mark a release `Failed`, keeping it in the queue for retry. No-op if it's
    /// no longer present (cancelled before the failure landed).
    pub fn mark_failed(&self, release_id: &str, error: String) {
        let mut state = self.state.lock().unwrap();
        if let Some(op) = state.ops.iter_mut().find(|o| o.release_id == release_id) {
            op.state = ExportState::Failed { error };
        }
    }

    /// Flip every `Failed` entry back to `Queued` so the worker retries them.
    /// Returns whether any were flipped (so the caller wakes the worker).
    pub fn retry_failed(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut any = false;
        for op in state.ops.iter_mut() {
            if matches!(op.state, ExportState::Failed { .. }) {
                op.state = ExportState::Queued;
                any = true;
            }
        }
        any
    }

    /// Set the paused flag. Returns the previous value so the caller can tell a
    /// resume (false now, true before) from a redundant set.
    pub fn set_paused(&self, paused: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        std::mem::replace(&mut state.paused, paused)
    }

    /// Clear the active export's abort handle once its export settles. The handle
    /// is set atomically with the `Active` flip by [`activate`](Self::activate).
    pub fn clear_active_abort(&self) {
        self.state.lock().unwrap().active_abort = None;
    }

    /// Cancel a release's export. If it's the active one, abort the in-flight
    /// export task and clear it; either way drop the queue entry. Returns whether
    /// the active task was aborted. The in-flight export writes into a staging
    /// directory and renames it into place only after all files succeed; aborting
    /// the task drops its future, whose staging-dir guard removes the partial
    /// output, so no files ever appear at the final export path.
    pub fn cancel(&self, release_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_active = state
            .ops
            .iter()
            .any(|o| o.release_id == release_id && matches!(o.state, ExportState::Active { .. }));
        if was_active {
            if let Some(abort) = state.active_abort.take() {
                abort.abort();
            }
        }
        state.ops.retain(|o| o.release_id != release_id);
        was_active
    }
}

impl Default for ExportQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn op(release_id: &str) -> ExportOp {
        ExportOp {
            release_id: release_id.to_string(),
            target_dir: PathBuf::from("/tmp/exports"),
            title: "Album Title".to_string(),
            file_count: 3,
            total_size: 350_000_000,
            created_at: 0,
            state: ExportState::Queued,
        }
    }

    #[test]
    fn enqueue_dedups_by_release_id() {
        let q = ExportQueue::new();
        assert!(q.enqueue(op("rel-a")));
        // Same release, second enqueue: rejected, no duplicate row.
        assert!(!q.enqueue(op("rel-a")));
        assert!(q.contains("rel-a"));
        assert_eq!(q.ops().len(), 1);

        assert!(q.enqueue(op("rel-b")));
        assert_eq!(q.ops().len(), 2);
    }

    /// An aborted-immediately handle, valid for `activate` in tests.
    fn dummy_abort() -> tokio::task::AbortHandle {
        tokio::spawn(std::future::ready(())).abort_handle()
    }

    #[tokio::test]
    async fn next_queued_picks_head_and_respects_pause() {
        let q = ExportQueue::new();
        q.enqueue(op("rel-a"));
        q.enqueue(op("rel-b"));

        // Picking doesn't flip; activating does (and registers the handle).
        assert_eq!(
            q.next_queued().map(|o| o.release_id).as_deref(),
            Some("rel-a")
        );
        assert!(q.activate("rel-a", dummy_abort()));
        let ops = q.ops();
        assert_eq!(ops[0].state, ExportState::Active { percent: 0 });
        assert_eq!(ops[1].state, ExportState::Queued);

        // The next still-queued is rel-b; while paused, nothing is handed out.
        assert_eq!(
            q.next_queued().map(|o| o.release_id).as_deref(),
            Some("rel-b")
        );
        q.set_paused(true);
        assert!(q.next_queued().is_none());
    }

    #[tokio::test]
    async fn activate_fails_when_entry_cancelled_in_the_gap() {
        let q = ExportQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(
            q.next_queued().map(|o| o.release_id).as_deref(),
            Some("rel-a")
        );
        // A cancel removed it between picking and activating: activate refuses, so
        // the worker knows to abort the task it just spawned.
        q.remove("rel-a");
        assert!(!q.activate("rel-a", dummy_abort()));
    }

    #[tokio::test]
    async fn percent_remove_fail_retry_transitions() {
        let q = ExportQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(
            q.next_queued().map(|o| o.release_id).as_deref(),
            Some("rel-a")
        );
        assert!(q.activate("rel-a", dummy_abort()));

        q.set_active_percent("rel-a", 50);
        assert_eq!(q.ops()[0].state, ExportState::Active { percent: 50 });

        q.mark_failed("rel-a", "boom".to_string());
        assert_eq!(
            q.ops()[0].state,
            ExportState::Failed {
                error: "boom".to_string()
            }
        );

        // Retry flips Failed back to Queued so the worker re-picks it.
        assert!(q.retry_failed());
        assert_eq!(q.ops()[0].state, ExportState::Queued);
        // Nothing failed now: retry reports no change.
        assert!(!q.retry_failed());

        q.remove("rel-a");
        assert!(q.ops().is_empty());
    }

    #[test]
    fn cancel_queued_entry_drops_it_without_active_abort() {
        let q = ExportQueue::new();
        q.enqueue(op("rel-a"));
        // Cancelling a still-queued entry isn't an active abort.
        assert!(!q.cancel("rel-a"));
        assert!(q.ops().is_empty());
    }

    #[test]
    fn pause_returns_prior_value() {
        let q = ExportQueue::new();
        assert!(!q.set_paused(true)); // was false
        assert!(q.is_paused());
        assert!(q.set_paused(false)); // was true
        assert!(!q.is_paused());
    }

    #[test]
    fn worker_spawn_claimed_exactly_once() {
        let q = ExportQueue::new();
        assert!(q.claim_worker_spawn());
        assert!(!q.claim_worker_spawn());
    }
}
