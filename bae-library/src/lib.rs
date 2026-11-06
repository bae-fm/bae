mod album_selection;
mod assembly;
mod bootstrap;
mod closing_step;
pub(crate) mod detach_cancels;
mod detail;
#[cfg(all(feature = "dev-reset", unix))]
pub mod dev_reset;
mod file_transfers;
#[cfg(any(test, feature = "test-utils"))]
pub mod followed;
pub mod keys;
pub mod library_name;
mod library_status;
pub mod live_query_events;
mod live_read;
mod local_lifecycle;
pub(crate) mod make_local_transfers;
pub mod manager;
mod outbox_watch;
pub mod people;
mod queue_upcoming;
#[cfg(desktop)]
pub mod save;
mod saved_restore_codes;
mod schema;
pub mod storage;
pub mod sync;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub mod test_libraries;
mod track_decode;
mod upload_progress;
mod upload_throughput;
pub use album_selection::{AlbumSelectionSnapshot, LiveAlbumSelection};
pub use assembly::open_library_manager;
pub use bootstrap::{
    create_library, join_cancellable, join_with_invite, reset_device, restore_from_code,
    restore_from_saved_code, restore_replacing_device, resume_join, unfinished_joins,
    CreateLibraryError, JoinControl, JoinedLibrary, RestoreFromCodeError,
};
pub use closing_step::ClosingStep;
pub use track_decode::TrackDecode;

pub use bae_store::StorageSetupSteps;
pub use detail::{DetailSnapshot, LiveDetail};
pub use file_transfers::{FileTransferChange, FileTransfersWatch};
pub use library_status::{LibraryStatusSnapshot, LiveLibraryStatus};
pub use live_read::{live_read_channel, SubscriptionReader};
pub use live_read::{LiveRead, LiveReadError};
pub use local_lifecycle::{
    finish_deletion, finish_removals, remove_unopenable_library, unsent_before_removal,
    LibraryRemovalError, UnfinishedRemoval,
};
pub use make_local_transfers::MakeLocalTransferProgress;
pub use manager::*;
pub use outbox_watch::{
    OutboxTotalTransferWatch, OutboxTransferChange, OutboxTransfers, OutboxTransfersWatch,
    ReleaseUploadTransfer, UploadTransfer,
};
pub use queue_upcoming::{subscribe_queue_upcoming, LiveQueueUpcoming, QueueUpcomingSnapshot};
pub use saved_restore_codes::{saved_restore_codes, SavedRestoreCodesError};
pub use schema::SCHEMA;
pub use upload_progress::UploadBar;

pub use tokio_util::sync::CancellationToken;

/// One track of a save: its tags, its stored audio, and which of its audio
/// the saved file carries. Holds no open source; a save opens the files a
/// plan reads only while that track is being saved.
#[cfg(desktop)]
pub(crate) struct SaveTrackPlan {
    resolved: manager::ResolvedSaveTags,
    audio_meta: bae_records::TrackAudioMeta,
    window: manager::SaveWindow,
}

/// Why the folder for SQLite's temporary tables couldn't be named.
#[cfg(target_os = "android")]
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TemporaryDirectoryError(#[from] coven::TemporaryDirectoryError);

#[cfg(target_os = "android")]
impl TemporaryDirectoryError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        bae_diagnostics::UiErrorCategory::Config
    }
}

/// Keep SQLite's temporary tables in `directory`, the app's cache folder, for
/// every library this process opens. SQLite finds no such folder on Android
/// by itself, and a temporary table that outgrows its connection's cache, as
/// a staged snapshot does, fails without one. Called at launch, before the
/// first library opens.
#[cfg(target_os = "android")]
pub fn set_temporary_directory(directory: &std::path::Path) -> Result<(), TemporaryDirectoryError> {
    Ok(coven::Coven::set_temporary_directory(directory)?)
}
