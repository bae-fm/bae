//! The transfer queues and storage transitions: downloading releases to keep
//! on this device, exporting and saving them to a folder, and the storage
//! transition each release is in. Each queue is a table of this device's,
//! drained one release at a time by its owner ([`QueuedReleases`]); the
//! transitions are `release_storage_action`'s rows, each ended in the write
//! that changes the release's files.

mod download_entry;
mod download_snapshot;
mod output_snapshot;
mod queued_releases;
mod release_queue;
mod release_state;
pub mod release_storage_action;
mod release_transfers;

pub use download_entry::{download_entry_column, download_entry_sql, DownloadEntry, QueueWait};
pub use download_snapshot::{
    DownloadPhase, DownloadProgress, DownloadProgressError, DownloadRate, DownloadRow,
    DownloadSnapshot, DownloadTransferProgress, QueuedDownload, QueuedPin,
};
pub use output_snapshot::{
    OutputEntry, OutputKind, OutputOp, OutputPhase, OutputPhaseRow, OutputPhaseSnapshot,
    OutputProgress, OutputRequest, OutputRow, OutputSnapshot, OutputState,
};
pub use queued_releases::tables;
pub use queued_releases::{QueuedPayload, QueuedReleases, RunningOp};
#[cfg(desktop)]
pub use release_queue::ReleaseQueueEntries;
pub use release_queue::{
    transfer_summary, QueuedRelease, ReleaseQueueCounts, ReleaseQueueEntry, ReleaseQueueOp,
    ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot, ReleaseQueueState,
    ReleaseQueueWindows, TransferCount, TransferStage,
};
pub use release_state::{release_transfer_state_on, ReleaseTransferState};
pub use release_storage_action::{
    StorageTransitionError, StorageTransitionRefusal, StoredTransition,
};
pub use release_transfers::{
    release_changes, ReleaseTransferChange, ReleaseTransferReport, ReleaseTransfers,
    ReleaseTransfersWatch,
};

/// The transfer queues' part of the library's schema: one domain, its files in
/// run order.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] =
    &[bae_store::SchemaPart::new(&[queued_releases::tables::SQL, release_storage_action::SQL])];

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, preferences' (the save presets a queued save names) and
/// the transfers'.
#[cfg(test)]
const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_preferences::SCHEMA_PARTS,
    SCHEMA_PARTS,
]);

/// The pin queue: its table, and the bytes its active download has moved.
pub type Downloads = QueuedReleases<(), DownloadTransferProgress>;
/// The export and save queue: its table, and the percent its active one has
/// reached.
#[cfg(desktop)]
pub type Outputs = QueuedReleases<OutputRequest, u8>;
