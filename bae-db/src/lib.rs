//! bae's library store: the `Database` over coven's handle and the entry
//! points every read and write runs through, the schema (its one migration,
//! the synced tables and the SQL functions it declares), the library's
//! records and the row writers that keep them, list windows, the cancellable
//! live queries a surface pulls values from, the people, collections and
//! history a library holds, and the sync status as coven and this device's
//! rows give it.

pub mod admin_change;
pub mod client;
pub mod collections;
pub mod credits;
mod entered_name;
pub use entered_name::{BlankName, EnteredName};
mod error;
pub use error::DbError;
pub mod history;
pub mod identity;
pub mod live_query;
pub mod membership;
pub mod migrations;
mod models;
mod pending_item;
pub mod people;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};
pub mod schema;
pub use schema::configure_store;
mod resolved_track_audio;
pub use resolved_track_audio::{
    ResolvedTrackAudio, ResolvedTrackAudioSegment, TrackAudioError, TrackAudioMeta,
};
pub mod source_audio;
pub mod stored_name;
mod sync_status;
pub mod synced;
pub use sync_status::{
    pending_action, sync_status_rows_on, PendingAction, SyncIndicator, SyncRetryOutcome,
    SyncStatusRows, SyncStatusSnapshot,
};
mod with_sync_state;
pub use with_sync_state::WithSyncState;
mod upload_progress;
pub use upload_progress::{UploadActivity, UploadProgress};
#[cfg(any(test, feature = "test-utils"))]
pub mod test_devices;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
pub mod watched_path;
pub use client::list_windows::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};
#[cfg(any(test, feature = "test-utils"))]
pub use client::resolved_in_an_empty_library;
pub use watched_path::{RelativeFolder, WatchedPathError};
// Only an import names its own artist credits from outside bae-db.
pub use client::AlbumFields;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::ArtistCredits;
pub use client::FileRow;
pub use client::PendingRow;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::RecordsChange;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::ReleaseEditWrite;
pub use client::ReleaseMetadataRows;
pub use client::{Database, IdentityWriteError, ReleaseDeletion, StorageSetupSteps};
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub use client::{SeededAlbum, SeededAudio};
pub use models::*;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
