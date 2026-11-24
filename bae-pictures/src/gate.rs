//! How many reads of one volume run at once, and which waits first.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::oneshot;

/// Whether a request is for a picture a screen shows now, or one it may show
/// soon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Visible,
    Prefetch,
}

struct Waiting {
    visible: VecDeque<oneshot::Sender<()>>,
    prefetch: VecDeque<oneshot::Sender<()>>,
}

struct GateState {
    free: usize,
    waiting: Waiting,
}

/// The reads of one volume.
struct Gate {
    state: Mutex<GateState>,
}

impl Gate {
    fn new(reads_at_once: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(GateState {
                free: reads_at_once,
                waiting: Waiting { visible: VecDeque::new(), prefetch: VecDeque::new() },
            }),
        })
    }

    /// Returns once the request holds one of the volume's places.
    async fn wait_for_place(self: &Arc<Self>, priority: Priority) {
        loop {
            let Some(receiver) = self.take_or_queue(priority) else {
                return;
            };
            let mut pending = Pending { gate: self.clone(), receiver: Some(receiver) };
            if pending.wait().await {
                return;
            }
        }
    }

    /// Takes a place now and returns `None`, or queues the request and returns
    /// what its turn arrives on.
    fn take_or_queue(&self, priority: Priority) -> Option<oneshot::Receiver<()>> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // A prefetch never goes ahead of a visible request that waits.
        let may_go =
            state.free > 0 && (priority == Priority::Visible || state.waiting.visible.is_empty());
        if may_go {
            state.free -= 1;
            return None;
        }
        let (sender, receiver) = oneshot::channel();
        match priority {
            Priority::Visible => state.waiting.visible.push_back(sender),
            Priority::Prefetch => state.waiting.prefetch.push_back(sender),
        }
        Some(receiver)
    }

    /// Gives a place back: to the next request in line, visible ones first, or
    /// to the volume.
    fn release(&self) {
        loop {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let next =
                state.waiting.visible.pop_front().or_else(|| state.waiting.prefetch.pop_front());
            let Some(next) = next else {
                state.free += 1;
                return;
            };
            drop(state);
            // A request that stopped waiting dropped its receiver, and the
            // place goes to the next in line.
            if next.send(()).is_ok() {
                return;
            }
        }
    }
}

/// A request waiting for its turn. If it stops waiting after the place was
/// already handed to it, it gives the place back.
struct Pending {
    gate: Arc<Gate>,
    receiver: Option<oneshot::Receiver<()>>,
}

impl Pending {
    /// Whether the turn came; `false` when the queue let go of the request.
    async fn wait(&mut self) -> bool {
        let Some(receiver) = self.receiver.as_mut() else { return false };
        let arrived = receiver.await.is_ok();
        self.receiver = None;
        arrived
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(mut receiver) = self.receiver.take() {
            receiver.close();
            if receiver.try_recv().is_ok() {
                self.gate.release();
            }
        }
    }
}

/// One of a volume's places for a read, held until it is dropped.
pub struct Permit {
    gate: Arc<Gate>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.gate.release();
    }
}

/// The gates of the volumes originals are read from, one for each.
#[derive(Clone)]
pub struct Volumes {
    gates: Arc<Mutex<HashMap<Vec<u8>, Arc<Gate>>>>,
}

impl Volumes {
    pub fn new() -> Self {
        Self { gates: Arc::default() }
    }

    /// A place for one read of the volume named `volume`, which takes
    /// `reads_at_once` reads at once, waiting for one when all are taken:
    /// visible requests first. The first request for a volume sets how many
    /// it takes.
    pub async fn read_place(
        &self,
        volume: &[u8],
        reads_at_once: usize,
        priority: Priority,
    ) -> Permit {
        let gate = self
            .gates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(volume.to_vec())
            .or_insert_with(|| Gate::new(reads_at_once))
            .clone();
        gate.wait_for_place(priority).await;
        Permit { gate }
    }
}
