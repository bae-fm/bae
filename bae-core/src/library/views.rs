//! The library's views in its store: what the album, artist and composer
//! lists, the detail views, library search and the artist picker read, and
//! the shapes a screen shows them in. Their SQL
//! is functions over coven's read context; [`LibraryViews`] runs them through
//! the store and resolves what they read into what a screen shows.

pub(crate) mod album;
mod album_search;
pub(crate) mod artist;
pub(crate) mod artist_search;
pub(crate) mod composer;
mod models;
pub(crate) mod release;
pub(crate) mod storage;

pub use album::{AlbumBrowseProjection, AlbumDetailProjection, LibrarySearchProjection};
pub use artist::{ArtistBrowseProjection, ArtistDetailProjection};
pub use artist_search::{ArtistSearchProjection, DbArtistSearchRow};
pub use composer::{ComposerBrowseProjection, ComposerDetailProjection, WorkDetailProjection};
pub use models::*;
pub use release::ReleaseDetailProjection;
pub use storage::{StorageBrowseProjection, StorageBrowseRequest};

use artist::{artist_listing_query, row_to_artist_summary};
use composer::{
    composer_listing_query, row_to_composer_summary, row_to_work_summary, work_summary_query,
};
use release::{build_release_detail_on, unstored_audio_in_row, ReleaseDetailRows};

use crate::db::client::files::{artist_image_version_sql, cover_version_sql};
use crate::db::client::list_windows::{
    list_order_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Seek, SortTerm,
    WindowsRead,
};
use crate::db::client::pressing_columns::{read_media, release_media_sql};
use crate::db::client::query::*;
use crate::db::client::read::*;
use crate::db::client::release_storage_action::{transfer_action_column, transfer_action_sql};
use crate::db::client::sync_status::cloud_home_on;
use crate::db::credits::LibraryCredit;
use crate::db::schema::search_pattern;
use crate::db::*;
use album_search::*;
use bae_util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::{CovenError, SqlReadContext};
use rusqlite::{params, OptionalExtension, Row};
use std::collections::BTreeSet;

/// The library's store, as the library's views read it.
#[derive(Clone, Debug)]
pub struct LibraryViews {
    database: Database,
}

impl LibraryViews {
    /// The views over the library's store, `database`.
    pub fn new(database: Database) -> Self {
        Self { database }
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
mod tests;
