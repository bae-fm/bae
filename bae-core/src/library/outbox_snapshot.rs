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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub uploading: u32,
    pub upload_bytes_done: u64,
    pub upload_bytes_total: u64,
}
impl UploadProgress {
    pub fn bar(&self) -> Option<UploadBar> {
        (self.uploading > 0).then_some(UploadBar {
            bytes_done: self.upload_bytes_done,
            bytes_total: self.upload_bytes_total,
        })
    }
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
    fn add(&mut self, state: UploadState) {
        match state {
            UploadState::Waiting => {
                self.queued = self.queued.checked_add(1).expect("upload count overflow")
            }
            UploadState::Uploading {
                bytes_confirmed,
                bytes_total,
            } => {
                assert!(
                    bytes_confirmed <= bytes_total,
                    "provider progress exceeds file size"
                );
                self.uploading = self
                    .uploading
                    .checked_add(1)
                    .expect("upload count overflow");
                self.upload_bytes_done = self
                    .upload_bytes_done
                    .checked_add(bytes_confirmed)
                    .expect("upload bytes overflow");
                self.upload_bytes_total = self
                    .upload_bytes_total
                    .checked_add(bytes_total)
                    .expect("upload bytes overflow");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct UploadFileOp {
    /// Keep coven's file identity, row key, and progress together.
    pub upload: coven::QueuedUpload,
    pub pending: Option<crate::library::PendingWait>,
    pub label: UploadFileLabel,
    pub throughput_bps: u64,
}
impl UploadFileOp {
    /// The release-file row this upload belongs beside. Artwork has no such row.
    pub fn release_file_key(&self) -> Option<&coven::RowKey> {
        (self.upload.file.table() == "release_files").then(|| self.upload.file.key())
    }

    pub fn bar(&self) -> Option<UploadBar> {
        match self.upload.phase {
            UploadState::Waiting => None,
            UploadState::Uploading {
                bytes_confirmed,
                bytes_total,
            } => Some(UploadBar {
                bytes_done: bytes_confirmed,
                bytes_total,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UploadReleaseGroup {
    pub release_id: String,
    pub files: Vec<UploadFileOp>,
    pub progress: UploadProgress,
    pub throughput_bps: u64,
}
#[derive(Debug, Clone)]
pub struct ReleaseUploadProgress {
    pub progress: UploadProgress,
    pub throughput_bps: u64,
}

#[derive(Debug, Clone)]
pub struct OutboxSnapshot {
    pub revision: u64,
    pub upload_groups: Vec<UploadReleaseGroup>,
    /// The library's images (covers, artist images, avatars), which upload
    /// whether or not any release does, and uploads whose release-file rows
    /// were deleted.
    pub other_files: Vec<UploadFileOp>,
    pub total: UploadProgress,
    pub pause_state: OutboxPauseState,
    pub throughput_bps: u64,
    pub eta_seconds: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxPauseState {
    Running,
    Paused,
}

impl Default for OutboxSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            upload_groups: Vec::new(),
            other_files: Vec::new(),
            total: UploadProgress::default(),
            pause_state: OutboxPauseState::Running,
            throughput_bps: 0,
            eta_seconds: None,
        }
    }
}
impl OutboxSnapshot {
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
                .get(&(file.upload.file.uploader(), file.upload.file.id()))
                .cloned();
        }
        self
    }

    pub(crate) fn from_queue(queue: DbOutboxQueue, rates: &HashMap<coven::FileId, u64>) -> Self {
        let mut snapshot = Self {
            pause_state: if queue.paused {
                OutboxPauseState::Paused
            } else {
                OutboxPauseState::Running
            },
            ..Self::default()
        };
        let mut indices = HashMap::new();
        // Each release's files beside the names they sort by.
        let mut named_files: Vec<Vec<(String, UploadFileOp)>> = Vec::new();
        for entry in queue.uploads {
            let file = &entry.upload.file;
            let throughput_bps = if queue.paused {
                0
            } else {
                rates.get(&file.id()).copied().unwrap_or(0)
            };
            let op = UploadFileOp {
                upload: entry.upload,
                pending: None,
                label: entry.subject.label(),
                throughput_bps,
            };
            snapshot.total.add(op.upload.phase);
            snapshot.throughput_bps = snapshot
                .throughput_bps
                .checked_add(throughput_bps)
                .expect("upload rate overflow");
            match entry.subject {
                UploadSubject::ReleaseFile { release_id, name } => {
                    let index = *indices.entry(release_id.clone()).or_insert_with(|| {
                        snapshot.upload_groups.push(UploadReleaseGroup {
                            release_id,
                            files: Vec::new(),
                            progress: UploadProgress::default(),
                            throughput_bps: 0,
                        });
                        named_files.push(Vec::new());
                        snapshot.upload_groups.len() - 1
                    });
                    let group = &mut snapshot.upload_groups[index];
                    group.progress.add(op.upload.phase);
                    group.throughput_bps = group
                        .throughput_bps
                        .checked_add(throughput_bps)
                        .expect("upload rate overflow");
                    named_files[index].push((name, op));
                }
                UploadSubject::Cover { .. }
                | UploadSubject::ArtistImage
                | UploadSubject::Avatar
                | UploadSubject::DeletedReleaseFile => snapshot.other_files.push(op),
            }
        }
        // A release's files are listed the way every other list of names in
        // the app sorts, naturally and ignoring case, so "Disc 1/05" precedes
        // "Disc 1/18".
        for (group, mut files) in snapshot.upload_groups.iter_mut().zip(named_files) {
            files.sort_by(|(left, _), (right, _)| natord::compare_ignore_case(left, right));
            group.files = files.into_iter().map(|(_, op)| op).collect();
        }
        let remaining = snapshot.total.upload_bytes_total - snapshot.total.upload_bytes_done;
        if snapshot.total.queued == 0 && remaining > 0 && snapshot.throughput_bps > 0 {
            snapshot.eta_seconds = Some(remaining.div_ceil(snapshot.throughput_bps));
        }
        snapshot
    }
}

#[cfg(test)]
#[path = "outbox_snapshot_tests.rs"]
mod tests;
