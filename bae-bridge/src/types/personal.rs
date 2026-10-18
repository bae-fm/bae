use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSharing {
    Private,
    Household,
}
mirror_enum! { BridgeSharing = bae_core::people::Sharing,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Private, Household },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStars {
    One,
    Two,
    Three,
    Four,
    Five,
}
mirror_enum! { BridgeStars = bae_core::collections::Stars,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { One, Two, Three, Four, Five },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRatingTarget {
    Album { id: String },
    Track { id: String },
}
mirror_enum! { BridgeRatingTarget = bae_core::collections::RatingTarget,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Album(id), Track(id) },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudience {
    Store,
    Circle { id: String },
}
impl BridgeAudience {
    fn from_core(value: coven::Audience) -> Self {
        match value {
            coven::Audience::Store => Self::Store,
            coven::Audience::Circle(id) => Self::Circle {
                id: id.0.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePersonProfile {
    pub id: String,
    pub name: String,
    pub private_circle: String,
    pub history_sharing: BridgeSharing,
    pub avatar_id: Option<String>,
}
impl BridgePersonProfile {
    pub(crate) fn from_core(value: bae_core::people::PersonProfile) -> Self {
        let bae_core::people::PersonProfile {
            id,
            name,
            private_circle,
            history_sharing,
            avatar_id,
        } = value;
        Self {
            id: id.0.to_string(),
            name,
            private_circle: private_circle.0.to_string(),
            history_sharing: BridgeSharing::from_core(history_sharing),
            avatar_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRating {
    pub person: String,
    pub target: BridgeRatingTarget,
    pub stars: BridgeStars,
}
impl BridgeRating {
    pub(crate) fn from_core(value: bae_core::collections::Rating) -> Self {
        let bae_core::collections::Rating {
            person,
            target,
            stars,
        } = value;
        Self {
            person: person.0.to_string(),
            target: BridgeRatingTarget::from_core(target),
            stars: BridgeStars::from_core(stars),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePlaylist {
    pub id: String,
    pub person: String,
    pub name: String,
    pub audience: BridgeAudience,
}
impl BridgePlaylist {
    pub(crate) fn from_core(value: bae_core::collections::Playlist) -> Self {
        let bae_core::collections::Playlist {
            id,
            person,
            name,
            audience,
        } = value;
        Self {
            id: id.to_string(),
            person: person.0.to_string(),
            name,
            audience: BridgeAudience::from_core(audience),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePlaylistEntry {
    pub position_id: String,
    pub track_id: String,
}
impl BridgePlaylistEntry {
    pub(crate) fn from_core(value: bae_core::collections::PlaylistEntry) -> Self {
        let bae_core::collections::PlaylistEntry {
            position_id,
            track_id,
        } = value;
        Self {
            position_id: position_id.to_string(),
            track_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHistoryAudience {
    DevicePrivate,
    Person {
        person: String,
        audience: BridgeAudience,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeHistoryEntry {
    pub id: String,
    pub track_id: String,
    pub release_id: String,
    pub started_at: std::time::SystemTime,
    pub played: std::time::Duration,
    pub audience: BridgeHistoryAudience,
}
impl BridgeHistoryEntry {
    pub(crate) fn from_core(value: bae_core::history::HistoryEntry) -> Self {
        let bae_core::history::HistoryEntry {
            id,
            track_id,
            release_id,
            started_at,
            played,
            audience,
        } = value;
        Self {
            id: id.to_string(),
            track_id,
            release_id,
            started_at: started_at.into(),
            played,
            audience: match audience {
                bae_core::history::HistoryAudience::DevicePrivate => {
                    BridgeHistoryAudience::DevicePrivate
                }
                bae_core::history::HistoryAudience::Person { person, audience } => {
                    BridgeHistoryAudience::Person {
                        person: person.0.to_string(),
                        audience: BridgeAudience::from_core(audience),
                    }
                }
            },
        }
    }
}

impl From<bae_core::people::ProfileError> for BridgeError {
    fn from(error: bae_core::people::ProfileError) -> Self {
        Self::diagnostic(BridgeErrorCategory::from_core(error.category()), error)
    }
}
impl From<bae_core::collections::CollectionError> for BridgeError {
    fn from(error: bae_core::collections::CollectionError) -> Self {
        Self::diagnostic(BridgeErrorCategory::from_core(error.category()), error)
    }
}
impl From<bae_core::db::DbError> for BridgeError {
    fn from(error: bae_core::db::DbError) -> Self {
        Self::database(error)
    }
}
