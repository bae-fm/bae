//! How far each release this process is making local has come: the bytes of
//! the file coven is downloading into the person's folder, and of the whole
//! release. Held only in memory while the copies download, before the write
//! that switches the files; no table holds it.

/// How far one release's make-local download has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MakeLocalTransferProgress {
    /// The bytes of the file being downloaded that are down.
    pub file_bytes_done: u64,
    /// The file's bytes in all.
    pub file_bytes_total: u64,
    /// The bytes of every file of the release that are down.
    pub bytes_done: u64,
    /// The bytes of every file the release downloads.
    pub bytes_total: u64,
}

impl MakeLocalTransferProgress {
    /// The progress coven reports of one step of preparing the copies.
    pub(crate) fn of(progress: coven::Progress<coven::DeviceOnlyStep>) -> Self {
        let coven::DeviceOnlyStep::Downloading { before, all, .. } = progress.step;
        Self {
            file_bytes_done: progress.done,
            file_bytes_total: progress.total,
            bytes_done: before + progress.done,
            bytes_total: all,
        }
    }
}

#[cfg(test)]
#[path = "make_local_transfers_tests.rs"]
mod tests;
