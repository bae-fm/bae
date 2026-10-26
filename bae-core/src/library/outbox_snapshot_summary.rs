//! Read-only release and summary projections over the cloud-outbox queue.

use std::collections::HashMap;

use super::outbox_snapshot::{OutboxQueue, UploadProgress};
use super::release_queue::CountLabel;

impl OutboxQueue {
    pub fn transitioning_release_ids(&self) -> Vec<String> {
        self.upload_groups
            .iter()
            .map(|group| group.release_id.clone())
            .collect()
    }

    pub fn per_release_progress(&self) -> HashMap<String, UploadProgress> {
        self.upload_groups
            .iter()
            .map(|group| (group.release_id.clone(), group.progress.clone()))
            .collect()
    }

    /// The summary line's phase counts in dominance order. Each zero count drops
    /// out; platforms only localize and join.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        let mut parts = Vec::new();
        for (key, count) in [
            ("core.queue.uploading", self.total.uploading),
            ("core.queue.queued", self.total.queued),
        ] {
            if count > 0 {
                parts.push(CountLabel {
                    key: key.to_string(),
                    count,
                });
            }
        }
        parts
    }
}
