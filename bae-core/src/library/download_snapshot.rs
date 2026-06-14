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
    /// Pre-formatted total size, e.g. `"350 MB"`.
    pub size_label: String,
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
    /// Pre-formatted one-line summary, e.g. `"1 downloading · 2 queued"`.
    /// Empty when the queue is idle so the UI can hide the band.
    pub summary: String,
}

/// Build the snapshot from the queue's ordered list of downloads and the
/// user-driven pause flag. Pure over its inputs: counts roll up from each
/// release's state and `summary` is pre-formatted so the UI renders it verbatim.
pub fn build_download_snapshot(downloads: &[DownloadOp], paused: bool) -> DownloadSnapshot {
    let mut total = DownloadProgress::default();
    for op in downloads {
        match &op.state {
            DownloadState::Queued => total.queued += 1,
            DownloadState::Active { .. } => total.active += 1,
            DownloadState::Failed { .. } => total.failed += 1,
        }
    }

    let summary = format_summary(&total);

    DownloadSnapshot {
        downloads: downloads.to_vec(),
        total,
        paused,
        summary,
    }
}

/// One-line summary, e.g. `"1 downloading · 2 queued"` / `"1 failed"`. Empty
/// when the queue is idle so the UI can hide the band.
fn format_summary(total: &DownloadProgress) -> String {
    let mut parts = Vec::new();
    if total.active > 0 {
        parts.push(format!("{} downloading", total.active));
    }
    if total.failed > 0 {
        parts.push(format!("{} failed", total.failed));
    }
    if total.queued > 0 {
        parts.push(format!("{} queued", total.queued));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(release_id: &str, state: DownloadState) -> DownloadOp {
        DownloadOp {
            release_id: release_id.to_string(),
            title: "Test Album".to_string(),
            file_count: 3,
            size_label: "350 MB".to_string(),
            created_at: 0,
            state,
        }
    }

    #[test]
    fn summary_is_empty_when_idle() {
        assert_eq!(format_summary(&DownloadProgress::default()), "");
        assert_eq!(build_download_snapshot(&[], false).summary, "");
    }

    #[test]
    fn summary_lists_active_states_in_order() {
        assert_eq!(
            format_summary(&DownloadProgress {
                queued: 2,
                active: 1,
                failed: 1,
            }),
            "1 downloading · 1 failed · 2 queued"
        );
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
        assert_eq!(snap.summary, "1 downloading · 1 failed · 1 queued");

        // Order preserved for the pane.
        assert_eq!(snap.downloads.len(), 3);
        assert_eq!(snap.downloads[0].release_id, "rel-a");
    }
}
