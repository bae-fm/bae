//! Files that finished uploading during the current queue burst, grouped the
//! same way the outbox snapshot groups its rows (by the file's release; `None`
//! for a file with no backing release row).
//!
//! The outbox tables only know what *remains* — coven deletes an upload's row
//! the moment its post-upload commit lands — so on their own they can't answer
//! "how far along is this release?" or even "is this file done?" while the row
//! lingers between the upload finishing and the commit. This tally is the
//! missing half: the upload observer records each completion as coven reports
//! it, and the snapshot builder merges it with the remaining rows so
//!
//! - a completed file whose row hasn't been removed yet derives as `Done`,
//!   never as freshly `Queued`;
//! - a release's progress is cumulative (completed bytes stay in both the
//!   numerator and the denominator) instead of shrinking as rows drain;
//! - a fully-uploaded release stays visible as a done row while the rest of
//!   the queue drains.
//!
//! In-memory only, on purpose: after a restart the remaining rows are the
//! honest denominator, and an empty tally re-bases progress to what's left.
//! The builder clears the tally itself whenever it observes the queue fully
//! idle, so stale done-rows cannot outlive the burst no matter which mutation
//! path emptied the queue.

use std::collections::HashMap;
use std::sync::Mutex;

/// One completed upload, as recorded by the observer when coven reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneFile {
    pub file_id: String,
    /// What the queue pane labels the file with — the file's original name, or
    /// the file id when no release-file row backs it.
    pub display_name: String,
    pub bytes: u64,
}

/// A group's completed files, in completion order. `seq` orders groups by
/// first completion so the snapshot renders them deterministically.
#[derive(Debug, Clone)]
struct GroupTally {
    seq: u64,
    files: Vec<DoneFile>,
}

/// The completed-upload tallies for the current queue burst. Shared between
/// the upload observer (writer), the snapshot builder (reader + idle clear),
/// and the cancel paths (per-group clear).
#[derive(Debug, Default)]
pub struct UploadSessions {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    groups: HashMap<Option<String>, GroupTally>,
    next_seq: u64,
}

impl UploadSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed upload under its release. A re-upload of a file
    /// already recorded (a retried post-upload commit) replaces the entry
    /// rather than double-counting it.
    pub fn record_done(&self, release_id: Option<String>, file: DoneFile) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.groups.contains_key(&release_id) {
            let seq = inner.next_seq;
            inner.next_seq += 1;
            inner.groups.insert(
                release_id.clone(),
                GroupTally {
                    seq,
                    files: Vec::new(),
                },
            );
        }
        let group = inner
            .groups
            .get_mut(&release_id)
            .expect("group inserted above");
        match group.files.iter_mut().find(|f| f.file_id == file.file_id) {
            Some(existing) => *existing = file,
            None => group.files.push(file),
        }
    }

    /// Drop one group's tally. Used when a release's transition is cancelled —
    /// its already-uploaded blobs get tombstoned, so "done" would be a lie.
    pub fn clear_group(&self, release_id: Option<&str>) {
        self.inner
            .lock()
            .unwrap()
            .groups
            .remove(&release_id.map(str::to_string));
    }

    /// Drop every tally. The snapshot builder calls this when it observes the
    /// queue fully idle, ending the burst.
    pub fn clear_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.groups.clear();
        inner.next_seq = 0;
    }

    /// The tallies as `(release_id, completed files)` pairs, ordered by each
    /// group's first completion.
    pub fn tallies(&self) -> Vec<(Option<String>, Vec<DoneFile>)> {
        let inner = self.inner.lock().unwrap();
        let mut groups: Vec<_> = inner.groups.iter().collect();
        groups.sort_by_key(|(_, tally)| tally.seq);
        groups
            .into_iter()
            .map(|(release_id, tally)| (release_id.clone(), tally.files.clone()))
            .collect()
    }
}
