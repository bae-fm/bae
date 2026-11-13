use crate::credits::LibraryCredit;
use crate::models::*;
use bae_util::content_type::ContentType;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};
// coven owns production connections; row-mapping tests also use bare SQLite.
use bae_store::Store;
use bae_store::{DbError, WriteError};
use coven::{CovenWrite, SqlReadContext};
use std::collections::{HashMap, HashSet};

/// The columns of a release file row `f` that `row_to_file` reads: its own,
/// and from coven's record of the file it references, `file_size`, the
/// file's size, and `stored`, whether the file is stored. A macro so a
/// caller's `concat!` can build its statement from it.
#[macro_export]
macro_rules! release_file_columns {
    () => {
        "f.*, (SELECT coven.size FROM coven_files AS coven WHERE coven.id = f.file) \
         AS file_size, \
         (SELECT coven.hash IS NOT NULL FROM coven_files AS coven WHERE coven.id = f.file) \
         AS stored"
    };
}

mod album;
pub use album::PrimaryReleaseError;
mod catalog_rank;
pub(crate) use catalog_rank::CATALOG_RANK;
mod artist;
pub mod artist_credit_names;
use artist_credit_names::settle_credit_name;
pub mod artist_resolution;
mod catalog_match;
pub mod label_resolution;
#[cfg(any(test, feature = "test-utils"))]
pub use artist_resolution::resolved_in_an_empty_library;
pub use artist_resolution::ArtistCredits;
use artist_resolution::{relink_album_artists, relink_track_artists};
pub use artist_resolution::{IdentityRefusal, IdentityWriteError, IdentityWriteRefusal};
use label_resolution::label_write;
pub use label_resolution::{LabelParts, LabelWrite};
// Only the import write resolves credits inside a release's own write.
#[cfg(desktop)]
use artist_resolution::relink;
pub mod files;
pub use files::{ImageWriteError, ImageWriteRefusal};
pub(crate) mod identity_merges;
pub mod queued_uploads;
pub mod release_records;
// Only identifying a release, a desktop operation, changes its records.
#[cfg(desktop)]
pub use release_records::RecordsChange;
mod release;
#[cfg(desktop)]
pub use release::release_edit_tracks_on;
pub use release::ReleaseMetadataRows;
// Only saving a release edit session writes metadata alongside records.
#[cfg(desktop)]
use release::write_release_metadata_on;
#[cfg(desktop)]
mod release_edit_save;
pub use release::AlbumFields;
#[cfg(desktop)]
pub use release_edit_save::{ReleaseEditSaveError, ReleaseEditWrite};
mod emptied;
use emptied::Emptiable;
mod release_credits;
mod release_deletion;
mod release_detail;
pub use release_detail::{
    release_detail_rows_on, unstored_audio_on, DbReleaseDetailRows, OtherDeviceError,
};
mod release_file;
mod release_locality;
pub use release_deletion::{Deleted, DeletionError, DeletionRefusal, ReleaseDeletion};
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
mod seeded_albums;
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub use seeded_albums::{SeededAlbum, SeededAudio};
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;
mod track;

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;

pub mod pressing_columns;
pub mod query;
pub mod read;
mod scrub_points_column;
pub mod write;

use bae_store::query::*;
use bae_store::read::*;
use query::*;
use read::*;
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

/// Records' part of the schema: `records.sql`.
pub const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("records.sql")])
        .functions(SQL_FUNCTIONS)
        .collations(SQL_COLLATIONS)
        .synced(crate::synced::synced_tables, crate::synced::namespaces);

/// The SQL functions `records.sql` names.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[&CATALOG_RANK];

/// The collations `records.sql` names.
const SQL_COLLATIONS: &[&bae_store::SchemaCollation] = &[&bae_store::NATURAL_ORDER];
