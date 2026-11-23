//! Personal ratings and ordered playlists shared through coven's owner tables.

pub use bae_types::stars::Stars;

/// The stars a stored count names, read in a row closure.
pub(crate) fn stars_from_sql(value: i64) -> rusqlite::Result<Stars> {
    Stars::from_stored(value).ok_or(rusqlite::Error::IntegralValueOutOfRange(0, value))
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
