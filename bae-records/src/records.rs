use crate::credits::LibraryCredit;
use crate::models::*;
use bae_store::Store;
use bae_store::WriteContext;
use bae_store::{DbError, WriteError};
use bae_util::content_type::ContentType;
use std::collections::{HashMap, HashSet};

mod album;
pub use album::PrimaryReleaseError;
pub(crate) mod album_identity;
mod artist;
pub mod artist_credit_names;
pub(crate) mod artist_sql;
pub mod artist_standing;
pub(crate) mod catalog_rank;
pub mod credit_rule;
pub(crate) mod lengths_disagree;
use artist_credit_names::settle_credit_name;
pub mod artist_resolution;
mod catalog_match;
mod credit_candidates;
pub mod label_resolution;
#[cfg(any(test, feature = "test-utils"))]
pub use artist_resolution::{credit_artists_checked_on, resolved_in_an_empty_library};
pub use artist_resolution::{ArtistCredits, GonePicked};
pub use artist_resolution::{IdentityRefusal, IdentityWriteError};
use label_resolution::pressing_write;
pub use label_resolution::{PressingParts, PressingWrite};
// Only the import write resolves credits inside a release's own write.
#[cfg(desktop)]
use artist_resolution::relink;
pub mod files;
pub use files::{ImageWriteError, ImageWriteRefusal};
pub mod fragments;
pub(crate) mod identity_merges;
pub mod name_sort;
pub mod queued_uploads;
pub mod release_records;
pub mod spread_fragments;
// Only identifying a release, a desktop operation, changes its records.
#[cfg(desktop)]
pub use release_records::RecordsChange;
pub mod release;
pub use release::{edited_release_of, EditedRelease, ReleaseMetadataRows};
// Only saving a release edit session writes metadata alongside records.
#[cfg(desktop)]
#[cfg(desktop)]
use release::write_release_metadata_on;
#[cfg(desktop)]
mod release_edit_save;
pub use release::AlbumFields;
#[cfg(desktop)]
pub use release_edit_save::{
    GonePicks, ReleaseEditPlan, ReleaseEditRefusal, ReleaseEditSaveError, ReleaseEditWrite,
};
mod emptied;
pub mod layout;
pub mod layout_fragments;
pub mod sheet_files;
pub mod sheet_fragments;
use emptied::Emptiable;
pub mod browse_fragments;
pub mod credit_fragments;
#[cfg(desktop)]
pub mod editor_files;
#[cfg(desktop)]
pub mod editor_tracks;
#[cfg(desktop)]
pub use editor_tracks::draft_release_edit_row_of;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub use editor_tracks::draft_release_edit_tracks_on;
#[cfg(desktop)]
pub mod release_edit_rows;
#[cfg(desktop)]
pub use release_edit_rows::{editor_release_on, release_to_edit_on, ReleaseToEdit};
pub mod file_fragments;
pub mod place_fragments;
mod release_credits;
mod release_deletion;
pub mod release_detail;
pub mod track_group_fragments;
#[cfg(all(desktop, test))]
pub use editor_files::EditorReleaseFilesParams;
#[cfg(desktop)]
pub use release_detail::release_files_of;
#[cfg(all(desktop, test))]
pub use release_detail::release_files_on;
pub use release_detail::OtherDeviceError;
pub mod export_numbers;
mod release_file;
mod release_locality;
mod release_tracks;
pub mod release_with_files;
pub mod served;
pub mod track_display;
pub use release_deletion::{Deleted, DeletionError, DeletionRefusal, ReleaseDeletion};
pub use release_tracks::{track_place_of, track_places_read, track_position_of, track_side_of};
pub use release_with_files::ReleaseWithFiles;
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
mod seeded_albums;
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub use seeded_albums::{SeededAlbum, SeededAudio};
#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod test_capabilities;
mod track;
mod track_list;
pub use track::UnknownTrackSource;
pub use track_list::{TrackList, TrackReading};
pub mod track_audio;
pub use track_audio::{DbTrackTags, TrackForSave, TrackSaveRead};

// Four disc names, in the natural order bae's schema names.
#[cfg(test)]
coven::read!(
    writer NaturallyOrderedNames { name: String },
    r#"
    SELECT listed.column1 AS name
    FROM (VALUES ('Disc 10'), ('disc 2'), ('Disc 2'), ('Disc 1')) AS listed
    ORDER BY listed.column1 COLLATE natural_order
"#
);

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "records/covers_tests.rs"]
mod covers_tests;

#[cfg(any(test, feature = "test-utils", desktop))]
pub mod file_writes;
pub mod image_writes;
pub mod pressing_columns;
pub mod query;
pub mod read;
mod scrub_points_column;
#[cfg(any(test, feature = "test-utils", desktop))]
pub mod work_writes;
pub mod write;

use bae_store::query::*;
#[cfg(any(test, feature = "test-utils", desktop))]
use file_writes::*;
use image_writes::*;
use read::*;
#[cfg(any(test, feature = "test-utils", desktop))]
use work_writes::*;
use write::*;

/// The library's records and release storage over the store.
#[derive(Clone)]
pub struct Records {
    /// The store every read and write of the records runs through.
    store: Store,
    /// Where a row a write makes takes its fresh id: a new label known only
    /// by its name.
    ids: coven::IdSourceRef,
}

impl std::fmt::Debug for Records {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Records").finish_non_exhaustive()
    }
}

impl Records {
    // ── Records method conventions ───────────────────────────────────────
    //
    // Lookup methods come in two shapes, picked by where the ID came from. Using
    // the wrong one is a bug.
    //
    // 1. `find_*` — the ID came from the caller (a screen's action, an API parameter, a
    //    user-provided string). The row may not exist; the user could be looking
    //    at something since deleted. Returns `Result<Option<T>>`.
    //
    // 2. `get_*_for_*` — you're following a foreign key off a record you already
    //    read from the DB. The row MUST exist; if it doesn't, our data integrity
    //    is broken. Returns `Result<T>`, NOT Option, and takes the parent record
    //    rather than a raw ID string so it can't be called with a caller-provided
    //    ID. A missing row surfaces as `QueryReturnedNoRows`.

    pub fn new(store: Store, ids: coven::IdSourceRef) -> Self {
        Records { store, ids }
    }
}
