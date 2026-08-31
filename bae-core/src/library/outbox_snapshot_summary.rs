//! Read-only release and summary projections over a cloud-outbox snapshot.

use std::collections::HashMap;

use super::outbox_snapshot::{OutboxSnapshot, ReleaseUploadProgress};
use super::release_queue::CountLabel;

impl OutboxSnapshot {
    pub fn transitioning_release_ids(&self) -> Vec<String> {
        self.upload_groups
            .iter()
            .map(|group| group.release_id.clone())
            .collect()
    }

    pub fn per_release_progress(&self) -> HashMap<String, ReleaseUploadProgress> {
        self.upload_groups
            .iter()
            .map(|group| {
                (
                    group.release_id.clone(),
                    ReleaseUploadProgress {
                        progress: group.progress.clone(),
                        throughput_bps: group.throughput_bps,
                    },
                )
            })
            .collect()
    }

    pub fn pending_delete_count(&self) -> u32 {
        u32::try_from(self.deletes.len()).expect("pending delete count exceeds u32")
    }

    /// The summary line's phase counts in dominance order, followed by pending
    /// deletes. Each zero count drops out; platforms only localize and join.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        let mut parts = Vec::new();
        for (key, count) in [
            ("core.outbox.cancelling", self.total.cancelling),
            ("core.outbox.publishing", self.total.publishing),
            ("core.queue.uploading", self.total.uploading),
            ("core.outbox.preparing", self.total.preparing),
            ("core.outbox.retrying", self.total.retrying),
            ("core.outbox.prepared", self.total.prepared),
            ("core.queue.queued", self.total.queued),
            ("core.outbox.uploaded", self.total.uploaded),
        ] {
            if count > 0 {
                parts.push(CountLabel {
                    key: key.to_string(),
                    count,
                });
            }
        }
        let pending_deletes = self.pending_delete_count();
        if pending_deletes > 0 {
            parts.push(CountLabel {
                key: "core.outbox.pending_deletes".to_string(),
                count: pending_deletes,
            });
        }
        parts
    }
}
