use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSharing {
    Private,
    Household,
}
mirror_enum! { BridgeSharing = bae_db::people::Sharing,
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
mirror_enum! { BridgeStars = bae_db::collections::Stars,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { One, Two, Three, Four, Five },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRatingTarget {
    Album { id: String },
    Track { id: String },
}
mirror_enum! { BridgeRatingTarget = bae_db::collections::RatingTarget,
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
            coven::Audience::Circle(id) => Self::Circle { id: id.0.to_string() },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePersonProfile {
    pub id: String,
    pub name: String,
    pub private_circle: String,
    pub history_sharing: BridgeSharing,
    /// The version of the person's avatar, which an app caches the image
    /// `AppHandle::avatar` reads under; it moves whenever the avatar's bytes
    /// do, and when an avatar this device couldn't read becomes readable, as
    /// a cover's does. `None` without an avatar.
    pub avatar_version: Option<String>,
}
impl BridgePersonProfile {
    pub(crate) fn from_core(value: bae_db::people::PersonProfile) -> Self {
        let bae_db::people::PersonProfile { id, name, private_circle, history_sharing, avatar } =
            value;
        Self {
            id: id.0.to_string(),
            name,
            private_circle: private_circle.0.to_string(),
            history_sharing: BridgeSharing::from_core(history_sharing),
            avatar_version: avatar.map(|avatar| avatar.version),
        }
    }
}

/// The profile this device's person asked for, until it is written.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRequestedProfile {
    pub name: String,
    pub state: BridgeRequestedProfileState,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRequestedProfileState {
    /// The person's private circle is being made; the profile is written
    /// once sync has made it.
    Making,
    /// The library refused the circle; asking again asks anew.
    Refused { reason: super::BridgeOperationRejection },
    /// This device stopped for good before the circle was made; asking
    /// again asks anew.
    Stopped { stop: super::BridgeDeviceStop },
}
impl BridgeRequestedProfile {
    pub(crate) fn from_core(value: bae_db::people::RequestedProfile) -> Self {
        let bae_db::people::RequestedProfile { name, state } = value;
        Self {
            name,
            state: match state {
                bae_db::people::RequestedProfileState::Making => {
                    BridgeRequestedProfileState::Making
                }
                bae_db::people::RequestedProfileState::Refused(reason) => {
                    BridgeRequestedProfileState::Refused {
                        reason: super::BridgeOperationRejection::from_core(reason),
                    }
                }
                bae_db::people::RequestedProfileState::Stopped(stop) => {
                    BridgeRequestedProfileState::Stopped {
                        stop: super::BridgeDeviceStop::from_core(stop),
                    }
                }
            },
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
    pub(crate) fn from_core(value: bae_db::collections::Rating) -> Self {
        let bae_db::collections::Rating { person, target, stars } = value;
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
    pub(crate) fn from_core(value: bae_db::collections::Playlist) -> Self {
        let bae_db::collections::Playlist { id, person, name, audience } = value;
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
    pub(crate) fn from_core(value: bae_db::collections::PlaylistEntry) -> Self {
        let bae_db::collections::PlaylistEntry { position_id, track_id } = value;
        Self { position_id: position_id.to_string(), track_id }
    }
}

impl From<bae_db::people::ProfileError> for BridgeError {
    fn from(error: bae_db::people::ProfileError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_library::people::AvatarError> for BridgeError {
    fn from(error: bae_library::people::AvatarError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_db::collections::CollectionError> for BridgeError {
    fn from(error: bae_db::collections::CollectionError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_db::DbError> for BridgeError {
    fn from(error: bae_db::DbError) -> Self {
        Self::database(error)
    }
}
