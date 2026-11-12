use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSharing {
    Private,
    Household,
}
mirror_enum! { BridgeSharing = bae_people::people::Sharing,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Private, Household },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHistorySharing {
    Off,
    Private,
    Household,
}
mirror_enum! { BridgeHistorySharing = bae_people::people::HistorySharing,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Off, Private, Household },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStars {
    One,
    Two,
    Three,
    Four,
    Five,
}
mirror_enum! { BridgeStars = bae_people::collections::Stars,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { One, Two, Three, Four, Five },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRatingTarget {
    Album { id: String },
    Track { id: String },
}
mirror_enum! { BridgeRatingTarget = bae_people::collections::RatingTarget,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Album(id), Track(id) },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePersonProfile {
    pub id: String,
    pub name: String,
    pub private_circle: String,
    pub history_sharing: BridgeHistorySharing,
    /// The version of the person's avatar, which an app caches the image
    /// `AppHandle::avatar` reads under; it moves whenever the avatar's bytes
    /// do, and when an avatar this device couldn't read becomes readable, as
    /// a cover's does. `None` without an avatar.
    pub avatar_version: Option<String>,
}
impl BridgePersonProfile {
    pub(crate) fn from_core(value: bae_people::people::PersonProfile) -> Self {
        let bae_people::people::PersonProfile { id, name, private_circle, history_sharing, avatar } =
            value;
        Self {
            id: id.0.to_string(),
            name,
            private_circle: private_circle.0.to_string(),
            history_sharing: BridgeHistorySharing::from_core(history_sharing),
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
    pub(crate) fn from_core(value: bae_people::people::RequestedProfile) -> Self {
        let bae_people::people::RequestedProfile { name, state } = value;
        Self {
            name,
            state: match state {
                bae_people::people::RequestedProfileState::Making => {
                    BridgeRequestedProfileState::Making
                }
                bae_people::people::RequestedProfileState::Refused(reason) => {
                    BridgeRequestedProfileState::Refused {
                        reason: super::BridgeOperationRejection::from_core(reason),
                    }
                }
                bae_people::people::RequestedProfileState::Stopped(stop) => {
                    BridgeRequestedProfileState::Stopped {
                        stop: super::BridgeDeviceStop::from_core(stop),
                    }
                }
            },
        }
    }
}

/// An album's ratings as its page shows them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAlbumRating {
    /// Whether this person can rate: only a person with a profile does.
    pub can_rate: bool,
    /// The stars this person gave the album.
    pub mine: Option<BridgeStars>,
    /// Who rated the album: this person first, when they did, then each
    /// other person who did, by name.
    pub raters: Vec<BridgeRater>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRater {
    /// This person; their stars are `BridgeAlbumRating.mine`.
    You,
    Person {
        name: String,
        stars: BridgeStars,
    },
}

impl BridgeAlbumRating {
    pub(crate) fn from_core(rating: bae_views::album_detail::AlbumRating) -> Self {
        let bae_views::album_detail::AlbumRating { can_rate, mine, raters } = rating;
        Self {
            can_rate,
            mine: mine.map(BridgeStars::from_core),
            raters: raters
                .into_iter()
                .map(|rater| match rater {
                    bae_views::album_detail::Rater::You => BridgeRater::You,
                    bae_views::album_detail::Rater::Person { name, stars } => {
                        BridgeRater::Person { name, stars: BridgeStars::from_core(stars) }
                    }
                })
                .collect(),
        }
    }
}

/// Who made a playlist.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePlaylistOwner {
    pub person: String,
    pub name: String,
    /// The version of the person's avatar, which an app caches the image
    /// `AppHandle::avatar` reads under; `None` without an avatar.
    pub avatar_version: Option<String>,
}

/// The picture a playlist is drawn with.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaylistCover {
    /// One cover filling the picture; `None` draws the missing-art tile.
    Single { cover: Option<BridgeImageRef> },
    /// The covers of four different releases, first to fourth, drawn as a 2×2
    /// square; a release with no cover is `None` in its place.
    Grid { covers: Vec<Option<BridgeImageRef>> },
}
impl BridgePlaylistCover {
    fn from_core(value: bae_views::playlists::PlaylistCover) -> Self {
        match value {
            bae_views::playlists::PlaylistCover::Single(cover) => {
                Self::Single { cover: cover.map(BridgeImageRef::from_core) }
            }
            bae_views::playlists::PlaylistCover::Grid(covers) => Self::Grid {
                covers: covers
                    .into_iter()
                    .map(|cover| cover.map(BridgeImageRef::from_core))
                    .collect(),
            },
        }
    }
}

/// A playlist as the Playlists view and the Add to playlist menu list it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaylistSummary {
    pub id: String,
    pub name: String,
    pub sharing: BridgeSharing,
    pub owner: BridgePlaylistOwner,
    /// Whether this device's person made it.
    pub yours: bool,
    pub track_count: BridgeCountLabel,
    /// The entries' total playing time as the words it reads in; `None` when
    /// they report no length.
    pub total_duration: Option<BridgeDurationUnits>,
    pub cover: BridgePlaylistCover,
}
impl BridgePlaylistSummary {
    fn from_core(value: bae_views::playlists::PlaylistSummary) -> Self {
        let bae_views::playlists::PlaylistSummary {
            id,
            name,
            sharing,
            owner,
            yours,
            track_count,
            total_duration_ms,
            cover,
        } = value;
        Self {
            id: id.to_string(),
            name,
            sharing: BridgeSharing::from_core(sharing),
            owner: BridgePlaylistOwner {
                person: owner.person.0.to_string(),
                name: owner.name,
                avatar_version: owner.avatar_version,
            },
            yours,
            track_count: BridgeCountLabel::from_core(track_count),
            total_duration: bae_util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(BridgeDurationUnits::from_core),
            cover: BridgePlaylistCover::from_core(cover),
        }
    }
}

/// The playlists this device holds, now and after each change.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaylistListing {
    /// Every playlist this device holds, the person's own and the household
    /// ones others made, by name.
    pub listed: Vec<BridgePlaylistSummary>,
    /// The person's own playlists, by name: where Add to playlist can put a
    /// track.
    pub add_targets: Vec<BridgePlaylistSummary>,
    /// Whether the person has a profile, which making a playlist needs.
    pub can_create: bool,
}
impl BridgePlaylistListing {
    pub(crate) fn from_core(value: bae_views::playlists::PlaylistListing) -> Self {
        let bae_views::playlists::PlaylistListing { listed, add_targets, can_create } = value;
        Self {
            listed: listed.into_iter().map(BridgePlaylistSummary::from_core).collect(),
            add_targets: add_targets.into_iter().map(BridgePlaylistSummary::from_core).collect(),
            can_create,
        }
    }
}

/// One entry of a playlist's page.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaylistEntryRow {
    /// The entry's own id, which tells a track apart where the playlist holds
    /// it twice.
    pub position_id: String,
    pub track_id: String,
    pub title: String,
    /// The track's credit line: its own credits, else its album's.
    pub artist_line: super::BridgeCreditLine,
    pub album_id: String,
    pub album_title: String,
    pub release_id: String,
    pub duration_clock: BridgeDurationClock,
    /// The cover of the release the track is on; `None` when it has none.
    pub cover: Option<BridgeImageRef>,
}
impl BridgePlaylistEntryRow {
    fn from_core(value: bae_views::playlists::PlaylistEntryRow) -> Self {
        let bae_views::playlists::PlaylistEntryRow {
            position_id,
            track_id,
            title,
            artist_line,
            album_id,
            album_title,
            release_id,
            duration_ms,
            cover,
        } = value;
        Self {
            position_id: position_id.to_string(),
            track_id,
            title,
            artist_line: super::BridgeCreditLine::from_core(artist_line),
            album_id,
            album_title,
            release_id,
            duration_clock: BridgeDurationClock::of_length(duration_ms),
            cover: cover.map(BridgeImageRef::from_core),
        }
    }
}

/// A playlist's page: the playlist and its entries in order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaylistDetail {
    pub summary: BridgePlaylistSummary,
    pub entries: Vec<BridgePlaylistEntryRow>,
}
impl BridgePlaylistDetail {
    pub(crate) fn from_core(value: bae_views::playlists::PlaylistDetail) -> Self {
        let bae_views::playlists::PlaylistDetail { summary, entries } = value;
        Self {
            summary: BridgePlaylistSummary::from_core(summary),
            entries: entries.into_iter().map(BridgePlaylistEntryRow::from_core).collect(),
        }
    }
}

impl From<bae_people::people::ProfileError> for BridgeError {
    fn from(error: bae_people::people::ProfileError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_library::people::AvatarError> for BridgeError {
    fn from(error: bae_library::people::AvatarError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_people::collections::CollectionError> for BridgeError {
    fn from(error: bae_people::collections::CollectionError) -> Self {
        Self::categorized(error.category(), error)
    }
}
impl From<bae_store::DbError> for BridgeError {
    fn from(error: bae_store::DbError) -> Self {
        Self::categorized(bae_diagnostics::database_category(&error), error)
    }
}
