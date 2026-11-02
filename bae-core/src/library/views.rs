//! The library's views in its store: what the album grid, the detail views
//! and library search read, and the shapes a screen shows them in. Their SQL
//! is functions over coven's read context; [`LibraryViews`] runs them through
//! the store and resolves what they read into what a screen shows.

pub(crate) mod album;
mod models;

pub use album::{AlbumBrowseProjection, AlbumDetailProjection, LibrarySearchProjection};
pub use models::*;

use crate::db::client::album_search::*;
use crate::db::client::list_windows::{
    list_order_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Seek, SortTerm,
    WindowsRead,
};
use crate::db::client::query::*;
use crate::db::client::read::*;
use crate::db::client::sync_status::cloud_home_on;
use crate::db::credits::LibraryCredit;
use crate::db::schema::search_pattern;
use crate::db::*;
use coven::{CovenError, SqlReadContext};
use rusqlite::{params, OptionalExtension};
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

#[cfg(test)]
mod tests;
