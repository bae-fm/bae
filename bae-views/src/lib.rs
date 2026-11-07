//! The library's views in its store: what the album, artist and composer
//! lists, the detail views, library search, the artist picker, the Storage
//! Manager and the sync activity read, and the shapes a screen shows them
//! in. Their SQL is functions over coven's read context; [`LibraryViews`]
//! runs them through the store and resolves what they read into what a
//! screen shows.
//!
//! Their tables are the SQL files beside the listings that read them: the
//! album grid's (`album/listing.sql`), the artist list's (`artist.sql`), the
//! composer list's (`composer.sql`), a composer's page's
//! (`composer_detail.sql`), the Storage Manager's (`storage.sql`), library
//! search's (`search.sql`) and the versions of the tables those lists page
//! (`versions.sql`). Each holds values a list sorts, filters or counts by that
//! come from several library tables, each kept in a device-local table so an
//! index serves it. Local triggers write them in the same write as the rows
//! they come from, on this device or applied from another, so they always
//! match those rows. Coven applies another device's write with foreign keys
//! checked only at its end, so a row can arrive before a row it references: a
//! trigger counts only rows whose references are there, and the referenced
//! row's own trigger, when it arrives, counts the rows that were waiting for
//! it.

pub mod album;
pub mod album_detail;
mod album_search;
pub mod artist;
pub mod artist_search;
mod browse;
pub mod composer;
pub mod composer_detail;
mod invites;
pub use invites::OpenInvite;
mod models;
pub mod release;
#[cfg(desktop)]
mod release_files;
mod search;
pub mod storage;
mod storage_browse;
#[cfg(desktop)]
pub mod storage_inspector;
mod sync_activity;
mod uploads;
mod versions;

pub use artist_search::{ArtistSearchSnapshot, ArtistSearchView, LiveArtistSearch};
pub use browse::*;
#[cfg(desktop)]
pub use release_files::{
    ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest, ReleaseFilesSnapshot,
};
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LiveLibrarySearch, SEARCH_RESULT_LIMIT,
};
pub use storage_browse::{ReleaseQueueRun, StorageBrowseSnapshot, StorageQueues, UploadQueueRun};
pub use sync_activity::{
    DownloadsInFlight, SyncActivity, SyncActivityRows, UploadQueueCounts, UploadsInFlight,
};

pub use album::{AlbumBrowseProjection, AlbumDetailProjection, LibrarySearchProjection};
pub use artist::{ArtistBrowseProjection, ArtistDetailProjection};
pub use artist_search::{ArtistSearchProjection, DbArtistSearchRow};
pub use composer::{ComposerBrowseProjection, WorkDetailProjection};
pub use composer_detail::{ComposerDetailProjection, ComposerDetailRowsProjection};
pub use models::*;
pub use release::ReleaseDetailProjection;
pub use storage::{StorageBrowseProjection, StorageBrowseRequest};

use artist::{artist_listing_query, row_to_artist_summary};
use composer::{
    composer_listing_query, row_to_composer_summary, row_to_work_summary, work_summary_query,
};
use release::{build_release_detail_on, ReleaseDetailRows};

use album_search::*;
use bae_records::credits::LibraryCredit;
use bae_records::records::files::{artist_image_version_sql, cover_version_sql};
use bae_records::records::pressing_columns::{read_media, release_media_sql};
use bae_records::records::query::*;
use bae_records::records::read::*;
use bae_records::records::{release_detail_rows_on, unstored_audio_on, DbReleaseDetailRows};
use bae_records::*;
use bae_store::list_windows::{
    list_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Seek, SortTerm, WindowsRead,
};
use bae_store::read::*;
use bae_store::syncs_on;
use bae_store::*;
use bae_transfers::download_entry_sql;
use bae_transfers::release_storage_action::{transfer_action_column, transfer_action_sql};
use bae_util::search_text::search_pattern;
use chrono::{DateTime, Utc};
use coven::SqlReadContext;
use rusqlite::{params, OptionalExtension, Row};
use std::collections::BTreeSet;

/// The library views' part of the library's schema: one domain, its files in
/// the order each runs after the files creating the tables its triggers fire
/// on.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[bae_store::SchemaPart::new(&[
    album::LISTING_SQL,
    artist::SQL,
    composer::SQL,
    composer_detail::SQL,
    storage::SQL,
    search::SQL,
    versions::SQL,
])
.functions(SQL_FUNCTIONS)];

/// The SQL functions the views' SQL names.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[&bae_store::FOLD_CASE];

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, people's and membership's, the sync status's part,
/// playback's (the tables its tests read), the transfers' (the queues the
/// release listing's pin triggers fire on) and the views'.
#[cfg(test)]
const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_people::SCHEMA_PARTS,
    bae_sync_status::SCHEMA_PARTS,
    bae_playback::SCHEMA_PARTS,
    bae_transfers::SCHEMA_PARTS,
    SCHEMA_PARTS,
]);

/// The library's store, as the library's views read it.
#[derive(Clone, Debug)]
pub struct LibraryViews {
    store: Store,
}

impl LibraryViews {
    /// The views over the library's store.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

/// `keyed`'s key columns after a select list's last column, or nothing.
fn key_columns(keyed: Option<&ListOrder>) -> String {
    keyed.map_or_else(String::new, |order| format!(", {}", order.key_columns()))
}

/// The statement reading the rows `seek` reads of a listing query built by
/// `listing` in `order`, at most `:limit` from `:offset`, each with its key.
pub(crate) fn listing_window_query(
    listing: fn(Option<&ListOrder>, Option<&str>, Option<&str>) -> String,
    order: &ListOrder,
    seek: &Seek,
) -> String {
    listing(
        Some(order),
        Some(&format!("WHERE {}", order.condition(seek))),
        Some(&format!("ORDER BY {} LIMIT :limit OFFSET :offset", order.ordering(seek))),
    )
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
