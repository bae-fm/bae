//! In-memory serial queue for "Pin for offline".
//!
//! Pinning a release downloads its whole file set from the cloud. This queue
//! orders those pins so one release downloads at a time and the rest wait,
//! drained by a single serial worker; the user can pause, cancel, or retry.
//!
//! The queue is NOT persisted — on restart it's empty, and any release that
//! wasn't fully pinned stays cloud-only (a pin flips a release to "pinned" only
//! after every file lands; see `transfer::do_pin`). This type owns only the
//! shared state and the worker's wake signal; the worker loop and snapshot
//! emission live on `LibraryManager`, which has the `TransferService` and event
//! bus the worker needs.

use std::sync::Mutex;

use tokio::sync::Notify;

use super::download_snapshot::{DownloadOp, DownloadState};

/// Shared, in-memory download-queue state plus the worker's wake signal.
///
/// `LibraryManager` holds one `Arc<DownloadQueue>` (shared across its clones).
/// The state-mutation methods here are pure and synchronous (lock, mutate,
/// drop); the manager calls them, then builds a fresh snapshot from
/// [`DownloadQueue::ops`] and emits it. `notify` wakes the single worker task
/// after any change that could give it work (a new enqueue, a resume, a retry).
pub struct DownloadQueue {
    state: Mutex<State>,
    /// Wakes the serial worker when there may be work to pick up. The worker
    /// parks on this whenever the queue is paused or holds nothing `Queued`.
    notify: Notify,
}

struct State {
    /// Queue order, preserved for both processing and display: the worker takes
    /// the first `Queued` op; the pane renders the list top-to-bottom.
    ops: Vec<DownloadOp>,
    /// User-driven pause flag. While set, the worker parks instead of picking
    /// up the next `Queued` op; in-flight downloads are unaffected (the active
    /// one runs to completion unless cancelled).
    paused: bool,
    /// `true` once the worker task has been spawned, so the first enqueue
    /// spawns it exactly once across all manager clones.
    worker_spawned: bool,
    /// Abort handle for the in-flight pin task, set while a release is
    /// downloading. `cancel_active` aborts it; the worker clears it when the
    /// download settles.
    active_abort: Option<tokio::task::AbortHandle>,
}

impl DownloadQueue {
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
    pub fn ops(&self) -> Vec<DownloadOp> {
        self.state.lock().unwrap().ops.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    /// Mark the worker spawned, returning whether THIS call is the one that
    /// should spawn it. Lets the first enqueue across all manager clones spawn
    /// the single worker exactly once.
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
    pub fn enqueue(&self, op: DownloadOp) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.ops.iter().any(|o| o.release_id == op.release_id) {
            return false;
        }
        state.ops.push(op);
        true
    }

    /// True when `release_id` is already in the queue in any state. Lets the
    /// caller skip the storage-summary lookup for a release it would dedup.
    pub fn contains(&self, release_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.release_id == release_id)
    }

    /// The next `Queued` release to process, or `None` when the queue is paused
    /// or holds nothing queued. Does NOT flip the entry — the worker spawns the
    /// pin task first, then calls [`activate`](Self::activate) to flip it and
    /// register the abort handle in one step. The pause check lives here (under
    /// the lock) so a pause set between picking and activating can't sneak a new
    /// download past it.
    pub fn next_queued_release(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        if state.paused {
            return None;
        }
        state
            .ops
            .iter()
            .find(|o| matches!(o.state, DownloadState::Queued))
            .map(|o| o.release_id.clone())
    }

    /// Flip a still-`Queued` release to `Active { percent: 0 }` and register the
    /// in-flight pin task's abort handle, both under one lock. Returns `false`
    /// when the entry is no longer `Queued` — a cancel removed it in the window
    /// between picking it and spawning its task, so the caller must abort the
    /// task it just spawned. Registering the handle atomically with the flip
    /// closes the race where a cancel sees `Active` but no handle yet.
    pub fn activate(&self, release_id: &str, abort: tokio::task::AbortHandle) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(op) = state
            .ops
            .iter_mut()
            .find(|o| o.release_id == release_id && matches!(o.state, DownloadState::Queued))
        else {
            return false;
        };
        op.state = DownloadState::Active { percent: 0 };
        state.active_abort = Some(abort);
        true
    }

    /// Update the in-flight percent for the active release. No-op if the release
    /// is no longer in the queue (cancelled mid-download).
    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
        let mut state = self.state.lock().unwrap();
        if let Some(op) = state.ops.iter_mut().find(|o| o.release_id == release_id) {
            op.state = DownloadState::Active { percent };
        }
    }

    /// Remove a release from the queue (on successful pin or on cancel of a
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
            op.state = DownloadState::Failed { error };
        }
    }

    /// Flip every `Failed` entry back to `Queued` so the worker retries them.
    /// Returns whether any were flipped (so the caller wakes the worker).
    pub fn retry_failed(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut any = false;
        for op in state.ops.iter_mut() {
            if matches!(op.state, DownloadState::Failed { .. }) {
                op.state = DownloadState::Queued;
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

    /// Clear the active download's abort handle once its pin settles. The handle
    /// is set atomically with the `Active` flip by [`activate`](Self::activate).
    pub fn clear_active_abort(&self) {
        self.state.lock().unwrap().active_abort = None;
    }

    /// Cancel a release's download. If it's the active one, abort the in-flight
    /// pin task and clear it; either way drop the queue entry. Returns whether
    /// the active task was aborted (so the worker's drain can be told to stop
    /// emitting for it — though it removes the entry regardless).
    pub fn cancel(&self, release_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_active = state
            .ops
            .iter()
            .any(|o| o.release_id == release_id && matches!(o.state, DownloadState::Active { .. }));
        if was_active {
            if let Some(abort) = state.active_abort.take() {
                abort.abort();
            }
        }
        state.ops.retain(|o| o.release_id != release_id);
        was_active
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(release_id: &str) -> DownloadOp {
        DownloadOp {
            release_id: release_id.to_string(),
            title: "Test Album".to_string(),
            file_count: 3,
            total_size: 350_000_000,
            created_at: 0,
            state: DownloadState::Queued,
        }
    }

    #[test]
    fn enqueue_dedups_by_release_id() {
        let q = DownloadQueue::new();
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
        let q = DownloadQueue::new();
        q.enqueue(op("rel-a"));
        q.enqueue(op("rel-b"));

        // Picking doesn't flip; activating does (and registers the handle).
        assert_eq!(q.next_queued_release().as_deref(), Some("rel-a"));
        assert!(q.activate("rel-a", dummy_abort()));
        let ops = q.ops();
        assert_eq!(ops[0].state, DownloadState::Active { percent: 0 });
        assert_eq!(ops[1].state, DownloadState::Queued);

        // The next still-queued is rel-b; while paused, nothing is handed out.
        assert_eq!(q.next_queued_release().as_deref(), Some("rel-b"));
        q.set_paused(true);
        assert_eq!(q.next_queued_release(), None);
    }

    #[tokio::test]
    async fn activate_fails_when_entry_cancelled_in_the_gap() {
        let q = DownloadQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(q.next_queued_release().as_deref(), Some("rel-a"));
        // A cancel removed it between picking and activating: activate refuses,
        // so the worker knows to abort the task it just spawned.
        q.remove("rel-a");
        assert!(!q.activate("rel-a", dummy_abort()));
    }

    #[tokio::test]
    async fn percent_remove_fail_retry_transitions() {
        let q = DownloadQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(q.next_queued_release().as_deref(), Some("rel-a"));
        assert!(q.activate("rel-a", dummy_abort()));

        q.set_active_percent("rel-a", 50);
        assert_eq!(q.ops()[0].state, DownloadState::Active { percent: 50 });

        q.mark_failed("rel-a", "boom".to_string());
        assert_eq!(
            q.ops()[0].state,
            DownloadState::Failed {
                error: "boom".to_string()
            }
        );

        // Retry flips Failed back to Queued so the worker re-picks it.
        assert!(q.retry_failed());
        assert_eq!(q.ops()[0].state, DownloadState::Queued);
        // Nothing failed now: retry reports no change.
        assert!(!q.retry_failed());

        q.remove("rel-a");
        assert!(q.ops().is_empty());
    }

    #[test]
    fn cancel_queued_entry_drops_it_without_active_abort() {
        let q = DownloadQueue::new();
        q.enqueue(op("rel-a"));
        // Cancelling a still-queued entry isn't an active abort.
        assert!(!q.cancel("rel-a"));
        assert!(q.ops().is_empty());
    }

    #[test]
    fn pause_returns_prior_value() {
        let q = DownloadQueue::new();
        assert!(!q.set_paused(true)); // was false
        assert!(q.is_paused());
        assert!(q.set_paused(false)); // was true
        assert!(!q.is_paused());
    }

    #[test]
    fn worker_spawn_claimed_exactly_once() {
        let q = DownloadQueue::new();
        assert!(q.claim_worker_spawn());
        assert!(!q.claim_worker_spawn());
    }
}
