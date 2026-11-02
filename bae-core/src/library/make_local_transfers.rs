//! How far each release this process is making local has come: the bytes of
//! the file coven is downloading into the person's folder, and of the whole
//! release. Held only in memory while the copies download, before the write
//! that switches the files; no table holds it.

use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::watch;

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
    fn of(progress: coven::Progress<coven::DeviceOnlyStep>) -> Self {
        let coven::DeviceOnlyStep::Downloading { before, all, .. } = progress.step;
        Self {
            file_bytes_done: progress.done,
            file_bytes_total: progress.total,
            bytes_done: before + progress.done,
            bytes_total: all,
        }
    }
}

/// The progress of every release this process is making local, by release.
#[derive(Clone)]
pub(crate) struct MakeLocalTransfers {
    transfers: Arc<watch::Sender<BTreeMap<String, MakeLocalTransferProgress>>>,
}

impl MakeLocalTransfers {
    pub(crate) fn new() -> Self {
        Self { transfers: Arc::new(watch::Sender::new(BTreeMap::new())) }
    }

    /// Every release's progress, and each change to it.
    pub(crate) fn subscribe(&self) -> watch::Receiver<BTreeMap<String, MakeLocalTransferProgress>> {
        self.transfers.subscribe()
    }

    /// Report `release_id`'s progress through the returned report until it
    /// drops, however the transfer ends.
    pub(crate) fn report(&self, release_id: &str) -> MakeLocalReport {
        MakeLocalReport { transfers: self.clone(), release_id: release_id.to_string() }
    }
}

/// One release's progress while its copies download. Dropping it removes
/// the release.
pub(crate) struct MakeLocalReport {
    transfers: MakeLocalTransfers,
    release_id: String,
}

impl MakeLocalReport {
    /// coven reported `progress` of the release's download.
    pub(crate) fn progress(&self, progress: coven::Progress<coven::DeviceOnlyStep>) {
        let progress = MakeLocalTransferProgress::of(progress);
        self.transfers.transfers.send_modify(|transfers| {
            transfers.insert(self.release_id.clone(), progress);
        });
    }
}

impl Drop for MakeLocalReport {
    fn drop(&mut self) {
        self.transfers.transfers.send_modify(|transfers| {
            transfers.remove(&self.release_id);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(
        before: u64,
        all: u64,
        done: u64,
        total: u64,
    ) -> coven::Progress<coven::DeviceOnlyStep> {
        coven::Progress {
            step: coven::DeviceOnlyStep::Downloading {
                file: coven::FileId(uuid::Uuid::from_u128(1)),
                before,
                all,
            },
            done,
            total,
        }
    }

    /// A release's progress is the file's bytes and, of the whole release,
    /// the files before it and the file's own; it is listed while the report
    /// lives and gone once it drops.
    #[test]
    fn a_report_lists_its_release_until_it_drops() {
        let transfers = MakeLocalTransfers::new();
        let read = transfers.subscribe();
        let report = transfers.report("release");
        report.progress(step(300, 1_000, 50, 200));
        assert_eq!(
            read.borrow().get("release"),
            Some(&MakeLocalTransferProgress {
                file_bytes_done: 50,
                file_bytes_total: 200,
                bytes_done: 350,
                bytes_total: 1_000,
            })
        );
        drop(report);
        assert!(read.borrow().is_empty());
    }
}
