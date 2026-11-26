//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    QueuePauses,
    r#"
    SELECT q.queue, q.paused FROM release_queues AS q
"#
);

impl QueuePauses {
    pub fn queue(&self) -> &str {
        &self.queue
    }
    pub fn paused(&self) -> i64 {
        self.paused
    }
}
