//! Which Storage Manager list a read is of: the sort, which picks a compiled
//! statement or is its parameter, and what the list keeps; and the keys that
//! say where a row is in it.

use super::*;
use bae_store::list_windows::SortValue;
use bae_util::search_text::search_pattern;

/// The Storage Manager's sort, a field one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageLead {
    Title,
    TitleBack,
    Artist,
    ArtistBack,
    Media,
    MediaBack,
    Files,
    FilesBack,
    Size,
    SizeBack,
    Storage,
    StorageBack,
    Added,
    AddedBack,
}

impl StorageLead {
    fn of(sort: &StorageSortCriterion) -> Self {
        use SortDirection::{Ascending, Descending};
        use StorageSortField::{
            AlbumTitle, ArtistNames, DateAdded, FileCount, Media, Storage, TotalSize,
        };
        match (sort.field, sort.direction) {
            (AlbumTitle, Ascending) => Self::Title,
            (AlbumTitle, Descending) => Self::TitleBack,
            (ArtistNames, Ascending) => Self::Artist,
            (ArtistNames, Descending) => Self::ArtistBack,
            (Media, Ascending) => Self::Media,
            (Media, Descending) => Self::MediaBack,
            (FileCount, Ascending) => Self::Files,
            (FileCount, Descending) => Self::FilesBack,
            (TotalSize, Ascending) => Self::Size,
            (TotalSize, Descending) => Self::SizeBack,
            (Storage, Ascending) => Self::Storage,
            (Storage, Descending) => Self::StorageBack,
            (DateAdded, Ascending) => Self::Added,
            (DateAdded, Descending) => Self::AddedBack,
        }
    }

    /// The code [`super::order_fragments::StorageOrder`] takes for the sort.
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::TitleBack => "title_back",
            Self::Artist => "artist",
            Self::ArtistBack => "artist_back",
            Self::Media => "media",
            Self::MediaBack => "media_back",
            Self::Files => "files",
            Self::FilesBack => "files_back",
            Self::Size => "size",
            Self::SizeBack => "size_back",
            Self::Storage => "storage",
            Self::StorageBack => "storage_back",
            Self::Added => "added",
            Self::AddedBack => "added_back",
        }
    }
}

/// Which releases a Storage Manager list keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StorageScope {
    /// Every release.
    All,
    /// The releases the Local or Uploaded filter keeps, by their `stored`.
    Kept(i64),
    /// The releases whose album the search finds, under a filter or none. A
    /// read of them reaches them by their albums and sorts only those.
    Found { pattern: String, stored: Option<i64> },
}

impl StorageScope {
    /// What the list under `filter` and the `search` text keeps. Blank text
    /// narrows nothing.
    pub(crate) fn new(filter: StorageFilter, search: &str) -> Self {
        let stored = stored_value(filter);
        match crate::LibrarySearchQuery::parse(search) {
            Some(query) => Self::Found { pattern: search_pattern(query.as_str()), stored },
            None => stored.map_or(Self::All, Self::Kept),
        }
    }

    /// The `stored` value and the search pattern the scope keeps releases by,
    /// each `None` when it keeps every one: what a statement for any scope
    /// binds.
    pub(super) fn bound(&self) -> (Option<i64>, Option<String>) {
        match self {
            Self::All => (None, None),
            Self::Kept(stored) => (Some(*stored), None),
            Self::Found { pattern, stored } => (*stored, Some(pattern.clone())),
        }
    }
}

/// The value of `stored` a filter keeps (1 for uploaded, 0 for local), or
/// `None` when it keeps both.
fn stored_value(filter: StorageFilter) -> Option<i64> {
    match filter {
        StorageFilter::All => None,
        StorageFilter::Remote => Some(1),
        StorageFilter::Local => Some(0),
    }
}

/// One Storage Manager list: its sort and what it keeps. Every sort goes on
/// with the album title, when the release was added and the release id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StorageList {
    pub(crate) lead: StorageLead,
    pub(crate) scope: StorageScope,
}

impl StorageList {
    pub(crate) fn new(sort: &StorageSortCriterion, filter: StorageFilter, search: &str) -> Self {
        Self { lead: StorageLead::of(sort), scope: StorageScope::new(filter, search) }
    }

    /// The values `key` names. Only the list's own reads make its keys.
    pub(super) fn values_of<const N: usize>(key: &OrderKey) -> Result<[SortValue; N], DbError> {
        <[SortValue; N]>::try_from(key.values().to_vec())
            .map_err(|values| DbError::unreadable("a storage list key", values))
    }
}
