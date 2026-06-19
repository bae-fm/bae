//! The in-memory download (pin) queue snapshot. Single source of truth for the
//! Storage Manager's Downloads pane and the per-release "Downloading…" badge
//! when a pin is queued or in flight.
//!
//! Unlike the cloud outbox (one row per file, persisted in the DB), the download
//! queue is per-RELEASE and entirely in-memory: a pin downloads a whole release,
//! and nothing survives a restart. So this snapshot is built from the queue's
//! in-memory state, not from any table — a not-fully-pinned release simply stays
//! cloud-only after a restart.
//!
//! Re-emitted on every queue mutation: enqueue, worker pick-up, per-file
//! progress, success, failure, cancel, retry, pause/resume. Consumers (the
//! Downloads pane, the inline storage-row badge) read this one snapshot rather
//! than holding their own cached counts.

/// What a queued download is doing right now. Per-release: a pin either waits,
/// downloads (with an overall percent across the release's files), or has
/// failed and stays in the queue for retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    /// Waiting for the serial worker, no failure recorded, not in flight.
    Queued,
    /// The release is downloading right now. `percent` is the overall
    /// release progress (combined across files), computed the same way
    /// `drive_transfer` folds per-file progress.
    Active { percent: u8 },
    /// The last attempt failed; the entry stays in the queue for retry.
    Failed { error: String },
}

/// One queued download — a whole release being pinned. Carries everything the
/// Downloads pane needs to render a row, resolved once at enqueue time from the
/// release's storage summary so the worker never re-queries for display data.
#[derive(Debug, Clone)]
pub struct DownloadOp {
    pub release_id: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the "queued 2m ago"
    /// relative label the UI renders.
    pub created_at: i64,
    pub state: DownloadState,
}

/// Per-state counts for the download queue, rolled up across all releases to
/// drive the pane header summary and the retry gate. No bytes: downloads track
/// an overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// Complete snapshot of the in-memory download queue. One source of truth for
/// everything pin-related the UI renders.
#[derive(Debug, Clone, Default)]
pub struct DownloadSnapshot {
    /// Queue order: the order downloads were enqueued, preserved so the pane
    /// renders them top-to-bottom the way the worker will process them.
    pub downloads: Vec<DownloadOp>,
    /// Sum across all downloads — drives the pane header summary and retry gate.
    pub total: DownloadProgress,
    /// True when the user paused the download queue. Drives the pane's
    /// pause/resume toggle; the worker waits while set.
    pub paused: bool,
}

/// Build the snapshot from the queue's ordered list of downloads and the
/// user-driven pause flag. Pure over its inputs: counts roll up from each
/// release's state.
pub fn build_download_snapshot(downloads: &[DownloadOp], paused: bool) -> DownloadSnapshot {
    let mut total = DownloadProgress::default();
    for op in downloads {
        match &op.state {
            DownloadState::Queued => total.queued += 1,
            DownloadState::Active { .. } => total.active += 1,
            DownloadState::Failed { .. } => total.failed += 1,
        }
    }

    DownloadSnapshot {
        downloads: downloads.to_vec(),
        total,
        paused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(release_id: &str, state: DownloadState) -> DownloadOp {
        DownloadOp {
            release_id: release_id.to_string(),
            title: "Test Album".to_string(),
            file_count: 3,
            total_size: 350_000_000,
            created_at: 0,
            state,
        }
    }

    #[test]
    fn snapshot_counts_roll_up_to_total() {
        let downloads = vec![
            op("rel-a", DownloadState::Active { percent: 42 }),
            op("rel-b", DownloadState::Queued),
            op(
                "rel-c",
                DownloadState::Failed {
                    error: "boom".to_string(),
                },
            ),
        ];
        let snap = build_download_snapshot(&downloads, false);

        assert_eq!(snap.total.active, 1);
        assert_eq!(snap.total.queued, 1);
        assert_eq!(snap.total.failed, 1);

        // Order preserved for the pane.
        assert_eq!(snap.downloads.len(), 3);
        assert_eq!(snap.downloads[0].release_id, "rel-a");
    }
}
