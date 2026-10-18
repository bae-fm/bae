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
    /// A file whose row went with its release. The entry is on its way out of
    /// the cloud, and there is nothing left to name it with — nor anything to
    /// name: what the person is watching is the release leaving, which the
    /// group says.
    Unwinding,
}

impl UploadFileLabel {
    /// The order a release's files are listed in: the release's own artwork
    /// leads, then the files by name. The artwork's name is localized by each
    /// platform and core has no locale to sort it by, so its place is fixed;
    /// the filenames sort the way every other list of names in the app does,
    /// naturally and ignoring case, so "Disc 1/05" precedes "Disc 1/18".
    fn display_order(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(label: &UploadFileLabel) -> u8 {
            match label {
                UploadFileLabel::Cover => 0,
                UploadFileLabel::ArtistImage => 1,
                UploadFileLabel::Filename(_) => 2,
                // Nameless, so there is nothing to sort it among the names by.
                UploadFileLabel::Unwinding => 3,
            }
        }
        rank(self)
            .cmp(&rank(other))
            .then_with(|| match (self, other) {
                (Self::Filename(left), Self::Filename(right)) => {
                    natord::compare_ignore_case(left, right)
                }
                _ => std::cmp::Ordering::Equal,
            })
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
    pub file_id: String,
    pub label: UploadFileLabel,
    pub source_bytes_total: u64,
    pub throughput_bps: u64,
    pub state: UploadState,
}
impl UploadFileOp {
    pub fn bar(&self) -> Option<UploadBar> {
        match self.state {
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
    /// Artist images and files whose release was removed while an upload ended.
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
        for entry in queue.uploads {
            let file = &entry.upload.file;
            let throughput_bps = if queue.paused {
                0
            } else {
                rates.get(&file.id()).copied().unwrap_or(0)
            };
            let op = UploadFileOp {
                file_id: format!("{}:{}", file.namespace(), file.id()),
                label: entry.label,
                source_bytes_total: file.plaintext_size(),
                throughput_bps,
                state: entry.upload.phase,
            };
            snapshot.total.add(op.state);
            snapshot.throughput_bps = snapshot
                .throughput_bps
                .checked_add(throughput_bps)
                .expect("upload rate overflow");
            if let Some(release_id) = entry.release_id {
                let index = *indices.entry(release_id.clone()).or_insert_with(|| {
                    snapshot.upload_groups.push(UploadReleaseGroup {
                        release_id,
                        files: Vec::new(),
                        progress: UploadProgress::default(),
                        throughput_bps: 0,
                    });
                    snapshot.upload_groups.len() - 1
                });
                let group = &mut snapshot.upload_groups[index];
                group.progress.add(op.state);
                group.throughput_bps = group
                    .throughput_bps
                    .checked_add(throughput_bps)
                    .expect("upload rate overflow");
                group.files.push(op);
            } else {
                snapshot.other_files.push(op);
            }
        }
        for group in &mut snapshot.upload_groups {
            group
                .files
                .sort_by(|left, right| left.label.display_order(&right.label));
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
