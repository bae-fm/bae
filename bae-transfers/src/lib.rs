//! The transfer queues: downloading releases to keep on this device or to
//! make them local, and exporting and saving them to a folder. Each queue is a
//! table of this device's, drained one release at a time by its owner
//! ([`QueuedReleases`]), each entry ended in the write that does its work.

mod download_entry;
mod download_snapshot;
mod output_snapshot;
mod queued_releases;
mod release_queue;
mod release_state;
mod release_transfers;
pub mod storage_moves;

pub use download_entry::{
    download_entry_column, download_entry_sql, DownloadEntry, DownloadKind, DownloadStep,
};
pub use download_snapshot::{
    DownloadPhase, DownloadProgress, DownloadProgressError, DownloadRate, DownloadRequest,
    DownloadRow, DownloadSnapshot, DownloadTransfer, DownloadTransferProgress,
    MoveToThisDeviceTransferProgress, QueuedDownload, QueuedPin,
};
pub use output_snapshot::{
    OutputEntry, OutputKind, OutputOp, OutputPhase, OutputPhaseRow, OutputPhaseSnapshot,
    OutputProgress, OutputRequest, OutputRow, OutputSnapshot, OutputState,
};
pub use queued_releases::tables;
pub use queued_releases::{ActiveEntry, Finished, QueuedPayload, QueuedReleases, RunningOp};
#[cfg(desktop)]
pub use release_queue::ReleaseQueueEntries;
pub use release_queue::{
    transfer_summary, QueueWait, QueuedRelease, ReleaseQueueCounts, ReleaseQueueEntry,
    ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot, ReleaseQueueState,
    TransferCount, TransferStage,
};
pub use release_state::release_download_entry_on;
pub use release_transfers::{
    apply_release_changes, release_changes, ReleaseTransferChange, ReleaseTransferReport,
    ReleaseTransfers, ReleaseTransfersWatch,
};
pub use storage_moves::{StorageMoveError, StorageMoveRefusal};

/// The transfer queues' part of the library's schema: one domain, its files in
/// run order.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] =
    &[bae_store::SchemaPart::new(&[queued_releases::tables::SQL])];

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, preferences' (the save presets a queued save names) and
/// the transfers'.
#[cfg(test)]
const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_preferences::SCHEMA_PARTS,
    SCHEMA_PARTS,
]);

/// The download queue, of pins and moves-to-this-device: its table, and the bytes its
/// active download has moved.
pub type Downloads = QueuedReleases<DownloadRequest, DownloadTransfer>;
/// The export and save queue: its table, and the percent its active one has
/// reached.
#[cfg(desktop)]
pub type Outputs = QueuedReleases<OutputRequest, u8>;
