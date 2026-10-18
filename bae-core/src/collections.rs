//! Personal ratings and ordered playlists shared through coven's owner tables.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stars {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
}

impl Stars {
    pub(crate) fn from_sql(value: i64) -> rusqlite::Result<Self> {
        match value {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            4 => Ok(Self::Four),
            5 => Ok(Self::Five),
            _ => Err(rusqlite::Error::IntegralValueOutOfRange(0, value)),
        }
    }
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
    #[error("the playlist changed; read its entries before reordering it")]
    OrderChanged,
    #[error(transparent)]
    Profile(#[from] crate::people::ProfileError),
    #[error(transparent)]
    Database(#[from] crate::db::DbError),
}

impl CollectionError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory as C;
        match self {
            Self::EmptyName | Self::Missing | Self::OrderChanged => C::Config,
            Self::NotOwner => C::Membership,
            Self::Profile(error) => error.category(),
            Self::Database(_) => C::Database,
        }
    }
}
