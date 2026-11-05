//! bae's library records, people, membership and sync status over the store:
//! the `Database` and the row writers that keep them, the people,
//! collections and history a library holds, and the sync status as coven and
//! this device's rows give it. This crate's part of the schema is
//! `001_initial.sql`.

pub mod admin_change;
pub mod client;
pub mod collections;
pub mod credits;
pub mod history;
pub mod identity;
pub mod membership;
mod models;
mod pending_item;
pub mod people;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};

/// bae-db's part of the schema: `001_initial.sql`, which holds every table
/// no other crate's part creates.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] =
    &[bae_store::SchemaPart::new(&[include_str!("../migrations/001_initial.sql")])
        .functions(SQL_FUNCTIONS)
        .collations(SQL_COLLATIONS)
        .synced(synced::synced_tables, synced::namespaces)];

/// The SQL functions `001_initial.sql` names. The text functions and the
/// folder paths' are the parts' that read them: the import list's and the
/// views'.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[&client::CATALOG_RANK];

/// The collations `001_initial.sql` names.
const SQL_COLLATIONS: &[&bae_store::SchemaCollation] = &[&bae_store::NATURAL_ORDER];
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
mod upload_progress;
pub use upload_progress::{UploadActivity, UploadProgress};
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
#[cfg(any(test, feature = "test-utils"))]
pub use client::resolved_in_an_empty_library;
// Only an import names its own artist credits from outside bae-db.
pub use client::AlbumFields;
#[cfg(desktop)]
pub use client::ArtistCredits;
pub use client::FileRow;
pub use client::PendingRow;
#[cfg(desktop)]
pub use client::RecordsChange;
#[cfg(desktop)]
pub use client::ReleaseEditWrite;
pub use client::ReleaseMetadataRows;
pub use client::{Database, IdentityWriteError, LabelParts, ReleaseDeletion};
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub use client::{SeededAlbum, SeededAudio};
pub use models::*;

#[cfg(test)]
mod schema_tests;
