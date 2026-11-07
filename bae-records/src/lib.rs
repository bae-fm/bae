//! bae's library records and where each release's files are: artists, albums,
//! releases, tracks, works and labels with their files, the rules that resolve
//! credits and labels and merge identities, the row writers an import and an
//! edit call inside their own writes, and release storage, behind the
//! `Records` owner.

pub mod credits;
pub mod identity;
mod models;
pub mod records;
pub mod release_storage;
mod resolved_track_audio;
pub mod source_audio;
pub mod stored_name;
pub mod synced;
mod upload_progress;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;

pub use models::*;
#[cfg(any(test, feature = "test-utils"))]
pub use records::resolved_in_an_empty_library;
// Only an import names its own artist credits from outside bae-records.
pub use records::AlbumFields;
#[cfg(desktop)]
pub use records::ArtistCredits;
#[cfg(desktop)]
pub use records::RecordsChange;
#[cfg(desktop)]
pub use records::ReleaseEditWrite;
pub use records::ReleaseMetadataRows;
pub use records::{
    Deleted, DeletionError, IdentityWriteError, ImageWriteError, LabelParts, OtherDeviceError,
    Records, ReleaseDeletion,
};
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub use records::{SeededAlbum, SeededAudio};
pub use resolved_track_audio::{
    ResolvedTrackAudio, ResolvedTrackAudioSegment, TrackAudioError, TrackAudioMeta,
};
pub use upload_progress::{UploadActivity, UploadProgress};

/// The parts of the schema records and release storage hold, in the order
/// they run.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[records::SCHEMA, release_storage::SCHEMA];
