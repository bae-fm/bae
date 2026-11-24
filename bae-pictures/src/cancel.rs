//! The flag that ends a picture job between two reads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared by a job and whoever may end it. The job's reader checks it before
/// every read of the original.
#[derive(Debug, Clone)]
pub struct CancelFlag(Arc<AtomicBool>);

/// Asks whether the job that holds it should stop: what a reader checks before
/// each read, and all a job sees of the flag that ends it.
pub type IsCancelled = Arc<dyn Fn() -> bool + Send + Sync>;

impl CancelFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// A question that reads this flag.
    pub fn watcher(&self) -> IsCancelled {
        let flag = self.clone();
        Arc::new(move || flag.is_cancelled())
    }
}
