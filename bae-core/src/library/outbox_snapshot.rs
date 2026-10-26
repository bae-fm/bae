//! Upload display values derived from coven's committed queue.
use crate::db::DbOutboxQueue;
pub use coven::UploadPhase as UploadState;
use std::collections::HashMap;

/// What the queue calls one uploaded object. Filenames are source data; image
/// kinds are localized by each platform from their typed case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadFileLabel {
    Filename(String),
    Cover,
    ArtistImage,
    Avatar,
    /// A committed upload whose release-file row was deleted.
    DeletedReleaseFile,
}

/// What one queued upload is, read from the table that declares its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadSubject {
    /// One of a release's own files, attached when the person moved the
    /// release to the cloud: its upload is the release's.
    ReleaseFile {
        release_id: String,
        name: String,
    },
    /// A release's cover. It is bae's own image, which coven queues whenever
    /// the cover row changes, a local release's included, so it is never the
    /// release uploading.
    Cover {
        release_id: String,
    },
    ArtistImage,
    Avatar,
    /// A committed upload whose release-file row was deleted.
    DeletedReleaseFile,
}

impl UploadSubject {
    /// The release this upload moves to the cloud: the release of one of its
    /// own files, and nothing else.
    pub fn uploading_release(&self) -> Option<&str> {
        match self {
            Self::ReleaseFile { release_id, .. } => Some(release_id),
            Self::Cover { .. } | Self::ArtistImage | Self::Avatar | Self::DeletedReleaseFile => {
                None
            }
        }
    }

    /// The release the uploaded file belongs to, which names it.
    pub fn release(&self) -> Option<&str> {
        match self {
            Self::ReleaseFile { release_id, .. } | Self::Cover { release_id } => Some(release_id),
            Self::ArtistImage | Self::Avatar | Self::DeletedReleaseFile => None,
        }
    }

    pub fn label(&self) -> UploadFileLabel {
        match self {
            Self::ReleaseFile { name, .. } => UploadFileLabel::Filename(name.clone()),
            Self::Cover { .. } => UploadFileLabel::Cover,
            Self::ArtistImage => UploadFileLabel::ArtistImage,
            Self::Avatar => UploadFileLabel::Avatar,
            Self::DeletedReleaseFile => UploadFileLabel::DeletedReleaseFile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Queued,
}

/// How many queued files wait and how many are being sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub uploading: u32,
}
impl UploadProgress {
    pub fn has_transition(&self) -> bool {
        self.queued > 0 || self.uploading > 0
    }
    pub fn activity(&self) -> Option<UploadActivity> {
        if self.uploading > 0 {
            Some(UploadActivity::Uploading)
        } else if self.queued > 0 {
            Some(UploadActivity::Queued)
        } else {
            None
        }
    }
    /// Count one queued file: waiting, or being sent now.
    fn add(&mut self, phase: UploadState) {
        let count = match phase {
            UploadState::Waiting => &mut self.queued,
            UploadState::Uploading => &mut self.uploading,
        };
        *count = count.checked_add(1).expect("upload count overflow");
    }
}

/// One queued upload: which file, whether it is being sent, and why it
/// waits when it does. How far it has gone is its [`UploadTransfer`].
#[derive(Debug, Clone, PartialEq)]
pub struct UploadFileOp {
    /// coven's file identity and row key.
    pub file: coven::FileRef,
    pub phase: UploadState,
    pub pending: Option<crate::library::PendingWait>,
    pub label: UploadFileLabel,
}
impl UploadFileOp {
    /// The release-file row this upload belongs beside. Artwork has no such row.
    pub fn release_file_key(&self) -> Option<&coven::RowKey> {
        (self.file.table() == "release_files").then(|| self.file.key())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadReleaseGroup {
    pub release_id: String,
    pub files: Vec<UploadFileOp>,
    pub progress: UploadProgress,
}

/// The upload queue as the Storage Manager lists it: which files wait or are
/// being sent, for which releases, and whether the queue is paused. It
/// changes only as files join or leave the queue, start or stop being sent,
/// or wait on something; the bytes they move are [`OutboxTransfers`].
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxQueue {
    pub upload_groups: Vec<UploadReleaseGroup>,
    /// The library's images (covers, artist images, avatars), which upload
    /// whether or not any release does, and uploads whose release-file rows
    /// were deleted.
    pub other_files: Vec<UploadFileOp>,
    pub total: UploadProgress,
    pub pause_state: OutboxPauseState,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxPauseState {
    Running,
    Paused,
}

/// What a transfer is of: one queued file, or a release's files together.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UploadTransferKey {
    File(coven::FileId),
    Release(String),
}

/// How far an upload has gone and how fast it goes now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadTransfer {
    /// The bytes the provider confirmed: for a file, while it has an upload
    /// session (a paused one keeps its own; a file sent in one request has
    /// none); for a release or the whole queue, those of the files being sent
    /// now, while any is.
    pub bar: Option<UploadBar>,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// The bytes the queued uploads have moved and how fast they move: every
/// queued file's transfer and every uploading release's, keyed, and the
/// queue's. They change with each part a provider confirms.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxTransfers {
    pub keyed: HashMap<UploadTransferKey, UploadTransfer>,
    pub total: UploadTransfer,
}

/// The upload queue and its transfers as one value, read from one durable
/// queue. `revision` counts the changes to the queue alone: a new transfer
/// on the same queue keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxState {
    pub revision: u64,
    pub queue: OutboxQueue,
    pub transfers: OutboxTransfers,
}

/// The transfer of several files together: the bytes of those being sent
/// and every file's rate.
#[derive(Default)]
struct TransferSum {
    uploading: bool,
    bytes_done: u64,
    bytes_total: u64,
    throughput_bps: u64,
}
impl TransferSum {
    /// Count one file's bytes, while it is being sent, and its rate. A file
    /// sent in one request has no bytes to count.
    fn add(&mut self, upload: &coven::QueuedUpload, throughput_bps: u64) {
        self.throughput_bps = self
            .throughput_bps
            .checked_add(throughput_bps)
            .expect("upload rate overflow");
        if upload.phase != UploadState::Uploading {
            return;
        }
        self.uploading = true;
        if let Some(progress) = &upload.progress {
            assert!(
                progress.bytes_confirmed <= progress.bytes_total,
                "provider progress exceeds file size"
            );
            self.bytes_done = self
                .bytes_done
                .checked_add(progress.bytes_confirmed)
                .expect("upload bytes overflow");
            self.bytes_total = self
                .bytes_total
                .checked_add(progress.bytes_total)
                .expect("upload bytes overflow");
        }
    }

    fn transfer(&self) -> UploadTransfer {
        UploadTransfer {
            bar: self.uploading.then_some(UploadBar {
                bytes_done: self.bytes_done,
                bytes_total: self.bytes_total,
            }),
            throughput_bps: self.throughput_bps,
        }
    }
}

impl OutboxQueue {
    pub(crate) fn with_pending(mut self, records: Vec<coven::PendingRecord>) -> Self {
        let records: HashMap<_, _> = records
            .into_iter()
            .filter_map(|record| match record.subject {
                coven::PendingSubject::File { device, file } => {
                    Some(((device, file), crate::library::PendingWait::of(&record)))
                }
                _ => None,
            })
            .collect();
        for file in self
            .upload_groups
            .iter_mut()
            .flat_map(|group| &mut group.files)
            .chain(&mut self.other_files)
        {
            file.pending = records
                .get(&(file.file.uploader(), file.file.id()))
                .cloned();
        }
        self
    }

    /// The queue `queue` holds, and the transfers of its files at `rates`.
    pub(crate) fn from_queue(
        queue: DbOutboxQueue,
        rates: &HashMap<coven::FileId, u64>,
    ) -> (Self, OutboxTransfers) {
        let mut snapshot = Self {
            upload_groups: Vec::new(),
            other_files: Vec::new(),
            total: UploadProgress::default(),
            pause_state: if queue.paused {
                OutboxPauseState::Paused
            } else {
                OutboxPauseState::Running
            },
        };
        let mut keyed = HashMap::new();
        let mut total = TransferSum::default();
        let mut release_sums: Vec<TransferSum> = Vec::new();
        let mut indices = HashMap::new();
        // Each release's files beside the names they sort by.
        let mut named_files: Vec<Vec<(String, UploadFileOp)>> = Vec::new();
        for entry in queue.uploads {
            let upload = entry.upload;
            // A file no rate was measured for moves nothing yet.
            let throughput_bps = if queue.paused {
                0
            } else {
                rates.get(&upload.file.id()).copied().unwrap_or(0)
            };
            keyed.insert(
                UploadTransferKey::File(upload.file.id()),
                UploadTransfer {
                    bar: upload.progress.map(|progress| UploadBar {
                        bytes_done: progress.bytes_confirmed,
                        bytes_total: progress.bytes_total,
                    }),
                    throughput_bps,
                },
            );
            total.add(&upload, throughput_bps);
            snapshot.total.add(upload.phase);
            let label = entry.subject.label();
            match entry.subject {
                UploadSubject::ReleaseFile { release_id, name } => {
                    let index = *indices.entry(release_id.clone()).or_insert_with(|| {
                        snapshot.upload_groups.push(UploadReleaseGroup {
                            release_id,
                            files: Vec::new(),
                            progress: UploadProgress::default(),
                        });
                        named_files.push(Vec::new());
                        release_sums.push(TransferSum::default());
                        snapshot.upload_groups.len() - 1
                    });
                    snapshot.upload_groups[index].progress.add(upload.phase);
                    release_sums[index].add(&upload, throughput_bps);
                    named_files[index].push((
                        name,
                        UploadFileOp {
                            file: upload.file,
                            phase: upload.phase,
                            pending: None,
                            label,
                        },
                    ));
                }
                UploadSubject::Cover { .. }
                | UploadSubject::ArtistImage
                | UploadSubject::Avatar
                | UploadSubject::DeletedReleaseFile => snapshot.other_files.push(UploadFileOp {
                    file: upload.file,
                    phase: upload.phase,
                    pending: None,
                    label,
                }),
            }
        }
        // A release's files are listed the way every other list of names in
        // the app sorts, naturally and ignoring case, so "Disc 1/05" precedes
        // "Disc 1/18".
        for ((group, mut files), sum) in snapshot
            .upload_groups
            .iter_mut()
            .zip(named_files)
            .zip(&release_sums)
        {
            files.sort_by(|(left, _), (right, _)| natord::compare_ignore_case(left, right));
            group.files = files.into_iter().map(|(_, op)| op).collect();
            keyed.insert(
                UploadTransferKey::Release(group.release_id.clone()),
                sum.transfer(),
            );
        }
        (
            snapshot,
            OutboxTransfers {
                keyed,
                total: total.transfer(),
            },
        )
    }
}

#[cfg(test)]
#[path = "outbox_snapshot_tests.rs"]
mod tests;
