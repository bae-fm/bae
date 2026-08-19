//! The cloud-outbox processing snapshot: the one source of truth for the Storage
//! Manager's queue panel, the per-release upload badges, and the master progress
//! bar. Re-emitted on every queue mutation (enqueue, upload start, progress tick,
//! success, failure, cancel, retry), so no consumer keeps cached counts of its own.
//!
//! Two inputs derive it:
//!
//! - coven's durable cloud queue, read through
//!   [`Database::outbox_queue`](crate::db::Database::outbox_queue): what remains.
//! - An in-memory map of preparation and provider-transfer callbacks. It refines
//!   the durable phase with buffer-cadence byte progress; a restart loses only
//!   those live counters and immediately retains coven's durable lower bound.

use std::collections::HashMap;

use crate::db::DbOutboxQueue;
use crate::library::upload_throughput::UploadThroughput;

/// One immutable cloud blob identity. A row can be repointed at a replacement
/// blob, so upload progress and completion follow the namespace and blob id,
/// not the row that happens to reference it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct UploadBlobKey {
    namespace: String,
    blob_id: String,
}

impl UploadBlobKey {
    pub(crate) fn new(namespace: impl Into<String>, blob_id: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            blob_id: blob_id.into(),
        }
    }

    pub(crate) fn from_row(blob: &coven::RowBlobRef) -> Self {
        Self::new(blob.blob().namespace.clone(), blob.blob().id.clone())
    }

    fn stable_id(&self) -> String {
        format!("{}:{}", self.namespace, self.blob_id)
    }
}

/// What the queue calls one uploaded object. Filenames are source data; image
/// kinds are localized by each platform from their typed case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadFileLabel {
    Filename(String),
    Cover,
    ArtistImage,
}

/// What an upload is doing right now, derived from coven's durable phase and
/// buffer-cadence callbacks. Preparation counts plaintext source bytes; upload
/// counts encrypted provider bytes, so each active phase carries its own exact
/// denominator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadState {
    Queued,
    Preparing {
        bytes_done: u64,
        bytes_total: u64,
    },
    Prepared {
        bytes_total: u64,
    },
    Uploading {
        bytes_done: u64,
        bytes_total: u64,
    },
    RetryingPreparation {
        last_error: String,
    },
    RetryingUpload {
        last_error: String,
        bytes_total: u64,
    },
    /// Provider bytes exist, but pinning or publishing the release failed.
    RetryingPublication {
        last_error: String,
        bytes_total: u64,
    },
    /// The cloud object exists; publication has not activated the release yet.
    Uploaded {
        bytes_total: u64,
    },
}

/// Buffer-cadence facts that are true only while this process performs work.
/// Coven's durable upload phase remains the restart truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransientUploadState {
    Preparing {
        bytes_done: u64,
        bytes_total: u64,
    },
    /// The provider write began but has not delivered its first exact byte
    /// report. The durable Prepared row owns the denominator in this interval.
    UploadStarted,
    Uploading {
        bytes_done: u64,
        bytes_total: u64,
    },
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

/// The dominant activity of one release transition or the whole cloud queue.
/// The order matches the user journey and gives foreground work precedence over
/// work waiting behind it. There is no terminal variant: after publication the
/// group leaves the snapshot and the storage row reads its resting Cloud state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Cancelling,
    Publishing,
    Uploading,
    Preparing,
    Retrying,
    Prepared,
    Queued,
    Uploaded,
}

/// Upload progress as the UI renders it: per-phase counts, each I/O stage's
/// native byte units, and one two-stage work fraction for progress bars. Serves
/// both a release and the whole queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub preparing: u32,
    pub prepared: u32,
    pub uploading: u32,
    pub retrying: u32,
    pub uploaded: u32,
    pub publishing: u32,
    pub cancelling: u32,
    pub preparation_bytes_done: u64,
    pub preparation_bytes_total: u64,
    pub upload_bytes_done: u64,
    pub upload_bytes_total: u64,
    /// Whether `upload_bytes_total` covers every upload in this slice. Provider
    /// byte sizes become exact only after preparation finishes.
    pub upload_bytes_total_complete: bool,
    /// Two-stage work: source preparation plus provider upload.
    pub work_done: u64,
    pub work_total: u64,
}

impl Default for UploadProgress {
    fn default() -> Self {
        Self {
            queued: 0,
            preparing: 0,
            prepared: 0,
            uploading: 0,
            retrying: 0,
            uploaded: 0,
            publishing: 0,
            cancelling: 0,
            preparation_bytes_done: 0,
            preparation_bytes_total: 0,
            upload_bytes_done: 0,
            upload_bytes_total: 0,
            upload_bytes_total_complete: true,
            work_done: 0,
            work_total: 0,
        }
    }
}

impl UploadProgress {
    fn scaled_stage_work(done: u64, total: u64, weight: u64) -> u64 {
        assert!(
            done <= total,
            "upload progress cannot exceed its exact total"
        );
        if total == 0 {
            return 0;
        }
        let scaled = u128::from(done) * u128::from(weight) / u128::from(total);
        u64::try_from(scaled).expect("scaled upload work fits its source-byte weight")
    }

    /// True while this slice still belongs to an unfinished make-Remote
    /// transition. `uploaded` remains present until publication activates the
    /// release, so the UI never mistakes provider completion for terminal Cloud.
    pub fn has_transition(&self) -> bool {
        self.queued > 0
            || self.preparing > 0
            || self.prepared > 0
            || self.uploading > 0
            || self.retrying > 0
            || self.uploaded > 0
            || self.publishing > 0
            || self.cancelling > 0
    }

    /// Whether coven can still unwind this make-Remote transition. Publishing
    /// has committed the provider objects and is activating the release, while
    /// cancelling already represents the requested unwind.
    pub fn can_cancel(&self) -> bool {
        self.has_transition() && self.publishing == 0 && self.cancelling == 0
    }

    /// The badge activity for this slice: active uploads outrank failures
    /// awaiting retry, which outrank items still only queued. `None` when
    /// nothing is pending.
    pub fn activity(&self) -> Option<UploadActivity> {
        if self.cancelling > 0 {
            Some(UploadActivity::Cancelling)
        } else if self.publishing > 0 {
            Some(UploadActivity::Publishing)
        } else if self.uploading > 0 {
            Some(UploadActivity::Uploading)
        } else if self.preparing > 0 {
            Some(UploadActivity::Preparing)
        } else if self.retrying > 0 {
            Some(UploadActivity::Retrying)
        } else if self.prepared > 0 {
            Some(UploadActivity::Prepared)
        } else if self.queued > 0 {
            Some(UploadActivity::Queued)
        } else if self.uploaded > 0 {
            Some(UploadActivity::Uploaded)
        } else {
            None
        }
    }

    fn add_upload(&mut self, state: &UploadState, bytes_total: u64) {
        self.preparation_bytes_total = self
            .preparation_bytes_total
            .checked_add(bytes_total)
            .expect("upload byte total overflow");
        self.work_total = self
            .work_total
            .checked_add(bytes_total.checked_mul(2).expect("upload work overflow"))
            .expect("upload work total overflow");
        match state {
            UploadState::Queued => {
                self.queued = self.queued.checked_add(1).expect("upload count overflow");
                self.upload_bytes_total_complete = false;
            }
            UploadState::Preparing {
                bytes_done,
                bytes_total: preparation_total,
            } => {
                self.preparing = self
                    .preparing
                    .checked_add(1)
                    .expect("upload count overflow");
                assert_eq!(
                    *preparation_total, bytes_total,
                    "preparation progress must use the source's exact plaintext total"
                );
                assert!(
                    *bytes_done <= *preparation_total,
                    "preparation progress cannot exceed its exact total"
                );
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(*bytes_done)
                    .expect("preparation byte progress overflow");
                self.work_done = self
                    .work_done
                    .checked_add(*bytes_done)
                    .expect("upload work progress overflow");
                self.upload_bytes_total_complete = false;
            }
            UploadState::Prepared {
                bytes_total: upload_total,
            } => {
                self.prepared = self.prepared.checked_add(1).expect("upload count overflow");
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(bytes_total)
                    .expect("preparation byte progress overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(*upload_total)
                    .expect("provider byte total overflow");
                self.work_done = self
                    .work_done
                    .checked_add(bytes_total)
                    .expect("upload work progress overflow");
            }
            UploadState::Uploading {
                bytes_done,
                bytes_total: upload_total,
            } => {
                self.uploading = self
                    .uploading
                    .checked_add(1)
                    .expect("upload count overflow");
                assert!(
                    *bytes_done <= *upload_total,
                    "provider progress cannot exceed its exact total"
                );
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(bytes_total)
                    .expect("preparation byte progress overflow");
                self.upload_bytes_done = self
                    .upload_bytes_done
                    .checked_add(*bytes_done)
                    .expect("provider byte progress overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(*upload_total)
                    .expect("provider byte total overflow");
                let completed_work = bytes_total
                    .checked_add(Self::scaled_stage_work(
                        *bytes_done,
                        *upload_total,
                        bytes_total,
                    ))
                    .expect("upload work progress overflow");
                self.work_done = self
                    .work_done
                    .checked_add(completed_work)
                    .expect("upload work progress overflow");
            }
            UploadState::RetryingPreparation { .. } => {
                self.retrying = self
                    .retrying
                    .checked_add(1)
                    .expect("upload retry count overflow");
                self.upload_bytes_total_complete = false;
            }
            UploadState::RetryingUpload {
                bytes_total: upload_total,
                ..
            } => {
                self.retrying = self
                    .retrying
                    .checked_add(1)
                    .expect("upload retry count overflow");
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(bytes_total)
                    .expect("preparation byte progress overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(*upload_total)
                    .expect("provider byte total overflow");
                self.work_done = self
                    .work_done
                    .checked_add(bytes_total)
                    .expect("upload work progress overflow");
            }
            UploadState::RetryingPublication {
                bytes_total: upload_total,
                ..
            } => {
                self.retrying = self
                    .retrying
                    .checked_add(1)
                    .expect("upload retry count overflow");
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(bytes_total)
                    .expect("preparation byte progress overflow");
                self.upload_bytes_done = self
                    .upload_bytes_done
                    .checked_add(*upload_total)
                    .expect("provider byte progress overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(*upload_total)
                    .expect("provider byte total overflow");
                self.work_done = self
                    .work_done
                    .checked_add(bytes_total.checked_mul(2).expect("upload work overflow"))
                    .expect("upload work progress overflow");
            }
            UploadState::Uploaded {
                bytes_total: upload_total,
            } => {
                self.uploaded = self.uploaded.checked_add(1).expect("upload count overflow");
                self.preparation_bytes_done = self
                    .preparation_bytes_done
                    .checked_add(bytes_total)
                    .expect("preparation byte progress overflow");
                self.upload_bytes_done = self
                    .upload_bytes_done
                    .checked_add(*upload_total)
                    .expect("provider byte progress overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(*upload_total)
                    .expect("provider byte total overflow");
                self.work_done = self
                    .work_done
                    .checked_add(bytes_total.checked_mul(2).expect("upload work overflow"))
                    .expect("upload work progress overflow");
            }
        }
    }

    fn add_progress(&mut self, progress: &UploadProgress) {
        self.queued = self
            .queued
            .checked_add(progress.queued)
            .expect("upload count overflow");
        self.preparing = self
            .preparing
            .checked_add(progress.preparing)
            .expect("upload count overflow");
        self.prepared = self
            .prepared
            .checked_add(progress.prepared)
            .expect("upload count overflow");
        self.uploading = self
            .uploading
            .checked_add(progress.uploading)
            .expect("upload count overflow");
        self.retrying = self
            .retrying
            .checked_add(progress.retrying)
            .expect("upload retry count overflow");
        self.uploaded = self
            .uploaded
            .checked_add(progress.uploaded)
            .expect("upload count overflow");
        self.publishing = self
            .publishing
            .checked_add(progress.publishing)
            .expect("upload count overflow");
        self.cancelling = self
            .cancelling
            .checked_add(progress.cancelling)
            .expect("upload count overflow");
        self.preparation_bytes_done = self
            .preparation_bytes_done
            .checked_add(progress.preparation_bytes_done)
            .expect("preparation byte progress overflow");
        self.preparation_bytes_total = self
            .preparation_bytes_total
            .checked_add(progress.preparation_bytes_total)
            .expect("preparation byte total overflow");
        self.upload_bytes_done = self
            .upload_bytes_done
            .checked_add(progress.upload_bytes_done)
            .expect("provider byte progress overflow");
        self.upload_bytes_total = self
            .upload_bytes_total
            .checked_add(progress.upload_bytes_total)
            .expect("provider byte total overflow");
        self.upload_bytes_total_complete &= progress.upload_bytes_total_complete;
        self.work_done = self
            .work_done
            .checked_add(progress.work_done)
            .expect("upload work progress overflow");
        self.work_total = self
            .work_total
            .checked_add(progress.work_total)
            .expect("upload work total overflow");
    }
}

/// One file in a release's upload group: what the queue pane's per-file rows
/// render. `source_bytes_total` is the local plaintext file size; an active
/// state's associated values carry that phase's exact progress denominator.
#[derive(Debug, Clone)]
pub struct UploadFileOp {
    pub file_id: String,
    pub label: UploadFileLabel,
    pub source_bytes_total: u64,
    pub state: UploadState,
}

/// A release's uploads, grouped so the queue pane renders one expandable row per
/// release (matching the storage table) with the files inside. Every durable
/// upload is rooted at a live release; missing release/title context fails the
/// database projection before this value can exist. Files retain queue order.
#[derive(Debug, Clone)]
pub struct UploadReleaseGroup {
    pub release_id: String,
    pub display_title: String,
    pub files: Vec<UploadFileOp>,
    pub progress: UploadProgress,
}

/// Complete snapshot of the cloud outbox. One source of truth for everything
/// upload-related the UI renders.
#[derive(Debug, Clone)]
pub struct OutboxSnapshot {
    /// Monotonic publication number assigned by the owning sync controller.
    /// Import completion carries the revision that first represented its
    /// durable enqueue, so a coalesced subscriber can distinguish "not seen
    /// yet" from "already reached terminal Cloud state."
    pub revision: u64,
    /// Uploads grouped by release — the rows the queue pane renders. A group
    /// remains through provider completion and publication, then leaves only
    /// when coven consumes the durable make-Remote transition.
    pub upload_groups: Vec<UploadReleaseGroup>,
    pub deletes: Vec<DeleteOp>,
    /// Sum across all uploads — drives the queue counts, ETA, the master
    /// progress bar, and the summary band.
    pub total: UploadProgress,
    /// Whether uploads are running, finishing the provider write that was
    /// already active when pause was requested, or fully paused between
    /// entries.
    pub pause_state: OutboxPauseState,
    /// Rolling-window upload throughput in bytes per second. Zero when the
    /// queue is idle or has been idle long enough for the window to drain. The
    /// UI formats it as a localized rate; aggregate bytes come from `total`.
    pub throughput_bps: u64,
    /// Estimated seconds remaining at the current rate. `None` when throughput
    /// is zero or no bytes remain. The UI formats it.
    pub eta_seconds: Option<u64>,
}

impl Default for OutboxSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            upload_groups: Vec::new(),
            deletes: Vec::new(),
            total: UploadProgress::default(),
            pause_state: OutboxPauseState::Running,
            throughput_bps: 0,
            eta_seconds: None,
        }
    }
}

/// The effective pause phase of the cloud upload pipeline. A provider write
/// cannot be interrupted halfway through; requesting pause while one is active
/// therefore enters `Pausing` until that exact write completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxPauseState {
    Running,
    Pausing,
    Paused,
}

impl OutboxSnapshot {
    pub fn transitioning_release_ids(&self) -> Vec<String> {
        let mut ids = self
            .upload_groups
            .iter()
            .map(|group| group.release_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn per_release_progress(&self) -> HashMap<String, UploadProgress> {
        self.upload_groups
            .iter()
            .map(|group| (group.release_id.clone(), group.progress.clone()))
            .collect()
    }

    pub fn pending_delete_count(&self) -> u32 {
        u32::try_from(self.deletes.len()).expect("pending delete count exceeds u32")
    }

    /// The summary line's phase counts in dominance order, followed by pending
    /// deletes. Each zero count drops out; platforms only localize and join.
    pub fn summary_parts(&self) -> Vec<crate::library::release_queue::CountLabel> {
        use crate::library::release_queue::CountLabel;
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

struct GroupBuilder {
    release_id: String,
    display_title: String,
    files: Vec<UploadFileOp>,
    progress: UploadProgress,
}

impl GroupBuilder {
    fn new(release_id: String, display_title: String) -> Self {
        Self {
            release_id,
            display_title,
            files: Vec::new(),
            progress: UploadProgress::default(),
        }
    }

    fn push(&mut self, file: UploadFileOp) {
        self.progress
            .add_upload(&file.state, file.source_bytes_total);
        self.files.push(file);
    }

    fn set_transition(&mut self, progress: coven::MakeRemoteProgress) {
        match progress {
            coven::MakeRemoteProgress::Uploading => {}
            coven::MakeRemoteProgress::Cancelling => self.progress.cancelling = 1,
            coven::MakeRemoteProgress::Publishing => self.progress.publishing = 1,
        }
    }
}

fn upload_eta_seconds(bytes_remaining: u64, throughput_bps: u64) -> u64 {
    assert!(bytes_remaining > 0, "ETA requires remaining provider bytes");
    assert!(throughput_bps > 0, "ETA requires nonzero throughput");
    bytes_remaining.div_ceil(throughput_bps)
}

/// Build the snapshot from coven's durable queue, buffer-cadence preparation and
/// provider callbacks, the rolling-window throughput tracker, and pause state.
///
/// A pure derivation over already-read state: everything it needs about what is
/// queued arrives in `queue`, so it neither reads the database nor fails.
///
/// Durable emptiness is terminal. Transient callbacks can refine a durable row,
/// never keep one alive after publication removed it.
pub(crate) fn build_outbox_snapshot(
    queue: DbOutboxQueue,
    transient: &HashMap<UploadBlobKey, TransientUploadState>,
    throughput: &UploadThroughput,
    pause_requested: bool,
) -> OutboxSnapshot {
    if queue.uploads.is_empty() && queue.deletes.is_empty() && queue.make_remotes.is_empty() {
        return OutboxSnapshot {
            pause_state: if pause_requested {
                OutboxPauseState::Paused
            } else {
                OutboxPauseState::Running
            },
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
    let mut group_index: HashMap<String, usize> = HashMap::new();

    for upload in queue.uploads {
        let blob_key = UploadBlobKey::from_row(&upload.blob);
        let bytes_total = upload.blob.plaintext_size();
        let state = resolve_upload_state(
            upload.phase,
            upload.provider_bytes_total,
            transient.get(&blob_key).copied(),
            upload.last_error,
        );
        let idx = *group_index
            .entry(upload.release_id.clone())
            .or_insert_with(|| {
                groups.push(GroupBuilder::new(
                    upload.release_id.clone(),
                    upload.album_title.clone(),
                ));
                groups.len() - 1
            });
        let group = &mut groups[idx];
        assert_eq!(
            group.display_title, upload.album_title,
            "one release cannot have conflicting queued album titles"
        );
        group.push(UploadFileOp {
            file_id: blob_key.stable_id(),
            label: upload.label,
            source_bytes_total: bytes_total,
            state,
        });
    }

    for make_remote in queue.make_remotes {
        let release_id = make_remote.transition.root_id.clone();
        let idx = *group_index.entry(release_id.clone()).or_insert_with(|| {
            groups.push(GroupBuilder::new(
                release_id,
                make_remote.album_title.clone(),
            ));
            groups.len() - 1
        });
        let group = &mut groups[idx];
        assert_eq!(
            group.display_title, make_remote.album_title,
            "one release cannot have conflicting queued album titles"
        );
        group.set_transition(make_remote.transition.progress);
    }

    let upload_groups: Vec<UploadReleaseGroup> = groups
        .into_iter()
        .filter(|group| group.progress.has_transition())
        .map(|group| UploadReleaseGroup {
            release_id: group.release_id,
            display_title: group.display_title,
            files: group.files,
            progress: group.progress,
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
    let pause_state = if !pause_requested {
        OutboxPauseState::Running
    } else if total.preparing > 0 || total.uploading > 0 {
        OutboxPauseState::Pausing
    } else {
        OutboxPauseState::Paused
    };
    let throughput_bps = if pause_requested {
        0
    } else {
        throughput.bytes_per_sec()
    };
    let bytes_remaining = total
        .upload_bytes_total
        .checked_sub(total.upload_bytes_done)
        .expect("provider progress cannot exceed its exact total");
    let eta_seconds = if pause_requested
        || !total.upload_bytes_total_complete
        || total.uploading == 0
        || throughput_bps == 0
        || bytes_remaining == 0
    {
        None
    } else {
        Some(upload_eta_seconds(bytes_remaining, throughput_bps))
    };

    OutboxSnapshot {
        revision: 0,
        upload_groups,
        deletes,
        total,
        pause_state,
        throughput_bps,
        eta_seconds,
    }
}

fn resolve_upload_state(
    phase: coven::QueuedUploadPhase,
    provider_bytes_total: Option<u64>,
    transient: Option<TransientUploadState>,
    last_error: Option<String>,
) -> UploadState {
    if phase == coven::QueuedUploadPhase::Created {
        let bytes_total =
            provider_bytes_total.expect("a Created upload requires a durable provider total");
        return match last_error {
            Some(last_error) => UploadState::RetryingPublication {
                last_error,
                bytes_total,
            },
            None => UploadState::Uploaded { bytes_total },
        };
    }
    // The durable queue query and the in-process callback stream are
    // independent feeds, so a transient can be one step ahead of or behind
    // the row it refines: a provider write's first callback can arrive
    // before the requery delivers the Prepared row, and a failed attempt's
    // rollback can land while its last callback is still in flight. The
    // durable phase is the truth; a transient that disagrees with it refines
    // nothing and the row renders from its durable state alone until the
    // feeds agree again.
    match transient {
        Some(TransientUploadState::Uploading {
            bytes_done,
            bytes_total,
        }) if phase == coven::QueuedUploadPhase::Prepared => {
            let exact_total =
                provider_bytes_total.expect("a Prepared upload requires a durable provider total");
            assert_eq!(
                bytes_total, exact_total,
                "provider callback total must match coven's durable provider total"
            );
            return UploadState::Uploading {
                bytes_done,
                bytes_total: exact_total,
            };
        }
        Some(TransientUploadState::UploadStarted)
            if phase == coven::QueuedUploadPhase::Prepared =>
        {
            let bytes_total =
                provider_bytes_total.expect("a Prepared upload requires a durable provider total");
            return UploadState::Uploading {
                bytes_done: 0,
                bytes_total,
            };
        }
        Some(TransientUploadState::Preparing {
            bytes_done,
            bytes_total,
        }) if phase == coven::QueuedUploadPhase::Pending => {
            assert!(
                provider_bytes_total.is_none(),
                "a Pending upload cannot already have a prepared provider object"
            );
            return UploadState::Preparing {
                bytes_done,
                bytes_total,
            };
        }
        Some(_) | None => {}
    }
    match (phase, provider_bytes_total, last_error) {
        (coven::QueuedUploadPhase::Pending, None, Some(last_error)) => {
            UploadState::RetryingPreparation { last_error }
        }
        (coven::QueuedUploadPhase::Prepared, Some(bytes_total), Some(last_error)) => {
            UploadState::RetryingUpload {
                last_error,
                bytes_total,
            }
        }
        (coven::QueuedUploadPhase::Pending, None, None) => UploadState::Queued,
        (coven::QueuedUploadPhase::Prepared, Some(bytes_total), None) => {
            UploadState::Prepared { bytes_total }
        }
        (phase, provider_bytes_total, _) => panic!(
            "coven upload phase {phase:?} has invalid durable provider total {provider_bytes_total:?}"
        ),
    }
}

#[cfg(test)]
#[path = "outbox_snapshot_tests.rs"]
mod tests;
