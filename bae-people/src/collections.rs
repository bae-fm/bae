//! Personal ratings and ordered playlists shared through coven's owner tables.

pub use bae_types::stars::Stars;

use crate::people::{PersonRef, Sharing};
use bae_records::ImageRef;
use bae_types::count_label::CountLabel;

/// How many different releases' covers a playlist's picture shows when it
/// has that many.
pub const COVER_GRID: usize = 4;

/// The picture a playlist is drawn with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistCover {
    /// One cover: the first entry's release's when the playlist's entries
    /// are on fewer than four different releases. `None` draws the
    /// missing-art tile, as a playlist with no entries or a first release
    /// with no cover does.
    Single(Option<ImageRef>),
    /// The covers of the first four different releases the entries are on,
    /// in entry order, drawn as a 2×2 square; a release with no cover is
    /// `None` in its place.
    Grid([Option<ImageRef>; COVER_GRID]),
}

/// A playlist as the Playlists view and the Add to playlist menu list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub sharing: Sharing,
    pub owner: PersonRef,
    /// Whether this device's person made it.
    pub yours: bool,
    /// "{count} tracks" (`core.library.track_count`).
    pub track_count: CountLabel,
    /// The entries' total playing time; zero when there are none.
    pub total_duration_ms: u64,
    pub cover: PlaylistCover,
}

/// One entry of a playlist's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntryRow {
    pub position_id: uuid::Uuid,
    pub track_id: String,
    pub title: String,
    /// The track's credit line: its own credits, else its album's (§2.3).
    pub artist_line: bae_types::credits::CreditLine,
    pub album_id: String,
    pub album_title: String,
    pub release_id: String,
    pub duration_ms: u64,
    /// The cover of the release the track is on.
    pub cover: Option<ImageRef>,
    /// The entry's number in the playlist's order, counted from 1.
    pub number: u64,
    /// Whether an entry comes before this one, so it can move up.
    pub can_move_up: bool,
    /// Whether an entry comes after this one, so it can move down.
    pub can_move_down: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RatingTarget {
    Album(String),
    Track(String),
}

/// A place in a playlist beside one of its entries, named by its position
/// id: just before it or just after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BesideEntry {
    Before(uuid::Uuid),
    After(uuid::Uuid),
}

/// Where an insert puts new playlist entries: at the playlist's start, the
/// one place a playlist with no entries has, or beside one of its entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertPlace {
    Start,
    Beside(BesideEntry),
}

/// Where a move puts a playlist entry: beside another entry, or one place up
/// or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryPlace {
    Beside(BesideEntry),
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rating {
    pub person: coven::MemberId,
    pub target: RatingTarget,
    pub stars: Stars,
}

/// What the album's page shows of its ratings in the library's system: this
/// person's stars and each other's, in name and id order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumRatings {
    pub mine: Option<Stars>,
    pub others: Vec<OtherRating>,
    pub has_raters: bool,
}

/// One other person's stars for an album, under the name their profile
/// carries. A removed person's profile stays, so their rating still names
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtherRating {
    pub name: String,
    pub stars: Stars,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub id: uuid::Uuid,
    pub person: coven::MemberId,
    pub name: String,
    pub audience: coven::Audience,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub position_id: uuid::Uuid,
    pub track_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    #[error("a playlist's name cannot be empty")]
    EmptyName,
    #[error("the playlist or entry no longer exists")]
    Missing,
    #[error("only the playlist's author can change it")]
    NotOwner,
    #[error(transparent)]
    Profile(#[from] crate::people::ProfileError),
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

impl From<bae_store::BlankName> for CollectionError {
    fn from(_: bae_store::BlankName) -> Self {
        Self::EmptyName
    }
}

impl CollectionError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::EmptyName | Self::Missing => C::Config,
            Self::NotOwner => C::Membership,
            Self::Profile(error) => error.category(),
            Self::Database(error) => bae_diagnostics::database_category(error),
        }
    }
}
