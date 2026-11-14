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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rating {
    pub person: coven::MemberId,
    pub target: RatingTarget,
    pub stars: Stars,
}

/// What the album's page reads of its ratings: whether this person can rate
/// (they have a profile), the stars they gave it, and the stars each other
/// person gave it, by name then id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumRatings {
    pub can_rate: bool,
    pub mine: Option<Stars>,
    pub others: Vec<OtherRating>,
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
