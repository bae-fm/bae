//! The cloud-outbox processing snapshot. Single source of truth for the
//! Storage Manager's queue panel, per-release upload badges, and the master
//! progress bar.
//!
//! Derived from three inputs: the `cloud_outbox` rows (what remains), the
//! in-memory map of uploads in flight right now (an upload is "active" only
//! between coven's `on_blob_upload_started` and its terminal callback — never
//! persisted, since nothing is in flight after a restart), and the
//! [`UploadSessions`] tally of files that already completed during this queue
//! burst. The in-flight map's value is the live `bytes_done` for that file,
//! advanced by coven's mid-upload progress callback so the per-file and
//! aggregate bars move within a single large file. The completed tally keeps
//! finished files in every fraction's numerator *and* denominator, so
//! per-release and whole-queue progress climb monotonically instead of
//! resetting as completed rows drain out of the table.
//!
//! Re-emitted on every queue mutation: enqueue, upload start, progress tick,
//! success, failure, cancel, retry. Consumers (release-row badges, queue
//! panel, aggregate progress) all read from this one snapshot rather than
//! holding their own cached counts.

use std::collections::{HashMap, HashSet};

use tracing::debug;

use crate::db::Database;
use crate::library::upload_sessions::UploadSessions;
use crate::library::upload_throughput::UploadThroughput;

use crate::db::DbOutboxOperation;

/// What an upload is doing right now. Derived from the row, the in-flight
/// map, and the completed tally; never stored.
///
/// `bytes_done` on `Active` is the live count of encrypted bytes that have
/// reached the cloud for this file so far, fed by coven's mid-upload progress
/// callback. It's 0 the instant an upload starts and climbs to `bytes_total`
/// as the file transfers.
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

/// One pending cloud delete. Deletes have no progress concept — they're a
/// single DELETE call per entry.
#[derive(Debug, Clone)]
pub struct DeleteOp {
    pub id: i64,
    pub cloud_key: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// The dominant activity of a slice of the upload queue (a release's uploads,
/// or the whole queue), for the storage-row badge. A slice with any file
/// uploading reads as `Uploading`; with none uploading but some failed and
/// awaiting retry, `Retrying`; with work still waiting, `Queued`; with every
/// file shipped, `Done`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Retrying,
    Queued,
    Done,
}

/// Upload progress as the UI cares about it: per-state counts plus
/// bytes-done/bytes-total. Used both per-release (for storage-row badges and
/// the release's bar) and as the overall total (for queue counts, ETA, the
/// master bar, and the summary band). Completed files count in `done`,
/// `bytes_done`, and `bytes_total`, so fractions are cumulative over the
/// whole burst.
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
    /// The badge activity for this slice: active uploads outrank failures
    /// awaiting retry, which outrank items still only queued, which outrank
    /// fully-shipped work. `None` when the slice holds nothing at all.
    pub fn activity(&self) -> Option<UploadActivity> {
        if self.active > 0 {
            Some(UploadActivity::Uploading)
        } else if self.failed > 0 {
            Some(UploadActivity::Retrying)
        } else if self.queued > 0 {
            Some(UploadActivity::Queued)
        } else if self.done > 0 {
            Some(UploadActivity::Done)
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

/// A release's uploads, grouped for the queue pane so it renders one
/// expandable row per release (matching the storage table) with the files
/// inside. `release_id` is `None` for the orphaned-files bucket (files whose
/// backing release is gone). `display_title` is what the row labels itself
/// with — the album title, or the first file's own name for an orphan —
/// resolved here so the UI renders it directly. `files` runs completed files
/// first (in completion order), then the remaining rows in queue order.
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
    /// Uploads grouped by release — the rows the queue pane renders. Groups
    /// whose files all completed stay listed (as done rows) until the whole
    /// queue drains, so the master bar and the pane tell one coherent story
    /// over the burst.
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

/// Build the snapshot from the outbox rows, the in-flight upload map (file_id
/// → live `bytes_done`), the completed-upload tallies for this burst, the
/// rolling-window throughput tracker, and the user-driven pause flag.
///
/// When the queue is observed fully idle — no rows, nothing in flight — the
/// burst is over: the tallies are cleared here, in the one derivation every
/// mutation path funnels through, so stale done-rows cannot survive it.
pub(crate) async fn build_outbox_snapshot(
    db: &Database,
    in_flight: &HashMap<String, u64>,
    sessions: &UploadSessions,
    throughput: &UploadThroughput,
    paused: bool,
) -> Result<OutboxSnapshot, coven::DbError> {
    let rows = db.outbox_items().await?;

    if rows.is_empty() && in_flight.is_empty() {
        sessions.clear_all();
        return Ok(OutboxSnapshot {
            paused,
            ..Default::default()
        });
    }

    let mut deletes = Vec::new();
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

    for row in rows {
        match row.operation {
            DbOutboxOperation::Upload => {
                // An upload row always carries the file id it reports progress
                // under; only a delete has none.
                let file_id = row
                    .file_id
                    .expect("an upload outbox row always carries a file_id");
                // Already tallied as done: the row lingers only until coven's
                // post-upload commit removes it, and the tally entry above
                // already represents the file. Deriving the row too would
                // double-count it — and deriving it as queued would announce
                // a completed upload as fresh work.
                if done_ids.contains(&file_id) {
                    continue;
                }
                let bytes_total = row.file_size.unwrap_or(0) as u64;
                let state = if let Some(&live) = in_flight.get(&file_id) {
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
                let idx = *group_index
                    .entry(row.release_id.clone())
                    .or_insert_with(|| {
                        groups.push(GroupBuilder::new(row.release_id.clone()));
                        groups.len() - 1
                    });
                let group = &mut groups[idx];
                if group.display_title.is_none() {
                    group.display_title = row.title;
                }
                group.push(UploadFileOp {
                    file_id,
                    display_name,
                    bytes_total,
                    state,
                });
            }
            DbOutboxOperation::Delete => {
                deletes.push(DeleteOp {
                    id: row.id,
                    cloud_key: row.cloud_key,
                    created_at: row.created_at,
                });
            }
        }
    }

    // Resolve titles the rows didn't supply: a group whose files all completed
    // has no rows left to carry the album title, so look it up by release id.
    let unresolved: Vec<String> = groups
        .iter()
        .filter(|g| g.display_title.is_none())
        .filter_map(|g| g.release_id.clone())
        .collect();
    let titles = if unresolved.is_empty() {
        HashMap::new()
    } else {
        db.album_titles_for_releases(&unresolved).await?
    };

    let upload_groups: Vec<UploadReleaseGroup> = groups
        .into_iter()
        .map(|group| {
            let display_title = group
                .display_title
                .or_else(|| {
                    group
                        .release_id
                        .as_ref()
                        .and_then(|id| titles.get(id).cloned())
                })
                .unwrap_or_else(|| {
                    // No album to name the group by (the orphaned bucket, or a
                    // release deleted mid-upload): label it by its first file,
                    // matching the per-file orphan labelling above.
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
        upload_groups,
        deletes,
        total,
        paused,
        throughput_bps,
        eta_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        Database, DbAlbum, DbArtist, DbFile, DbRelease, Pressing, ReleaseMetadataSource,
    };
    use crate::library::upload_sessions::DoneFile;
    use crate::util::content_type::ContentType;
    use chrono::Utc;
    use coven::SystemClock;
    use std::sync::Arc;
    use tempfile::TempDir;

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
    fn activity_ranks_active_over_failed_over_queued_over_done() {
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
        // Fully-shipped work reads as done; any pending work outranks it.
        assert_eq!(progress(0, 0, 0, 4).activity(), Some(UploadActivity::Done));
        assert_eq!(
            progress(1, 0, 0, 4).activity(),
            Some(UploadActivity::Queued)
        );
    }

    fn test_release(id: &str) -> DbRelease {
        DbRelease {
            id: id.into(),
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
        }
    }

    fn test_file(id: &str, release_id: &str, name: &str, size: i64) -> DbFile {
        DbFile {
            id: id.into(),
            release_id: release_id.into(),
            original_filename: name.into(),
            file_size: size,
            content_type: ContentType::Flac,
            cloud_path: None,
            created_at: Utc::now(),
        }
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
        db.insert_release(&test_release("rel-1")).await.unwrap();

        for (id, name, size) in [
            ("file-small", "01 Track Title.flac", 100i64),
            ("file-large", "02 Track Title.flac", 1000i64),
        ] {
            db.insert_file(&test_file(id, "rel-1", name, size))
                .await
                .unwrap();
            db.add_cloud_outbox_upload(id, &format!("storage/{id}"), None, false)
                .await
                .unwrap();
        }

        (db, "file-small".into(), "file-large".into(), tmp)
    }

    async fn build(
        db: &Database,
        in_flight: &HashMap<String, u64>,
        sessions: &UploadSessions,
    ) -> OutboxSnapshot {
        build_outbox_snapshot(db, in_flight, sessions, &UploadThroughput::new(), false)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn upload_groups_group_a_releases_files_with_aggregate_progress() {
        let (db, _small, _large, _tmp) = seed_two_queued_uploads().await;
        let snapshot = build(&db, &HashMap::new(), &UploadSessions::new()).await;

        // The release's two files collapse to one group carrying both.
        assert_eq!(snapshot.upload_groups.len(), 1);
        let group = &snapshot.upload_groups[0];
        assert_eq!(group.release_id.as_deref(), Some("rel-1"));
        assert_eq!(group.display_title, "Album Title");
        assert_eq!(group.files.len(), 2);
        assert_eq!(group.files[0].display_name, "01 Track Title.flac");
        assert_eq!(group.files[0].state, UploadState::Queued);
        // Aggregate progress: both queued, summed bytes (100 + 1000).
        assert_eq!(group.progress.queued, 2);
        assert_eq!(group.progress.active, 0);
        assert_eq!(group.progress.bytes_total, 1100);
    }

    #[tokio::test]
    async fn live_bytes_ride_the_active_file_and_the_totals() {
        let (db, _small, large, _tmp) = seed_two_queued_uploads().await;

        // The large file is uploading right now (250 of 1000 bytes done); the
        // small file is still queued.
        let in_flight = HashMap::from([(large.clone(), 250u64)]);
        let snapshot = build(&db, &in_flight, &UploadSessions::new()).await;

        assert_eq!(snapshot.total.bytes_total, 1100);
        // bytes_done is the in-flight file's live progress.
        assert_eq!(snapshot.total.bytes_done, 250);
        assert_eq!(snapshot.total.active, 1);
        assert_eq!(snapshot.total.queued, 1);
        let group = &snapshot.upload_groups[0];
        let active = group
            .files
            .iter()
            .find(|f| f.file_id == large)
            .expect("active file listed");
        assert_eq!(active.state, UploadState::Active { bytes_done: 250 });
    }

    /// The frozen-"Queued (1)" regression at the derivation level: a file the
    /// tally records as done while its outbox row still lingers must derive as
    /// `Done` — kept in the cumulative bytes, absent from the queued count,
    /// represented exactly once.
    #[tokio::test]
    async fn tallied_file_with_lingering_row_derives_done() {
        let (db, small, _large, _tmp) = seed_two_queued_uploads().await;

        let sessions = UploadSessions::new();
        sessions.record_done(
            Some("rel-1".into()),
            DoneFile {
                file_id: small.clone(),
                display_name: "01 Track Title.flac".into(),
                bytes: 100,
            },
        );
        let snapshot = build(&db, &HashMap::new(), &sessions).await;

        let group = &snapshot.upload_groups[0];
        assert_eq!(group.files.len(), 2, "done file represented exactly once");
        let done = group
            .files
            .iter()
            .find(|f| f.file_id == small)
            .expect("done file listed");
        assert_eq!(done.state, UploadState::Done);
        assert_eq!(group.progress.done, 1);
        assert_eq!(group.progress.queued, 1);
        // Cumulative: the completed bytes stay in numerator and denominator.
        assert_eq!(group.progress.bytes_done, 100);
        assert_eq!(group.progress.bytes_total, 1100);
    }

    /// A release whose rows all drained stays visible as a done group (title
    /// re-resolved from the release) until the queue idles, so the master bar
    /// stays cumulative over the whole burst.
    #[tokio::test]
    async fn fully_done_group_stays_listed_while_queue_busy() {
        let (db, small, large, _tmp) = seed_two_queued_uploads().await;

        // Both of rel-1's files completed and their rows are gone; a second
        // release still has a queued row keeping the queue busy.
        let pending = db.get_pending_cloud_uploads().await.unwrap();
        for entry in &pending {
            db.remove_cloud_outbox_entry(entry.id).await.unwrap();
        }
        db.insert_release(&test_release("rel-2")).await.unwrap();
        db.insert_file(&test_file(
            "file-other",
            "rel-2",
            "03 Track Title.flac",
            500,
        ))
        .await
        .unwrap();
        db.add_cloud_outbox_upload("file-other", "storage/file-other", None, false)
            .await
            .unwrap();

        let sessions = UploadSessions::new();
        for (id, bytes) in [(small.clone(), 100u64), (large.clone(), 1000u64)] {
            sessions.record_done(
                Some("rel-1".into()),
                DoneFile {
                    file_id: id.clone(),
                    display_name: id,
                    bytes,
                },
            );
        }
        let snapshot = build(&db, &HashMap::new(), &sessions).await;

        assert_eq!(snapshot.upload_groups.len(), 2);
        let done_group = &snapshot.upload_groups[0];
        assert_eq!(done_group.release_id.as_deref(), Some("rel-1"));
        // No rows left to carry the title; the batch lookup resolves it.
        assert_eq!(done_group.display_title, "Album Title");
        assert_eq!(done_group.progress.activity(), Some(UploadActivity::Done));
        assert_eq!(done_group.progress.done, 2);
        assert_eq!(done_group.progress.bytes_done, 1100);
        // The master totals span the finished release and the busy one.
        assert_eq!(snapshot.total.bytes_total, 1600);
        assert_eq!(snapshot.total.bytes_done, 1100);
        assert_eq!(snapshot.total.queued, 1);
    }

    /// An idle queue ends the burst: the tallies clear and the snapshot is
    /// empty, so no stale done-rows can outlive it.
    #[tokio::test]
    async fn idle_queue_clears_the_tallies() {
        let (db, small, _large, _tmp) = seed_two_queued_uploads().await;
        let pending = db.get_pending_cloud_uploads().await.unwrap();
        for entry in &pending {
            db.remove_cloud_outbox_entry(entry.id).await.unwrap();
        }

        let sessions = UploadSessions::new();
        sessions.record_done(
            Some("rel-1".into()),
            DoneFile {
                file_id: small,
                display_name: "01 Track Title.flac".into(),
                bytes: 100,
            },
        );
        let snapshot = build(&db, &HashMap::new(), &sessions).await;

        assert!(snapshot.upload_groups.is_empty());
        assert!(sessions.tallies().is_empty());
    }
}
