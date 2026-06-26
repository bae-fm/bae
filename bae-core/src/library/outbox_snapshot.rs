//! The cloud-outbox processing snapshot. Single source of truth for the
//! Storage Manager's queue panel, per-release upload badges, and the master
//! progress bar.
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

use tracing::debug;

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
    /// Album title. `None` for an orphaned file_id. Consumed by the Windows FFI
    /// (`bae-windows-ffi`), whose upload rows label by album; the macOS/iOS/
    /// Android bridge instead renders `display_name`.
    pub title: Option<String>,
    /// What to show as the row's primary label: the file's original on-disk
    /// name, or the cloud key when the backing `release_files` row is gone
    /// (orphaned file_id). Always populated so the UI renders it directly.
    pub display_name: String,
    pub cloud_key: String,
    pub bytes_total: u64,
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

/// The dominant activity of a slice of the upload queue (a release's pending
/// uploads, or the whole queue), for the storage-row badge. A slice with any
/// file uploading reads as `Uploading`; with none uploading but some failed and
/// awaiting retry, `Retrying`; otherwise `Queued`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Retrying,
    Queued,
}

/// Upload progress as the UI cares about it: per-state counts plus
/// bytes-done/bytes-total. Used both per-release (for storage-row badges) and
/// as the overall total (for queue counts, ETA, and the summary band).
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

    /// The badge activity for this slice: active uploads outrank failures
    /// awaiting retry, which outrank items still only queued. `None` when idle.
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
}

/// A release's pending uploads, grouped for the queue pane so it renders one row
/// per release (matching the storage table) instead of a flat per-file list.
/// `release_id` is `None` for the orphaned-files bucket (files whose backing
/// release is gone). `display_title` is what the row labels itself with — the
/// album title, or the file's own name for an orphan — resolved here so the UI
/// renders it directly. `progress` and `file_count` are the group's aggregates.
#[derive(Debug, Clone)]
pub struct UploadReleaseGroup {
    pub release_id: Option<String>,
    pub display_title: String,
    pub file_count: u32,
    pub progress: UploadProgress,
}

/// Complete snapshot of the cloud outbox. One source of truth for everything
/// upload-related the UI renders.
#[derive(Debug, Clone, Default)]
pub struct OutboxSnapshot {
    pub uploads: Vec<UploadOp>,
    /// `uploads` grouped by release, in first-seen order — the per-release rows
    /// the queue pane renders.
    pub upload_groups: Vec<UploadReleaseGroup>,
    pub deletes: Vec<DeleteOp>,
    /// Per-release aggregate, keyed by release id. Drives the "Uploading (N)"
    /// badge on each storage row and gates per-release storage actions.
    /// Releases with no pending work are absent from the map.
    pub per_release: HashMap<String, UploadProgress>,
    /// Sum across all uploads — drives the queue counts, ETA, and summary band.
    pub total: UploadProgress,
    /// Total bytes of the files uploading *right now* (Active state). The master
    /// progress bar shows `total.bytes_done` of this — the live transfer —
    /// rather than progress against the whole backlog, which would crawl as
    /// completed files drain from the queue and shrink both numerator and
    /// denominator.
    pub active_bytes_total: u64,
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
    let mut active_bytes_total = 0u64;

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
                    UploadState::Active { .. } => {
                        total.active += 1;
                        active_bytes_total += bytes_total;
                    }
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

                let cloud_key = row.cloud_key;
                let display_name = match row.file_name {
                    Some(name) => name,
                    None => {
                        debug!(
                            outbox_id = row.id,
                            "orphaned outbox upload (no backing file row); \
                             showing cloud key as the label"
                        );
                        cloud_key.clone()
                    }
                };
                uploads.push(UploadOp {
                    id: row.id,
                    file_id: row.file_id,
                    release_id: row.release_id,
                    title: row.title,
                    display_name,
                    cloud_key,
                    bytes_total,
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

    // Group the flat uploads by release (first-seen order) for the queue pane's
    // per-release rows. The orphaned-files bucket (release_id `None`) groups
    // together. Each group's `progress` is accumulated from its own files.
    let mut upload_groups: Vec<UploadReleaseGroup> = Vec::new();
    let mut group_index: HashMap<Option<String>, usize> = HashMap::new();
    for op in &uploads {
        let idx = *group_index.entry(op.release_id.clone()).or_insert_with(|| {
            // The album title, or the file's own label for an orphan (a release
            // deleted mid-upload, so the row has no album to name it by).
            let display_title = match &op.title {
                Some(title) => title.clone(),
                None => {
                    debug!(
                        file_id = %op.file_id,
                        "outbox upload group has no album title (orphaned); labelling by file name"
                    );
                    op.display_name.clone()
                }
            };
            upload_groups.push(UploadReleaseGroup {
                release_id: op.release_id.clone(),
                display_title,
                file_count: 0,
                progress: UploadProgress::default(),
            });
            upload_groups.len() - 1
        });
        let group = &mut upload_groups[idx];
        group.file_count += 1;
        group.progress.bytes_total += op.bytes_total;
        match &op.state {
            UploadState::Queued => group.progress.queued += 1,
            UploadState::Active { bytes_done } => {
                group.progress.active += 1;
                group.progress.bytes_done += bytes_done;
            }
            UploadState::Failed { .. } => group.progress.failed += 1,
        }
    }

    Ok(OutboxSnapshot {
        uploads,
        upload_groups,
        deletes,
        per_release,
        total,
        active_bytes_total,
        pending_deletes,
        paused,
        throughput_bps,
        eta_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::SystemClock;
    use crate::db::{
        Database, DbAlbum, DbArtist, DbFile, DbRelease, Pressing, ReleaseMetadataSource,
    };
    use crate::util::content_type::ContentType;
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::TempDir;

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

    #[test]
    fn activity_ranks_active_over_failed_over_queued() {
        assert_eq!(progress(0, 0, 0).activity(), None);
        assert_eq!(progress(3, 0, 0).activity(), Some(UploadActivity::Queued));
        assert_eq!(progress(0, 0, 2).activity(), Some(UploadActivity::Retrying));
        // Failures awaiting retry outrank items still only queued.
        assert_eq!(progress(5, 0, 2).activity(), Some(UploadActivity::Retrying));
        // Any active upload wins outright.
        assert_eq!(
            progress(5, 1, 2).activity(),
            Some(UploadActivity::Uploading)
        );
    }

    /// Seed a release with two files (sizes 100 and 1000) and queue both for
    /// upload. Returns the db, the small file id, the large file id, and the
    /// temp-dir guard (drop ends the test db).
    async fn seed_two_queued_uploads() -> (Database, String, String, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db = Database::new_test(
            tmp.path().join("test.db").to_str().unwrap(),
            Arc::new(SystemClock),
        )
        .await
        .unwrap();

        db.insert_artist(&DbArtist {
            id: "artist-1".into(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        db.insert_album(&DbAlbum {
            id: "album-1".into(),
            title: "Album Title".into(),
            artist_id: "artist-1".into(),
            year: None,
            primary_release_id: None,
            is_compilation: false,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        db.insert_release(&DbRelease {
            id: "rel-1".into(),
            album_id: "album-1".into(),
            release_name: None,
            pressing: Pressing {
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: true,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();

        for (id, name, size) in [
            ("file-small", "01 Track Title.flac", 100i64),
            ("file-large", "02 Track Title.flac", 1000i64),
        ] {
            db.insert_file(&DbFile {
                id: id.into(),
                release_id: "rel-1".into(),
                original_filename: name.into(),
                file_size: size,
                content_type: ContentType::Flac,
                cloud_path: None,
                created_at: Utc::now(),
            })
            .await
            .unwrap();
            db.add_cloud_outbox_upload(id, &format!("storage/{id}"), None, false)
                .await
                .unwrap();
        }

        (db, "file-small".into(), "file-large".into(), tmp)
    }

    #[tokio::test]
    async fn snapshot_labels_rows_with_per_file_names() {
        let (db, _small, _large, _tmp) = seed_two_queued_uploads().await;
        let snapshot = build_outbox_snapshot(&db, &HashMap::new(), &UploadThroughput::new(), false)
            .await
            .unwrap();

        let mut names: Vec<_> = snapshot
            .uploads
            .iter()
            .map(|u| u.display_name.clone())
            .collect();
        names.sort();
        assert_eq!(names, ["01 Track Title.flac", "02 Track Title.flac"]);
    }

    #[tokio::test]
    async fn upload_groups_group_a_releases_files_with_aggregate_progress() {
        let (db, _small, _large, _tmp) = seed_two_queued_uploads().await;
        let snapshot = build_outbox_snapshot(&db, &HashMap::new(), &UploadThroughput::new(), false)
            .await
            .unwrap();

        // The release's two files collapse to one group carrying both.
        assert_eq!(snapshot.upload_groups.len(), 1);
        let group = &snapshot.upload_groups[0];
        assert_eq!(group.release_id.as_deref(), Some("rel-1"));
        assert_eq!(group.display_title, "Album Title");
        assert_eq!(group.file_count, 2);
        // Aggregate progress: both queued, summed bytes (100 + 1000).
        assert_eq!(group.progress.queued, 2);
        assert_eq!(group.progress.active, 0);
        assert_eq!(group.progress.bytes_total, 1100);
    }

    #[tokio::test]
    async fn active_bytes_total_counts_only_the_uploading_file() {
        let (db, _small, large, _tmp) = seed_two_queued_uploads().await;

        // The large file is uploading right now (250 of 1000 bytes done); the
        // small file is still queued.
        let in_flight = HashMap::from([(large, 250u64)]);
        let snapshot = build_outbox_snapshot(&db, &in_flight, &UploadThroughput::new(), false)
            .await
            .unwrap();

        // The whole-queue total spans both files; the master bar's denominator
        // is only the file actually transferring.
        assert_eq!(snapshot.total.bytes_total, 1100);
        assert_eq!(snapshot.active_bytes_total, 1000);
        // bytes_done — the bar's numerator — is the in-flight file's progress.
        assert_eq!(snapshot.total.bytes_done, 250);
        assert_eq!(snapshot.total.active, 1);
        assert_eq!(snapshot.total.queued, 1);
    }
}
