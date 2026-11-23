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

/// How the library shows the ratings of albums, or of tracks. `Hidden` is
/// Library settings' *None*: no ratings show, and they stay stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRatingSystem {
    Stars,
    Heart,
    Hidden,
}
mirror_enum! { BridgeRatingSystem = bae_people::people::RatingSystem,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Stars, Heart, Hidden },
}

/// What a rating system applies to: the library's albums or its tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRatedItems {
    Albums,
    Tracks,
}
mirror_enum! { BridgeRatedItems = bae_people::people::RatedItems,
    from_core: pub(crate) fn, into_core: pub(crate) fn,
    variants: { Albums, Tracks },
}

/// How the library shows the ratings of its albums and of its tracks, which
/// only an admin changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRatingSystems {
    pub albums: BridgeRatingSystem,
    pub tracks: BridgeRatingSystem,
}
mirror_struct! {
    BridgeRatingSystems = bae_people::people::RatingSystems,
    from_core: pub(crate) fn,
    fields: { albums: (BridgeRatingSystem), tracks: (BridgeRatingSystem) },
}

/// A place in a playlist beside one of its entries, named by its position
/// id: just before it or just after it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeBesideEntry {
    Before { position_id: String },
    After { position_id: String },
}

impl BridgeBesideEntry {
    pub(crate) fn into_core(self) -> Result<bae_people::collections::BesideEntry, BridgeError> {
        use bae_people::collections::BesideEntry;
        let id = |position_id: String| -> Result<_, BridgeError> {
            position_id.parse().map_err(BridgeError::config)
        };
        Ok(match self {
            Self::Before { position_id } => BesideEntry::Before(id(position_id)?),
            Self::After { position_id } => BesideEntry::After(id(position_id)?),
        })
    }
}

/// Where an insert puts new playlist entries: at the playlist's start, the
/// one place a playlist with no entries has, or beside one of its entries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInsertPlace {
    Start,
    Beside { place: BridgeBesideEntry },
}

impl BridgeInsertPlace {
    pub(crate) fn into_core(self) -> Result<bae_people::collections::InsertPlace, BridgeError> {
        use bae_people::collections::InsertPlace;
        Ok(match self {
            Self::Start => InsertPlace::Start,
            Self::Beside { place } => InsertPlace::Beside(place.into_core()?),
        })
    }
}

/// Where a move puts a playlist entry: beside another entry, or one place up
/// or down.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEntryPlace {
    Beside { place: BridgeBesideEntry },
    Up,
    Down,
}

impl BridgeEntryPlace {
    pub(crate) fn into_core(self) -> Result<bae_people::collections::EntryPlace, BridgeError> {
        use bae_people::collections::EntryPlace;
        Ok(match self {
            Self::Beside { place } => EntryPlace::Beside(place.into_core()?),
            Self::Up => EntryPlace::Up,
            Self::Down => EntryPlace::Down,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePersonProfile {
    pub id: String,
    pub name: String,
    /// The letters an avatar draws while the person has no picture.
    pub initials: String,
    pub history_sharing: BridgeHistorySharing,
    /// The version of the person's avatar, which an app caches the image
    /// `AppHandle::avatar` reads under; it moves whenever the avatar's bytes
    /// do, and when an avatar this device couldn't read becomes readable, as
    /// a cover's does. `None` without an avatar.
    pub avatar_version: Option<String>,
}
impl BridgePersonProfile {
    pub(crate) fn from_core(value: bae_people::people::PersonProfile) -> Self {
        let bae_people::people::PersonProfile {
            id,
            name,
            // Core shares history with the circle; no screen names it.
            private_circle: _,
            history_sharing,
            avatar,
        } = value;
        Self {
            id: id.0.to_string(),
            initials: bae_people::people::initials(&name),
            name,
            history_sharing: BridgeHistorySharing::from_core(history_sharing),
            avatar_version: avatar.map(|avatar| avatar.version),
        }
    }
}

/// An album's ratings as its page shows them under the library's rating
/// system for albums.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAlbumRating {
    /// *None*: the page shows no rating.
    Hidden,
    /// *Stars*: the person's stars, and who rated the album.
    Stars {
        /// The stars this person gave the album.
        mine: Option<BridgeStars>,
        /// Who rated the album: this person first, when they did, then each
        /// other person who did, by name.
        raters: Vec<BridgeRater>,
    },
    /// *Heart*: whether this person hearted the album, and who did.
    Heart {
        hearted: bool,
        /// Who has a heart on the album: this person first, when they do,
        /// then each other person who does, by name. None when nobody has
        /// a heart on the album.
        hearters: Option<Vec<BridgeHearter>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRater {
    /// This person; their stars are `BridgeAlbumRating::Stars.mine`.
    You,
    Person {
        name: String,
        stars: BridgeStars,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHearter {
    You,
    Person { name: String },
}

impl BridgeAlbumRating {
    pub(crate) fn from_core(rating: bae_views::album_detail::AlbumRating) -> Self {
        use bae_views::album_detail::{AlbumRating, Hearter, Rater};
        match rating {
            AlbumRating::Hidden => Self::Hidden,
            AlbumRating::Stars { mine, raters } => Self::Stars {
                mine: mine.map(BridgeStars::from_core),
                raters: raters
                    .into_iter()
                    .map(|rater| match rater {
                        Rater::You => BridgeRater::You,
                        Rater::Person { name, stars } => {
                            BridgeRater::Person { name, stars: BridgeStars::from_core(stars) }
                        }
                    })
                    .collect(),
            },
            AlbumRating::Heart { hearted, hearters } => Self::Heart {
                hearted,
                hearters: hearters.map(|hearters| {
                    hearters
                        .into_iter()
                        .map(|hearter| match hearter {
                            Hearter::You => BridgeHearter::You,
                            Hearter::Person { name } => BridgeHearter::Person { name },
                        })
                        .collect()
                }),
            },
        }
    }
}

/// A track's rating as its row shows it under the library's rating system
/// for tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackRating {
    /// *None*: the row shows no rating.
    Hidden,
    /// *Stars*: the stars this person gave the track.
    Stars { mine: Option<BridgeStars> },
    /// *Heart*: whether this person hearted the track.
    Heart { hearted: bool },
}

impl BridgeTrackRating {
    pub(crate) fn from_core(rating: bae_views::album_detail::TrackRating) -> Self {
        use bae_views::album_detail::TrackRating;
        match rating {
            TrackRating::Hidden => Self::Hidden,
            TrackRating::Stars(mine) => Self::Stars { mine: mine.map(BridgeStars::from_core) },
            TrackRating::Heart(hearted) => Self::Heart { hearted },
        }
    }
}

/// A person as a screen's row refers to them: who made a playlist, whose play
/// it is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePersonRef {
    pub person: String,
    pub name: String,
    /// The letters an avatar draws while the person has no picture.
    pub initials: String,
    /// The version of the person's avatar, which an app caches the image
    /// `AppHandle::avatar` reads under; `None` without an avatar.
    pub avatar_version: Option<String>,
}

impl BridgePersonRef {
    pub(crate) fn from_core(person: bae_people::people::PersonRef) -> Self {
        Self {
            person: person.person.0.to_string(),
            initials: bae_people::people::initials(&person.name),
            name: person.name,
            avatar_version: person.avatar_version,
        }
    }
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
    pub owner: BridgePersonRef,
    /// Whether this device's person made it.
    pub yours: bool,
    /// Who sees the playlist as its page header names it: the person's own
    /// playlist says so, and one someone else made says nothing, since it is
    /// shared to be seen at all.
    pub header_sharing: Option<BridgeSharing>,
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
            owner: BridgePersonRef::from_core(owner),
            yours,
            header_sharing: yours.then(|| BridgeSharing::from_core(sharing)),
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
    /// How many playlists this device holds.
    pub total: u64,
    /// How many of them are the person's own, where Add to playlist can put
    /// a track.
    pub add_target_count: u64,
    /// Whether the person has a profile, which making a playlist needs.
    pub can_create: bool,
}
impl BridgePlaylistListing {
    pub(crate) fn from_core(value: bae_views::playlists::PlaylistListing) -> Self {
        let bae_views::playlists::PlaylistListing {
            listed,
            add_targets,
            total,
            add_target_count,
            can_create,
        } = value;
        Self {
            listed: listed.into_iter().map(BridgePlaylistSummary::from_core).collect(),
            add_targets: add_targets.into_iter().map(BridgePlaylistSummary::from_core).collect(),
            total,
            add_target_count,
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
    /// The entry's number in the playlist's order, counted from 1.
    pub number: u64,
    /// Whether an entry comes before this one, so it can move up.
    pub can_move_up: bool,
    /// Whether an entry comes after this one, so it can move down.
    pub can_move_down: bool,
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
            number,
            can_move_up,
            can_move_down,
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
            number,
            can_move_up,
            can_move_down,
        }
    }
}

/// A playlist's page: the playlist and its entries in order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaylistDetail {
    pub summary: BridgePlaylistSummary,
    pub entries: Vec<BridgePlaylistEntryRow>,
    /// Whether the playlist holds no entries, so its start is the one place
    /// its page takes dropped tracks.
    pub empty: bool,
}
impl BridgePlaylistDetail {
    pub(crate) fn from_core(value: bae_views::playlists::PlaylistDetail) -> Self {
        let bae_views::playlists::PlaylistDetail { summary, entries, empty } = value;
        Self {
            summary: BridgePlaylistSummary::from_core(summary),
            entries: entries.into_iter().map(BridgePlaylistEntryRow::from_core).collect(),
            empty,
        }
    }
}

/// A calendar day: the date the playing device showed when a play began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDay {
    pub year: i32,
    /// From 1 for January.
    pub month: u8,
    pub day: u8,
}
impl BridgeDay {
    fn from_core(day: time::Date) -> Self {
        Self { year: day.year(), month: u8::from(day.month()), day: day.day() }
    }
}

/// One play as History lists it: the track, when it began, the day it was
/// played and whose it is.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeHistoryPlay {
    pub id: String,
    pub title: String,
    /// The track's credit line: its own credits, else its album's.
    pub artist_line: super::BridgeCreditLine,
    /// The cover of the release the track is on; `None` when it has none.
    pub cover: Option<BridgeImageRef>,
    /// When the play began, as Unix epoch milliseconds; the app formats it.
    pub started_at: i64,
    /// The day the play was played on, as the playing device's calendar
    /// showed it.
    pub played_on: BridgeDay,
    /// Whether this is the newest play the list shows of its day, which
    /// starts the day's heading.
    pub opens_day: bool,
    /// Whose play it is.
    pub person: BridgePersonRef,
}
impl BridgeHistoryPlay {
    pub(crate) fn from_core(value: bae_views::history::HistoryRow) -> Self {
        use bae_util::timestamp::StoredTimestamp as _;
        let bae_views::history::HistoryRow {
            id,
            title,
            artist_line,
            cover,
            started_at,
            played_on,
            opens_day,
            person,
        } = value;
        Self {
            id: id.to_string(),
            title,
            artist_line: super::BridgeCreditLine::from_core(artist_line),
            cover: cover.map(BridgeImageRef::from_core),
            started_at: started_at.unix_timestamp_millis(),
            played_on: BridgeDay::from_core(played_on),
            opens_day,
            person: BridgePersonRef::from_core(person),
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
impl From<bae_people::people::RatingSystemError> for BridgeError {
    fn from(error: bae_people::people::RatingSystemError) -> Self {
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

/// A person whose plays History's menu offers to show.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeHistoryPerson {
    /// Their member id, which History's list takes to show their plays.
    pub person: String,
    pub name: String,
    /// Whether History shows this person's plays.
    pub chosen: bool,
}
impl BridgeHistoryPerson {
    pub(crate) fn from_core(value: bae_views::history::HistoryPerson) -> Self {
        let bae_views::history::HistoryPerson { person, name, chosen } = value;
        Self { person: person.to_string(), name, chosen }
    }
}

/// Whose plays History shows, as its menu names them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHistoryShown {
    Everyone,
    Person { name: String },
}
impl BridgeHistoryShown {
    pub(crate) fn from_core(value: bae_views::history::HistoryShown) -> Self {
        match value {
            bae_views::history::HistoryShown::Everyone => Self::Everyone,
            bae_views::history::HistoryShown::Person { name } => Self::Person { name },
        }
    }
}
