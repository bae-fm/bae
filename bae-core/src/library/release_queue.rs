//! Shared in-memory serial queue for per-release background work.
//!
//! Pinning and exporting both process one release at a time, keep transient
//! in-memory rows for the queue pane, and support pause, cancel, and retry.
//! `Extra` carries the operation-specific row payload: downloads use `()`,
//! exports use their target directory.

use std::sync::Mutex;

use tokio::sync::Notify;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseQueueState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct ReleaseQueueOp<Extra> {
    pub release_id: String,
    pub title: String,
    pub file_count: i64,
    pub total_size: i64,
    pub created_at: i64,
    pub payload: Extra,
    pub state: ReleaseQueueState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ReleaseQueueSnapshot<Extra> {
    pub ops: Vec<ReleaseQueueOp<Extra>>,
    pub total: ReleaseQueueProgress,
    pub paused: bool,
}

pub fn build_release_queue_snapshot<Extra: Clone>(
    ops: &[ReleaseQueueOp<Extra>],
    paused: bool,
) -> ReleaseQueueSnapshot<Extra> {
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

pub struct ReleaseQueue<Extra> {
    state: Mutex<State<Extra>>,
    notify: Notify,
}

struct State<Extra> {
    ops: Vec<ReleaseQueueOp<Extra>>,
    paused: bool,
    worker_spawned: bool,
    active_abort: Option<tokio::task::AbortHandle>,
}

impl<Extra: Clone> ReleaseQueue<Extra> {
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

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    pub async fn wait(&self) {
        self.notify.notified().await;
    }

    pub fn ops(&self) -> Vec<ReleaseQueueOp<Extra>> {
        self.state.lock().unwrap().ops.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    pub fn claim_worker_spawn(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.worker_spawned {
            false
        } else {
            state.worker_spawned = true;
            true
        }
    }

    pub fn enqueue(&self, op: ReleaseQueueOp<Extra>) -> bool {
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

    pub fn next_queued(&self) -> Option<ReleaseQueueOp<Extra>> {
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

    pub fn activate(&self, release_id: &str, abort: tokio::task::AbortHandle) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(op) = state
            .ops
            .iter_mut()
            .find(|o| o.release_id == release_id && matches!(o.state, ReleaseQueueState::Queued))
        else {
            return false;
        };
        op.state = ReleaseQueueState::Active { percent: 0 };
        state.active_abort = Some(abort);
        true
    }

    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
        let mut state = self.state.lock().unwrap();
        if let Some(op) = state.ops.iter_mut().find(|o| o.release_id == release_id) {
            op.state = ReleaseQueueState::Active { percent };
        }
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

impl<Extra: Clone> Default for ReleaseQueue<Extra> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(release_id: &str) -> ReleaseQueueOp<()> {
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
        assert!(q.activate("rel-a", dummy_abort()));
        let ops = q.ops();
        assert_eq!(ops[0].state, ReleaseQueueState::Active { percent: 0 });
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
        assert!(!q.activate("rel-a", dummy_abort()));
    }

    #[tokio::test]
    async fn percent_remove_fail_retry_transitions() {
        let q = ReleaseQueue::new();
        q.enqueue(op("rel-a"));
        assert_eq!(q.next_queued().map(|o| o.release_id), Some("rel-a".into()));
        assert!(q.activate("rel-a", dummy_abort()));

        q.set_active_percent("rel-a", 50);
        assert_eq!(q.ops()[0].state, ReleaseQueueState::Active { percent: 50 });

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
        let q: ReleaseQueue<()> = ReleaseQueue::new();
        assert!(!q.set_paused(true));
        assert!(q.is_paused());
        assert!(q.set_paused(false));
        assert!(!q.is_paused());
    }

    #[test]
    fn worker_spawn_claimed_exactly_once() {
        let q: ReleaseQueue<()> = ReleaseQueue::new();
        assert!(q.claim_worker_spawn());
        assert!(!q.claim_worker_spawn());
    }

    #[test]
    fn snapshot_counts_roll_up_to_total() {
        let mut active = op("rel-a");
        active.state = ReleaseQueueState::Active { percent: 42 };
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
