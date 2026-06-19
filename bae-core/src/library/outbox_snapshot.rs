//! The cloud-outbox processing snapshot. Single source of truth for the
//! Storage Manager's queue panel, per-release upload badges, and (in later
//! phases) the master progress bar.
//!
//! Derived from the `cloud_outbox` rows plus the in-memory map of uploads that
//! are in flight right now (an upload is "active" only between coven's
//! `on_blob_upload_started` and its terminal callback — never persisted, since
//! nothing is in flight after a restart). The map's value is the live
//! `bytes_done` for that file, advanced by coven's mid-upload progress callback
//! so the per-release and aggregate bars move within a single large file.
//!
//! Re-emitted on every queue mutation: enqueue, upload start, success,
//! failure, cancel, retry. Consumers (release-row badges, queue panel,
//! aggregate progress) all read from this one snapshot rather than holding
//! their own cached counts.

use std::collections::HashMap;

use crate::db::Database;
use crate::library::upload_throughput::UploadThroughput;

use crate::db::OutboxOpKind;

/// What an upload is doing right now. Derived from the row plus the in-flight
/// map; never stored.
///
/// `bytes_done` on `Active` is the live count of encrypted bytes that have
/// reached the cloud for this file so far, fed by coven's mid-upload progress
/// callback. It's 0 the instant an upload starts and climbs to `bytes_total`
/// as the file transfers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadState {
    /// Queued, no failure recorded, not in flight.
    Queued,
    /// An upload attempt is in flight right now.
    Active { bytes_done: u64 },
    /// The last attempt failed; the entry stays queued for retry.
    Failed { last_error: String },
}

/// One upload operation. Carries everything the UI needs to render a queue row
/// and everything the snapshot needs to compute per-release and aggregate
/// progress.
#[derive(Debug, Clone)]
pub struct UploadOp {
    pub id: i64,
    pub file_id: String,
    /// Owning release. `None` for an orphaned file_id (release deleted but
    /// outbox entry not yet drained).
    pub release_id: Option<String>,
    /// Album title for display. `None` for an orphaned file_id.
    pub title: Option<String>,
    pub cloud_key: String,
    pub bytes_total: u64,
    /// Pre-formatted file size, e.g. `"70 MB"`. The UI renders this beside
    /// the title so users see how much is being shipped.
    pub size_label: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub attempt_count: i64,
    pub state: UploadState,
}

/// One pending cloud delete. Deletes have no progress concept — they're a
/// single DELETE call per entry.
#[derive(Debug, Clone)]
pub struct DeleteOp {
    pub id: i64,
    pub file_id: String,
    pub cloud_key: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// Upload progress as the UI cares about it: per-state counts plus
/// bytes-done/bytes-total. Used both per-release (for storage-row badges) and
/// as the overall total (for the master progress bar).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl UploadProgress {
    /// True when the queue holds nothing for this release / overall.
    pub fn is_idle(&self) -> bool {
        self.queued == 0 && self.active == 0 && self.failed == 0
    }

    /// Total queued + active + failed (i.e. anything not yet shipped).
    pub fn pending(&self) -> u32 {
        self.queued + self.active + self.failed
    }
}

/// Complete snapshot of the cloud outbox. One source of truth for everything
/// upload-related the UI renders.
#[derive(Debug, Clone, Default)]
pub struct OutboxSnapshot {
    pub uploads: Vec<UploadOp>,
    pub deletes: Vec<DeleteOp>,
    /// Per-release aggregate, keyed by release id. Drives the "Uploading (N)"
    /// badge on each storage row and gates per-release storage actions.
    /// Releases with no pending work are absent from the map.
    pub per_release: HashMap<String, UploadProgress>,
    /// Sum across all uploads — drives the master progress bar.
    pub total: UploadProgress,
    pub pending_deletes: u32,
    /// True when the user has paused the upload pipeline. Drives the
    /// pause/resume toggle in the Storage Manager's bottom panel and
    /// suppresses the throughput display.
    pub paused: bool,
    /// Rolling-window upload throughput in bytes per second. Zero when the
    /// queue is idle or has been idle long enough for the window to drain. The
    /// UI formats it as a localized rate; aggregate bytes come from `total`.
    pub throughput_bps: u64,
    /// Estimated seconds remaining at the current rate. `None` when throughput
    /// is zero or no bytes remain. The UI formats it.
    pub eta_seconds: Option<u64>,
}

/// Build the snapshot from the outbox rows, the in-flight upload map (file_id →
/// live `bytes_done`), the rolling-window throughput tracker, and the
/// user-driven pause flag.
pub(crate) async fn build_outbox_snapshot(
    db: &Database,
    in_flight: &HashMap<String, u64>,
    throughput: &UploadThroughput,
    paused: bool,
) -> Result<OutboxSnapshot, coven::database::DbError> {
    let rows = db.outbox_items().await?;

    let mut uploads = Vec::new();
    let mut deletes = Vec::new();
    let mut per_release: HashMap<String, UploadProgress> = HashMap::new();
    let mut total = UploadProgress::default();

    for row in rows {
        match row.operation {
            OutboxOpKind::Upload => {
                let bytes_total = row.file_size.unwrap_or(0) as u64;
                let state = if let Some(&live) = in_flight.get(&row.file_id) {
                    // The reported count is of the encrypted payload, which can
                    // edge just past the stored plaintext size; clamp so the
                    // bar never exceeds 100% or skews the ETA math.
                    UploadState::Active {
                        bytes_done: live.min(bytes_total),
                    }
                } else if let Some(last_error) = row.last_error.clone() {
                    UploadState::Failed { last_error }
                } else {
                    UploadState::Queued
                };
                let bytes_done = match &state {
                    UploadState::Active { bytes_done } => *bytes_done,
                    UploadState::Queued | UploadState::Failed { .. } => 0,
                };

                total.bytes_total += bytes_total;
                total.bytes_done += bytes_done;
                match state {
                    UploadState::Queued => total.queued += 1,
                    UploadState::Active { .. } => total.active += 1,
                    UploadState::Failed { .. } => total.failed += 1,
                }

                if let Some(rid) = row.release_id.clone() {
                    let prog = per_release.entry(rid).or_default();
                    prog.bytes_total += bytes_total;
                    prog.bytes_done += bytes_done;
                    match &state {
                        UploadState::Queued => prog.queued += 1,
                        UploadState::Active { .. } => prog.active += 1,
                        UploadState::Failed { .. } => prog.failed += 1,
                    }
                }

                let size_label = crate::util::format::format_bytes(bytes_total);
                uploads.push(UploadOp {
                    id: row.id,
                    file_id: row.file_id,
                    release_id: row.release_id,
                    title: row.title,
                    cloud_key: row.cloud_key,
                    bytes_total,
                    size_label,
                    created_at: row.created_at,
                    attempt_count: row.attempt_count,
                    state,
                });
            }
            OutboxOpKind::Delete => {
                deletes.push(DeleteOp {
                    id: row.id,
                    file_id: row.file_id,
                    cloud_key: row.cloud_key,
                    created_at: row.created_at,
                });
            }
        }
    }

    let pending_deletes = deletes.len() as u32;

    // When paused, hide throughput/ETA — uploads aren't flowing so the rolling
    // window decays toward zero anyway, and rendering "2.3 MB/s" beside a
    // paused indicator would be confusing.
    let throughput_bps = if paused {
        0
    } else {
        throughput.bytes_per_sec()
    };
    let bytes_remaining = total.bytes_total.saturating_sub(total.bytes_done);
    let eta_seconds = if paused || throughput_bps == 0 || bytes_remaining == 0 {
        None
    } else {
        Some(bytes_remaining / throughput_bps)
    };
    Ok(OutboxSnapshot {
        uploads,
        deletes,
        per_release,
        total,
        pending_deletes,
        paused,
        throughput_bps,
        eta_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(queued: u32, active: u32, failed: u32) -> UploadProgress {
        UploadProgress {
            queued,
            active,
            failed,
            bytes_done: 0,
            bytes_total: 0,
        }
    }

    #[test]
    fn progress_is_idle_only_when_all_counts_zero() {
        assert!(progress(0, 0, 0).is_idle());
        assert!(!progress(1, 0, 0).is_idle());
        assert!(!progress(0, 1, 0).is_idle());
        assert!(!progress(0, 0, 1).is_idle());
    }
}
