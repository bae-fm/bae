//! Resolved album types.
//!
//! Only `AlbumSummary`'s projection is pure enough to own its constructor here.
//! `AlbumDetail`'s needs covers and whether storage is set up, so the album
//! view resolves it (`crate`).

use super::*;
use crate::DbAlbumAppearance;
use bae_records::{DbAlbum, DbAlbumSummary};

/// What the album page reads: the album, and the release the person chose on
/// its page.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AlbumDetailRequest {
    pub album_id: String,
    /// The release the person chose on the album's page, or `None` to show
    /// the primary release.
    pub chosen_release_id: Option<String>,
}

impl AlbumDetailRequest {
    /// The album `album_id`, showing its primary release. Only tests build a
    /// request this way: the apps send the release the person chose.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn album(album_id: String) -> Self {
        AlbumDetailRequest { album_id, chosen_release_id: None }
    }
}

/// An album with its releases, each carrying tracks, files, and gallery.
#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: DbAlbum,
    /// The album's credits in list order, each the artist it shows after
    /// merges.
    pub artists: Vec<bae_records::credits::LibraryCredit>,
    /// The album's credit line: each credit's printed name, then the join its
    /// source prints after it (the plain join where it states none).
    pub artist_line: bae_types::credits::CreditLine,
    /// The album's releases in album order: the primary release first.
    pub releases: Vec<ReleaseDetail>,
    /// The release the page shows: the one its request chose while the album
    /// holds it, else the primary release.
    pub shown_release: ReleaseDetail,
    /// The shown release's place in `releases`.
    pub shown_release_index: u32,
    /// Whether the shown release is not the primary release, so the page
    /// offers to make it the primary one.
    pub shown_release_can_become_primary: bool,
    /// Whether the album has more than one release, so its page offers a
    /// choice between them.
    pub shows_release_choices: bool,
    /// The album's lowest-ordered release: the one whose cover it shows and
    /// that plays by default.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. The version moves when the bytes do, so subscribers receive a changed
    /// field and the UI re-renders.
    pub cover: Option<ImageRef>,
    /// How the page shows the album's ratings, and who gave them.
    pub rating: AlbumRating,
}

/// An album's ratings as its page shows them under the library's rating
/// system for albums, which the album's own read supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlbumRating {
    /// *None*: the page shows no rating.
    Hidden,
    /// *Stars*: the person's stars, and who rated the album.
    Stars {
        /// The stars this person gave the album.
        mine: Option<Stars>,
        /// Who rated the album: this person first, when they did, then each
        /// other person who did, by name then id. None when nobody rated the
        /// album.
        raters: Option<Vec<Rater>>,
    },
    /// *Heart*: whether this person hearted the album, and who did.
    Heart {
        hearted: bool,
        /// Who has a heart on the album: this person first, when they do,
        /// then each other person who does, by name then id. None when
        /// nobody has a heart on the album.
        hearters: Option<Vec<Hearter>>,
    },
}

/// One person who rated an album, as the album's rating line names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rater {
    /// This person; their stars are [`AlbumRating::Stars::mine`].
    You,
    Person {
        name: String,
        stars: Stars,
    },
}

/// One person who has a heart on an album, as the album's heart line names
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hearter {
    You,
    Person { name: String },
}

impl AlbumRating {
    pub(crate) fn of(
        system: bae_people::people::RatingSystem,
        raw: bae_people::collections::AlbumRatings,
    ) -> Self {
        use bae_people::people::RatingSystem;
        match system {
            RatingSystem::Hidden => Self::Hidden,
            RatingSystem::Stars => {
                let raters =
                    raw.has_raters.then(|| {
                        raw.mine
                            .map(|_| Rater::You)
                            .into_iter()
                            .chain(raw.others.into_iter().map(|other| Rater::Person {
                                name: other.name,
                                stars: other.stars,
                            }))
                            .collect()
                    });
                Self::Stars { mine: raw.mine, raters }
            }
            RatingSystem::Heart => {
                let hearted = raw.mine.is_some();
                let hearters = raw.has_raters.then(|| {
                    hearted
                        .then_some(Hearter::You)
                        .into_iter()
                        .chain(
                            raw.others
                                .into_iter()
                                .map(|other| Hearter::Person { name: other.name }),
                        )
                        .collect()
                });
                Self::Heart { hearted, hearters }
            }
        }
    }
}

/// The slim projection a list view renders. Composed into [`AlbumDetail`] for
/// detail views.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_line: bae_types::credits::CreditLine,
    /// The album's releases in album order.
    pub release_ids: Vec<String>,
    /// The album's lowest-ordered release: the one whose cover it shows and
    /// that plays by default.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. Carried on the *summary* so a cover change updates its subscribed
    /// value: the version moves when the bytes do and the cover reloads.
    pub cover: Option<ImageRef>,
}

impl AlbumSummary {
    /// The album as its row reads it, its cover the one its primary release's
    /// cover version names.
    pub(crate) fn from_raw(raw: DbAlbumSummary) -> AlbumSummary {
        let primary_release_id = raw.primary_release_id;
        let cover = cover_ref(&primary_release_id, raw.cover_version);
        AlbumSummary {
            id: raw.id,
            title: raw.title,
            year: raw.year,
            is_compilation: raw.is_compilation,
            artist_line: raw.artist_line,
            release_ids: raw.release_ids,
            primary_release_id,
            cover,
        }
    }
}

/// One row of the album grid: an album, listed once when the grid is flat and
/// under each artist its credits show when grouped by artist.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumAppearance {
    pub album: AlbumSummary,
    /// The credit line this card shows: all credits in the flat grid, or
    /// the other credits, if any, in an artist's group.
    pub display_artist_line: Option<bae_types::credits::CreditLine>,
    /// Another device alone holds audio from the album's default release.
    pub primary_release_unplayable: bool,
}

impl AlbumAppearance {
    pub(crate) fn from_raw(raw: DbAlbumAppearance) -> AlbumAppearance {
        AlbumAppearance {
            album: AlbumSummary::from_raw(raw.album),
            display_artist_line: raw.display_artist_line,
            primary_release_unplayable: raw.primary_release_unplayable,
        }
    }
}
