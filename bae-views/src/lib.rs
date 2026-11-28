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
//! (`composer_detail.sql`), the Storage Manager's (`storage.sql` and
//! `storage_totals.sql`), library
//! search's (`search.sql`). Each holds values a list sorts, filters or counts by that
//! come from several library tables, each kept in a device-local table so an
//! index serves it. Local triggers write them in the same write as the rows
//! they come from, so they always match those rows.

mod activity_bars;
pub mod album;
pub mod album_detail;
pub mod album_search;
pub mod artist;
pub mod artist_search;
mod browse;
pub mod composer;
pub mod composer_detail;
pub mod download_batches;
pub mod history;
mod invites;
mod library_search;
mod list_sort;
pub mod list_view;
pub use invites::{InviteEntry, OpenInvite, RefusedInvite};
mod models;
mod name_keys;
pub mod playlists;
pub mod release;
#[cfg(desktop)]
mod release_files;
mod search;
pub mod served;
pub mod storage;
pub mod storage_actions;
mod storage_browse;
#[cfg(desktop)]
pub mod storage_inspector;
mod storage_totals;
mod sync_activity;
mod uploads;

#[cfg(any(test, feature = "test-utils"))]
pub use activity_bars::activity_bars_on;
pub use activity_bars::{subscribe_activity_bars, ActivityBarRows, BatchBar, MovingShare};
pub use browse::*;
#[cfg(desktop)]
pub use release_files::{
    ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest, ReleaseFilesSnapshot,
};
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LiveLibrarySearch, SEARCH_RESULT_LIMIT,
};
pub use storage_actions::{DbStorageActionFlags, DbStorageActions};
pub use storage_browse::{
    ReleaseQueueRun, StorageQueues, StorageSelection, StorageSoleRelease, UploadQueueRun,
};
pub use sync_activity::{
    DownloadQueueActivity, DownloadQueueCounts, SyncActivity, TransferBatch, UploadQueueActivity,
    UploadQueueCounts,
};

pub use album::{AlbumCounts, AlbumDetailProjection};
pub use artist::ArtistDetailProjection;
pub use artist_search::DbArtistSearchRow;
pub use composer::WorkDetailProjection;
pub use composer_detail::ComposerDetailProjection;
pub use library_search::{resolve_library_search_projection, LibrarySearchProjection};
pub use models::*;
pub use release::ReleaseDetailProjection;
pub use storage::StorageCounts;

use composer::work_summary;
use release::{album_releases_of, album_releases_read, ReleaseDetailRows};

use bae_records::records::query::*;
use bae_records::*;
use bae_store::list_windows::OrderKey;
use bae_store::*;
use bae_util::search_text::search_pattern;
use coven::SqlReadContext;
use std::collections::BTreeSet;
use time::UtcDateTime;

include!(concat!(env!("OUT_DIR"), "/schema.rs"));

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

/// The count `count` a `COUNT(*)` read.
fn list_count(count: i64) -> Result<u64, DbError> {
    u64::try_from(count).map_err(|_error| DbError::unreadable("a list's count", count))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
mod test_artists;

#[cfg(test)]
mod test_pages;
