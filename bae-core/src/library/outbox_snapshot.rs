//! The cloud-outbox processing snapshot: the one source of truth for the Storage
//! Manager's queue panel, the per-release upload badges, and the master progress
//! bar. Re-emitted on every queue mutation (enqueue, upload start, progress tick,
//! success, failure, cancel, retry), so no consumer keeps cached counts of its own.
//!
//! Three inputs derive it:
//!
//! - coven's durable cloud queue, read through
//!   [`Database::outbox_queue`](crate::db::Database::outbox_queue): what remains.
//! - An in-memory map of the uploads in flight right now — an upload is "active"
//!   only between coven's `on_blob_upload_started` and its terminal callback, and
//!   the map is never persisted, since nothing is in flight after a restart. Its
//!   value is that file's live `bytes_done`, advanced by coven's mid-upload
//!   progress callback, so the per-file and aggregate bars move within one large
//!   file.
//! - The [`UploadSessions`] tally of files already completed in this burst. It
//!   keeps finished files in every fraction's numerator *and* denominator, so
//!   progress climbs monotonically instead of resetting as completed rows drain
//!   out of the table.

use std::collections::{HashMap, HashSet};

use tracing::debug;

use crate::db::DbOutboxQueue;
use crate::library::upload_sessions::UploadSessions;
use crate::library::upload_throughput::UploadThroughput;

/// What an upload is doing right now — derived from the row, the in-flight map,
/// and the completed tally; never stored. `Active`'s `bytes_done` is the live count
/// of encrypted bytes that have reached the cloud for this file, 0 the instant the
/// upload starts and climbing to `bytes_total` as it transfers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadState {
    /// Queued, no failure recorded, not in flight, not yet uploaded.
    Queued,
    /// An upload attempt is in flight right now.
    Active { bytes_done: u64 },
    /// The last attempt failed; the entry stays queued for retry.
    Failed { last_error: String },
    /// The bytes are in the cloud. Either the file's outbox row is already
    /// gone, or it lingers only until coven's post-upload commit removes it.
    Done,
}

/// One cloud object still owed a removal. Deletes have no progress concept —
/// they're a single DELETE call per entry.
///
/// The row that named the object is already gone, so the blob's namespace and
/// id are all there is to identify it by; there is no filename or album to
/// show. Together they are the entry's identity for the UI's list diffing.
#[derive(Debug, Clone)]
pub struct DeleteOp {
    pub namespace: String,
    pub blob_id: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// The dominant activity of a slice of the upload queue (one release's uploads, or
/// the whole queue), for the storage-row badge. Any file uploading reads as
/// `Uploading`; none uploading but some failed and awaiting retry, `Retrying`;
/// otherwise `Queued`. There is no terminal variant — a release with nothing left
/// to ship stops being rendered at all: its group leaves the snapshot and its
/// storage row falls back to the resting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Retrying,
    Queued,
}

/// Upload progress as the UI cares about it: per-state counts plus
/// bytes-done/bytes-total. Serves both a single release (its badge and bar) and the
/// overall total (queue counts, ETA, master bar, summary band). Completed files
/// count in `done`, `bytes_done`, and `bytes_total`, so fractions are cumulative
/// over the whole burst.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub done: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl UploadProgress {
    /// True when this slice still has unshipped work — queued, in flight, or
    /// failed awaiting retry. Completed files don't count: a slice that's all
    /// done has nothing left to render or wait for.
    pub fn has_pending(&self) -> bool {
        self.queued > 0 || self.active > 0 || self.failed > 0
    }

    /// The badge activity for this slice: active uploads outrank failures
    /// awaiting retry, which outrank items still only queued. `None` when
    /// nothing is pending.
    pub fn activity(&self) -> Option<UploadActivity> {
        if self.active > 0 {
            Some(UploadActivity::Uploading)
        } else if self.failed > 0 {
            Some(UploadActivity::Retrying)
        } else if self.queued > 0 {
            Some(UploadActivity::Queued)
        } else {
            None
        }
    }

    fn add_upload(&mut self, state: &UploadState, bytes_total: u64) {
        self.bytes_total += bytes_total;
        match state {
            UploadState::Queued => self.queued += 1,
            UploadState::Active { bytes_done } => {
                self.active += 1;
                self.bytes_done += bytes_done;
            }
            UploadState::Failed { .. } => self.failed += 1,
            UploadState::Done => {
                self.done += 1;
                self.bytes_done += bytes_total;
            }
        }
    }

    fn add_progress(&mut self, progress: &UploadProgress) {
        self.queued += progress.queued;
        self.active += progress.active;
        self.failed += progress.failed;
        self.done += progress.done;
        self.bytes_done += progress.bytes_done;
        self.bytes_total += progress.bytes_total;
    }
}

/// One file in a release's upload group: what the queue pane's per-file rows
/// render. `bytes_total` is the file's stored size; the live `bytes_done`
/// while uploading rides in the `Active` state.
#[derive(Debug, Clone)]
pub struct UploadFileOp {
    pub file_id: String,
    /// The file's original name, or its cloud key / file id when no
    /// release-file row backs it.
    pub display_name: String,
    pub bytes_total: u64,
    pub state: UploadState,
}

/// A release's uploads, grouped so the queue pane renders one expandable row per
/// release (matching the storage table) with the files inside. `release_id` is
/// `None` for the orphaned-files bucket, whose backing release is gone.
/// `display_title` is resolved here, so the UI renders it directly: the album
/// title, or an orphan's first file name. `files` runs completed files first, in
/// completion order, then the rest in queue order.
#[derive(Debug, Clone)]
pub struct UploadReleaseGroup {
    pub release_id: Option<String>,
    pub display_title: String,
    pub files: Vec<UploadFileOp>,
    pub progress: UploadProgress,
}

/// Complete snapshot of the cloud outbox. One source of truth for everything
/// upload-related the UI renders.
#[derive(Debug, Clone, Default)]
pub struct OutboxSnapshot {
    /// Uploads grouped by release — the rows the queue pane renders. Only
    /// groups with unshipped work appear: a release whose files all completed
    /// leaves the snapshot (and the pane hides once nothing pending remains).
    pub upload_groups: Vec<UploadReleaseGroup>,
    pub deletes: Vec<DeleteOp>,
    /// Sum across all uploads — drives the queue counts, ETA, the master
    /// progress bar, and the summary band.
    pub total: UploadProgress,
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

impl OutboxSnapshot {
    pub fn uploading_release_ids(&self) -> Vec<String> {
        self.upload_groups
            .iter()
            .filter_map(|group| group.release_id.clone())
            .collect()
    }

    pub fn per_release_progress(&self) -> HashMap<String, UploadProgress> {
        self.upload_groups
            .iter()
            .filter_map(|group| {
                group
                    .release_id
                    .clone()
                    .map(|release_id| (release_id, group.progress.clone()))
            })
            .collect()
    }

    pub fn pending_delete_count(&self) -> u32 {
        self.deletes.len() as u32
    }

    /// The summary line's parts: uploading, then failed, then queued, then any
    /// pending deletes — each dropped when zero. The order and drop rule are the
    /// decision, made once here rather than in each app's storage band.
    pub fn summary_parts(&self) -> Vec<crate::library::release_queue::CountLabel> {
        use crate::library::release_queue::CountLabel;
        let mut parts = crate::library::release_queue::ReleaseQueueProgress {
            queued: self.total.queued,
            active: self.total.active,
            failed: self.total.failed,
        }
        .summary_parts("core.queue.uploading");
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

/// A group under construction: `display_title` stays unresolved (`None`)
/// until a row supplies the album title or the batch lookup fills it in.
struct GroupBuilder {
    release_id: Option<String>,
    display_title: Option<String>,
    files: Vec<UploadFileOp>,
    progress: UploadProgress,
}

impl GroupBuilder {
    fn new(release_id: Option<String>) -> Self {
        Self {
            release_id,
            display_title: None,
            files: Vec::new(),
            progress: UploadProgress::default(),
        }
    }

    fn push(&mut self, file: UploadFileOp) {
        self.progress.add_upload(&file.state, file.bytes_total);
        self.files.push(file);
    }
}

/// Build the snapshot from coven's queue, the in-flight upload map (file_id →
/// live `bytes_done`), the completed-upload tallies for this burst, the
/// rolling-window throughput tracker, and the user-driven pause flag.
///
/// A pure derivation over already-read state: everything it needs about what is
/// queued arrives in `queue`, so it neither reads the database nor fails.
///
/// When the queue is observed fully idle — nothing queued, nothing in flight —
/// the burst is over: the tallies are cleared here, in the one derivation every
/// mutation path funnels through, so stale done-rows cannot survive it.
pub(crate) fn build_outbox_snapshot(
    queue: DbOutboxQueue,
    in_flight: &HashMap<String, u64>,
    sessions: &UploadSessions,
    throughput: &UploadThroughput,
    paused: bool,
) -> OutboxSnapshot {
    if queue.uploads.is_empty() && queue.deletes.is_empty() && in_flight.is_empty() {
        sessions.clear_all();
        return OutboxSnapshot {
            paused,
            ..Default::default()
        };
    }

    let deletes: Vec<DeleteOp> = queue
        .deletes
        .into_iter()
        .map(|delete| DeleteOp {
            namespace: delete.namespace,
            blob_id: delete.blob_id,
            created_at: delete.created_at,
        })
        .collect();
    let mut groups: Vec<GroupBuilder> = Vec::new();
    let mut group_index: HashMap<Option<String>, usize> = HashMap::new();
    let mut done_ids: HashSet<String> = HashSet::new();

    // Completed files first: they anchor their groups in completion order, so
    // a release that finished stays at the top while later ones drain.
    for (release_id, done_files) in sessions.tallies() {
        let mut group = GroupBuilder::new(release_id.clone());
        for file in done_files {
            done_ids.insert(file.file_id.clone());
            group.push(UploadFileOp {
                file_id: file.file_id,
                display_name: file.display_name,
                bytes_total: file.bytes,
                state: UploadState::Done,
            });
        }
        group_index.insert(release_id, groups.len());
        groups.push(group);
    }

    for upload in queue.uploads {
        // Already tallied as done — the entry just lingers until coven's
        // post-upload commit consumes it. Deriving it too would double-count the
        // file, and deriving it as queued would announce a completed upload as
        // fresh work.
        if done_ids.contains(&upload.file_id) {
            continue;
        }
        let bytes_total = upload.file_size.unwrap_or(0) as u64;
        let state = if let Some(&live) = in_flight.get(&upload.file_id) {
            // The reported count is of the encrypted payload, which can edge
            // just past the stored plaintext size; clamp it so the bar never
            // exceeds 100% or skews the ETA math.
            UploadState::Active {
                bytes_done: live.min(bytes_total),
            }
        } else if let Some(last_error) = upload.last_error {
            UploadState::Failed { last_error }
        } else {
            UploadState::Queued
        };
        let display_name = match upload.file_name {
            Some(name) => name,
            None => {
                debug!(
                    file_id = upload.file_id,
                    "queued upload has no backing release_files row; \
                     showing its file id as the label"
                );
                upload.file_id.clone()
            }
        };
        let idx = *group_index
            .entry(upload.release_id.clone())
            .or_insert_with(|| {
                groups.push(GroupBuilder::new(upload.release_id.clone()));
                groups.len() - 1
            });
        let group = &mut groups[idx];
        if group.display_title.is_none() {
            group.display_title = upload.album_title;
        }
        group.push(UploadFileOp {
            file_id: upload.file_id,
            display_name,
            bytes_total,
            state,
        });
    }

    let upload_groups: Vec<UploadReleaseGroup> = groups
        .into_iter()
        // A release with nothing left to ship stops being rendered: its group leaves
        // the snapshot, its storage row falls back to the resting state, and the
        // pane hides once no group or delete remains. The observer drops the tally
        // that fed its done files when the root completes; the idle clear above is
        // the backstop.
        .filter(|group| group.progress.has_pending())
        .map(|group| {
            // Every rendered group has a pending row, so the album title normally
            // arrives on the row join. The fallback labels the orphaned bucket (no
            // backing release) by its first file, as the per-file case above does.
            let display_title = group.display_title.unwrap_or_else(|| {
                debug!(
                    release_id = ?group.release_id,
                    "outbox upload group has no album title; labelling by file name"
                );
                group
                    .files
                    .first()
                    .map(|f| f.display_name.clone())
                    .unwrap_or_default()
            });
            UploadReleaseGroup {
                release_id: group.release_id,
                display_title,
                files: group.files,
                progress: group.progress,
            }
        })
        .collect();

    let total = upload_groups
        .iter()
        .fold(UploadProgress::default(), |mut total, group| {
            total.add_progress(&group.progress);
            total
        });

    // Hide throughput/ETA while paused: the rolling window decays toward zero
    // anyway, and "2.3 MB/s" beside a paused indicator would just confuse.
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

    OutboxSnapshot {
        upload_groups,
        deletes,
        total,
        paused,
        throughput_bps,
        eta_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DbOutboxDelete, DbOutboxUpload};
    use crate::library::upload_sessions::DoneFile;

    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const SMALL_FILE: &str = "00415c7f-b363-4ed9-8aad-422b93e974e9";
    const LARGE_FILE: &str = "357d9eb4-a021-4555-8713-0bc652d83c65";
    const OTHER_RELEASE: &str = "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b";
    const OTHER_FILE: &str = "36ebe9b3-749f-4638-82b2-57cba256ff68";

    fn progress(queued: u32, active: u32, failed: u32, done: u32) -> UploadProgress {
        UploadProgress {
            queued,
            active,
            failed,
            done,
            bytes_done: 0,
            bytes_total: 0,
        }
    }

    #[test]
    fn activity_ranks_active_over_failed_over_queued() {
        assert_eq!(progress(0, 0, 0, 0).activity(), None);
        assert_eq!(
            progress(3, 0, 0, 0).activity(),
            Some(UploadActivity::Queued)
        );
        assert_eq!(
            progress(0, 0, 2, 0).activity(),
            Some(UploadActivity::Retrying)
        );
        // Failures awaiting retry outrank items still only queued.
        assert_eq!(
            progress(5, 0, 2, 0).activity(),
            Some(UploadActivity::Retrying)
        );
        // Any active upload wins outright.
        assert_eq!(
            progress(5, 1, 2, 0).activity(),
            Some(UploadActivity::Uploading)
        );
        // Completed files never produce a badge of their own: a slice that's
        // all done is idle (the group stops being rendered entirely).
        assert_eq!(progress(0, 0, 0, 4).activity(), None);
        assert_eq!(
            progress(1, 0, 0, 4).activity(),
            Some(UploadActivity::Queued)
        );
    }

    /// One release with two queued uploads (100 and 1000 bytes), as coven's
    /// queue plus bae's context report them.
    fn two_queued_uploads() -> DbOutboxQueue {
        DbOutboxQueue {
            uploads: vec![
                queued_upload(SMALL_FILE, "01 Track Title.flac", 100),
                queued_upload(LARGE_FILE, "02 Track Title.flac", 1000),
            ],
            deletes: Vec::new(),
        }
    }

    fn queued_upload(file_id: &str, file_name: &str, file_size: i64) -> DbOutboxUpload {
        DbOutboxUpload {
            release_id: Some(RELEASE.to_string()),
            file_id: file_id.to_string(),
            attempt_count: 0,
            last_error: None,
            created_at: 1_700_000_000_000,
            file_name: Some(file_name.to_string()),
            file_size: Some(file_size),
            album_title: Some("Album Title".to_string()),
        }
    }

    fn build(
        queue: DbOutboxQueue,
        in_flight: &HashMap<String, u64>,
        sessions: &UploadSessions,
    ) -> OutboxSnapshot {
        build_outbox_snapshot(queue, in_flight, sessions, &UploadThroughput::new(), false)
    }

    #[test]
    fn upload_groups_group_a_releases_files_with_aggregate_progress() {
        let snapshot = build(
            two_queued_uploads(),
            &HashMap::new(),
            &UploadSessions::new(),
        );

        // The release's two files collapse to one group carrying both.
        assert_eq!(snapshot.upload_groups.len(), 1);
        let group = &snapshot.upload_groups[0];
        assert_eq!(group.release_id.as_deref(), Some(RELEASE));
        assert_eq!(group.display_title, "Album Title");
        assert_eq!(group.files.len(), 2);
        assert_eq!(group.files[0].display_name, "01 Track Title.flac");
        assert_eq!(group.files[0].state, UploadState::Queued);
        // Aggregate progress: both queued, summed bytes (100 + 1000).
        assert_eq!(group.progress.queued, 2);
        assert_eq!(group.progress.active, 0);
        assert_eq!(group.progress.bytes_total, 1100);
    }

    #[test]
    fn live_bytes_ride_the_active_file_and_the_totals() {
        // The large file is uploading right now (250 of 1000 bytes done); the
        // small file is still queued.
        let in_flight = HashMap::from([(LARGE_FILE.to_string(), 250u64)]);
        let snapshot = build(two_queued_uploads(), &in_flight, &UploadSessions::new());

        assert_eq!(snapshot.total.bytes_total, 1100);
        // bytes_done is the in-flight file's live progress.
        assert_eq!(snapshot.total.bytes_done, 250);
        assert_eq!(snapshot.total.active, 1);
        assert_eq!(snapshot.total.queued, 1);
        let group = &snapshot.upload_groups[0];
        let active = group
            .files
            .iter()
            .find(|f| f.file_id == LARGE_FILE)
            .expect("active file listed");
        assert_eq!(active.state, UploadState::Active { bytes_done: 250 });
    }

    /// A queue entry coven has recorded a failed attempt on derives as `Failed`,
    /// so the release badge reads "Retrying" rather than "Queued".
    #[test]
    fn a_recorded_failure_derives_failed_with_its_error() {
        let mut queue = two_queued_uploads();
        queue.uploads[1].attempt_count = 1;
        queue.uploads[1].last_error = Some("boom".to_string());

        let snapshot = build(queue, &HashMap::new(), &UploadSessions::new());

        assert_eq!(snapshot.total.failed, 1);
        assert_eq!(snapshot.total.queued, 1);
        assert_eq!(
            snapshot.total.activity(),
            Some(UploadActivity::Retrying),
            "a failure awaiting retry outranks the file still only queued"
        );
        let failed = snapshot.upload_groups[0]
            .files
            .iter()
            .find(|f| f.file_id == LARGE_FILE)
            .expect("failed file listed");
        assert_eq!(
            failed.state,
            UploadState::Failed {
                last_error: "boom".to_string()
            }
        );
    }

    /// An upload whose `release_files` row is gone still holds its place in the
    /// queue — labelled by its file id, since there is no filename left.
    #[test]
    fn an_upload_with_no_backing_row_is_labelled_by_its_file_id() {
        let queue = DbOutboxQueue {
            uploads: vec![DbOutboxUpload {
                file_name: None,
                file_size: None,
                album_title: None,
                ..queued_upload(SMALL_FILE, "unused", 0)
            }],
            deletes: Vec::new(),
        };

        let snapshot = build(queue, &HashMap::new(), &UploadSessions::new());

        let group = &snapshot.upload_groups[0];
        assert_eq!(group.files[0].display_name, SMALL_FILE);
        assert_eq!(group.files[0].bytes_total, 0);
        assert_eq!(
            group.display_title, SMALL_FILE,
            "with no album title the group falls back to its first file's label"
        );
    }

    /// Pending tombstones carry into the snapshot and into the summary line even
    /// when nothing is uploading — the queue pane still has work to show.
    #[test]
    fn pending_deletes_survive_an_otherwise_empty_queue() {
        let queue = DbOutboxQueue {
            uploads: Vec::new(),
            deletes: vec![DbOutboxDelete {
                namespace: "release_files".to_string(),
                blob_id: SMALL_FILE.to_string(),
                created_at: 1_700_000_000_000,
            }],
        };

        let snapshot = build(queue, &HashMap::new(), &UploadSessions::new());

        assert_eq!(snapshot.pending_delete_count(), 1);
        assert_eq!(snapshot.deletes[0].namespace, "release_files");
        assert_eq!(snapshot.deletes[0].blob_id, SMALL_FILE);
        assert!(snapshot.upload_groups.is_empty());
        assert_eq!(
            snapshot
                .summary_parts()
                .iter()
                .map(|part| part.key.as_str())
                .collect::<Vec<_>>(),
            vec!["core.outbox.pending_deletes"]
        );
    }

    /// The frozen-"Queued (1)" regression at the derivation level: a file the
    /// tally records as done while its queue entry still lingers must derive as
    /// `Done` — kept in the cumulative bytes, absent from the queued count,
    /// represented exactly once.
    #[test]
    fn tallied_file_with_lingering_entry_derives_done() {
        let sessions = UploadSessions::new();
        sessions.record_done(
            Some(RELEASE.to_string()),
            DoneFile {
                file_id: SMALL_FILE.to_string(),
                display_name: "01 Track Title.flac".into(),
                bytes: 100,
            },
        );
        let snapshot = build(two_queued_uploads(), &HashMap::new(), &sessions);

        let group = &snapshot.upload_groups[0];
        assert_eq!(group.files.len(), 2, "done file represented exactly once");
        let done = group
            .files
            .iter()
            .find(|f| f.file_id == SMALL_FILE)
            .expect("done file listed");
        assert_eq!(done.state, UploadState::Done);
        assert_eq!(group.progress.done, 1);
        assert_eq!(group.progress.queued, 1);
        // Cumulative: the completed bytes stay in numerator and denominator.
        assert_eq!(group.progress.bytes_done, 100);
        assert_eq!(group.progress.bytes_total, 1100);
    }

    /// A release with nothing left to ship stops being rendered: its group
    /// leaves the snapshot while other releases keep uploading, and the totals
    /// cover only the work still on screen.
    #[test]
    fn fully_done_group_is_dropped_while_queue_busy() {
        // Both of this release's files completed and their queue entries are
        // consumed; a second release still has a queued upload keeping the queue
        // busy.
        let queue = DbOutboxQueue {
            uploads: vec![DbOutboxUpload {
                release_id: Some(OTHER_RELEASE.to_string()),
                ..queued_upload(OTHER_FILE, "03 Track Title.flac", 500)
            }],
            deletes: Vec::new(),
        };

        let sessions = UploadSessions::new();
        for (id, bytes) in [(SMALL_FILE, 100u64), (LARGE_FILE, 1000u64)] {
            sessions.record_done(
                Some(RELEASE.to_string()),
                DoneFile {
                    file_id: id.to_string(),
                    display_name: id.to_string(),
                    bytes,
                },
            );
        }
        let snapshot = build(queue, &HashMap::new(), &sessions);

        assert_eq!(snapshot.upload_groups.len(), 1);
        let group = &snapshot.upload_groups[0];
        assert_eq!(group.release_id.as_deref(), Some(OTHER_RELEASE));
        assert_eq!(snapshot.total.bytes_total, 500);
        assert_eq!(snapshot.total.queued, 1);
        assert!(
            !snapshot.per_release_progress().contains_key(RELEASE),
            "a finished release must fall back to its resting storage badge"
        );
    }

    /// An idle queue ends the burst: the tallies clear and the snapshot is
    /// empty, so no stale done-rows can outlive it.
    #[test]
    fn idle_queue_clears_the_tallies() {
        let sessions = UploadSessions::new();
        sessions.record_done(
            Some(RELEASE.to_string()),
            DoneFile {
                file_id: SMALL_FILE.to_string(),
                display_name: "01 Track Title.flac".into(),
                bytes: 100,
            },
        );
        let snapshot = build(DbOutboxQueue::default(), &HashMap::new(), &sessions);

        assert!(snapshot.upload_groups.is_empty());
        assert!(sessions.tallies().is_empty());
    }
}
