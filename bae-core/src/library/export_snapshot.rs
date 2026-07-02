//! The in-memory export queue snapshot. Single source of truth for the Storage
//! Manager's Exporting pane while a release is being copied out to a user
//! directory.
//!
//! Like the download (pin) queue, the export queue is per-RELEASE and entirely
//! in-memory: an export copies a whole release's files verbatim to a chosen
//! folder, and nothing survives a restart. So this snapshot is built from the
//! queue's in-memory state, not from any table.
//!
//! Re-emitted on every queue mutation: enqueue, worker pick-up, per-file
//! progress, success, failure, cancel, retry, pause/resume. The Exporting pane
//! reads this one snapshot rather than holding its own cached counts.

use std::path::PathBuf;

/// What a queued export is doing right now. Per-release: an export either waits,
/// copies out (with an overall percent across the release's files), or has
/// failed and stays in the queue for retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportState {
    /// Waiting for the export worker, no failure recorded, not in flight.
    Queued,
    /// The release is copying out right now. `percent` is the overall release
    /// progress (combined across files, folded from the per-file index).
    Active { percent: u8 },
    /// The last attempt failed; the entry stays in the queue for retry.
    Failed { error: String },
}

/// One queued export — a whole release being copied out verbatim to
/// `target_dir`. Carries everything the Exporting pane needs to render a row,
/// resolved once at enqueue time from the release's storage summary so the
/// worker never re-queries for display data.
#[derive(Debug, Clone)]
pub struct ExportOp {
    pub release_id: String,
    /// Where this export writes: the release's source folder is reconstructed
    /// under `<target_dir>/<source_folder_name>/`.
    pub target_dir: PathBuf,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the "queued 2m ago"
    /// relative label the UI renders.
    pub created_at: i64,
    pub state: ExportState,
}

/// Per-state counts for the export queue, rolled up across all releases to drive
/// the pane header summary and the retry gate. No bytes: exports track an
/// overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// Complete snapshot of the in-memory export queue. One source of truth for
/// everything export-related the UI renders.
#[derive(Debug, Clone, Default)]
pub struct ExportSnapshot {
    /// Queue order: the order exports were enqueued, preserved so the pane
    /// renders them top-to-bottom the way the worker will process them.
    pub exports: Vec<ExportOp>,
    /// Sum across all exports — drives the pane header summary and retry gate.
    pub total: ExportProgress,
    /// True when the user paused the export queue. Drives the pane's
    /// pause/resume toggle; the worker waits while set.
    pub paused: bool,
}

/// Build the snapshot from the queue's ordered list of exports and the
/// user-driven pause flag. Pure over its inputs: counts roll up from each
/// release's state.
pub fn build_export_snapshot(exports: &[ExportOp], paused: bool) -> ExportSnapshot {
    let mut total = ExportProgress::default();
    for op in exports {
        match &op.state {
            ExportState::Queued => total.queued += 1,
            ExportState::Active { .. } => total.active += 1,
            ExportState::Failed { .. } => total.failed += 1,
        }
    }

    ExportSnapshot {
        exports: exports.to_vec(),
        total,
        paused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(release_id: &str, state: ExportState) -> ExportOp {
        ExportOp {
            release_id: release_id.to_string(),
            target_dir: PathBuf::from("/tmp/exports"),
            title: "Album Title".to_string(),
            file_count: 3,
            total_size: 350_000_000,
            created_at: 0,
            state,
        }
    }

    #[test]
    fn snapshot_counts_roll_up_to_total() {
        let exports = vec![
            op("rel-a", ExportState::Active { percent: 42 }),
            op("rel-b", ExportState::Queued),
            op(
                "rel-c",
                ExportState::Failed {
                    error: "boom".to_string(),
                },
            ),
        ];
        let snap = build_export_snapshot(&exports, false);

        assert_eq!(snap.total.active, 1);
        assert_eq!(snap.total.queued, 1);
        assert_eq!(snap.total.failed, 1);

        // Order preserved for the pane.
        assert_eq!(snap.exports.len(), 3);
        assert_eq!(snap.exports[0].release_id, "rel-a");
    }
}
