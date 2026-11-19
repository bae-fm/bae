//! An artist credited on an album or track, as a draft or an edit holds it,
//! and how such a credit stands to the library when it is read.

use super::raw_release_edit::trim_to_option;
use bae_catalogs::credits::{ArtistCredit, SourceCredit};
use bae_records::ExistingArtist;
use serde::{Deserialize, Serialize};

/// One artist credited on an album or track.
///
/// Either a library artist a person picked in the artist field, linked by its
/// library id and, where they picked one, one of its other printed names; or
/// a credit: what a catalog, the files' tags, or the person's typing said. A
/// credit claims nothing about the library. Which library artist it names, if
/// any, is read against the library whenever the draft is shown (the import
/// list's artist standings), and decided for good inside the write that
/// commits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtistAssignment {
    Picked { artist: ExistingArtist, credit_name: Option<bae_records::credits::CreditName> },
    Credit { credit: ArtistCredit },
}

/// One place in an album's or track's credit list: who is credited there, and
/// what is printed after it when another credit follows (`None`: the default
/// separator).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credited {
    pub artist: ArtistAssignment,
    pub join: Option<String>,
}

impl Credited {
    /// The draft's credit for `credit`'s place: the source's credit, left to
    /// be resolved when the draft is imported.
    pub fn of_source(credit: SourceCredit) -> Self {
        Self { artist: ArtistAssignment::Credit { credit: credit.credit }, join: credit.join }
    }

    /// A credit list as stored: each credit normalized, each join kept only
    /// where it states something, and the last credit's join dropped, since
    /// nothing follows it.
    pub(crate) fn normalized_list(credits: Vec<Self>) -> Vec<Self> {
        let mut credits: Vec<Self> = credits
            .into_iter()
            .map(|credited| Self {
                artist: credited.artist.normalized(),
                join: bae_types::credits::stated_join(credited.join.as_deref()),
            })
            .collect();
        bae_types::credits::drop_last_join(&mut credits, |credited| &mut credited.join);
        credits
    }

    /// A library credit as the editor holds it: its artist picked, with the
    /// credit name and join it names.
    pub fn of_library(credit: bae_records::credits::LibraryCredit) -> Self {
        Self {
            artist: ArtistAssignment::Picked {
                artist: credit.artist.into(),
                credit_name: credit.credit,
            },
            join: credit.join,
        }
    }

    /// `artist`, followed by the default separator.
    pub fn plain(artist: ArtistAssignment) -> Self {
        Self { artist, join: None }
    }

    /// A list of `artists` that states no joins.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn plain_list(artists: impl IntoIterator<Item = ArtistAssignment>) -> Vec<Self> {
        artists.into_iter().map(Self::plain).collect()
    }
}

/// The line `credits` print (see [`bae_types::credits::CreditLine`]).
pub fn credited_line(credits: &[Credited]) -> bae_types::credits::CreditLine {
    bae_types::credits::CreditLine::from_stored(
        credits.iter().map(|credited| (credited.artist.printed(), credited.join.as_deref())),
    )
}

/// Whether a track inherits its album artists or has its own ordered credits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackArtistAssignments {
    AlbumArtists,
    Explicit(Vec<Credited>),
}

impl TrackArtistAssignments {
    /// Each credit normalized and the joins kept as stored (see
    /// [`Credited::normalized_list`]).
    pub(crate) fn normalize(&mut self) {
        if let Self::Explicit(assignments) = self {
            *assignments = Credited::normalized_list(std::mem::take(assignments));
        }
    }
}

/// How one assigned artist stands to the library — what its badge says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtistStanding {
    /// The artist is in the library: picked there, or credited to one there.
    Library,
    /// Committing creates the artist.
    New,
    /// Several library artists carry its name and no catalog id tells them
    /// apart; `choices` are the ones the person may pick.
    Choose { choices: Vec<ExistingArtist> },
}

/// How one artist field (an album's artists, or a track's own) stands to the
/// library, as the read that returns the field's rows finds it: the field's
/// badge, and each artist's, in the field's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistFieldStanding {
    pub field: ArtistsStanding,
    pub artists: Vec<ArtistStanding>,
}

/// How a whole artist field — an album's artists, or a track's — stands to
/// the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistsStanding {
    /// Every artist is in the library.
    Library,
    /// Every artist is new to it.
    New,
    /// `count` of the artists are new; the rest are in the library.
    SomeNew { count: u32 },
    /// The field's one artist could be any of `choices` library artists.
    Choose { choices: u32 },
    /// `count` of the field's artists could each be several library artists.
    SomeToChoose { count: u32 },
}

impl ArtistAssignment {
    /// A credit known only by name: what a person types.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Credit { credit: ArtistCredit::named(name) }
    }

    /// A library artist, printed by its own name.
    pub fn picked(artist: ExistingArtist) -> Self {
        Self::Picked { artist, credit_name: None }
    }

    /// What this credit prints: a picked artist's chosen credit name, else
    /// its name; a credit's printed name, else its name.
    pub fn printed(&self) -> &str {
        match self {
            Self::Picked { artist, credit_name } => credit_name
                .as_ref()
                .map_or(artist.name.as_str(), |credit_name| credit_name.name.as_str()),
            Self::Credit { credit } => credit.printed(),
        }
    }

    /// The credit name a write's link for this credit names, before the
    /// write resolves its artist: a picked artist's chosen one; for a credit,
    /// the name it prints, which the write keeps only where it is not the
    /// resolved artist's own (see `bae_records::records::artist_credit_names::settle_credit_name`).
    pub fn link_credit(&self) -> Option<bae_records::credits::CreditName> {
        match self {
            Self::Picked { credit_name, .. } => credit_name.clone(),
            Self::Credit { credit } => Some(bae_records::credits::CreditName::of(credit.printed())),
        }
    }

    /// The artist row this assignment stands for in a write: a picked library
    /// artist as itself, by its own id; a credit as a fresh row carrying what
    /// it says, which the write resolves to a library artist when it commits.
    pub fn write_row(
        &self,
        ids: &dyn coven::IdSource,
        now: time::UtcDateTime,
    ) -> bae_records::DbArtist {
        match self {
            Self::Picked { artist, .. } => bae_records::DbArtist {
                id: artist.artist_id.clone(),
                name: artist.name.clone(),
                sort_name: artist.sort_name.clone(),
                discogs_artist_id: artist.discogs_artist_id.clone(),
                musicbrainz_artist_id: artist.musicbrainz_artist_id.clone(),
                created_at: now,
            },
            Self::Credit { credit } => bae_records::DbArtist::of_credit(credit, ids, now),
        }
    }

    pub(super) fn normalized(self) -> Self {
        match self {
            Self::Picked { artist, credit_name } => Self::Picked { artist, credit_name },
            Self::Credit { credit } => {
                let name = credit.name.trim().to_string();
                Self::Credit {
                    credit: ArtistCredit {
                        sort_name: credit.sort_name.and_then(|value| trim_to_option(&value)),
                        musicbrainz_artist_id: credit
                            .musicbrainz_artist_id
                            .and_then(|value| trim_to_option(&value)),
                        discogs_artist_id: credit
                            .discogs_artist_id
                            .and_then(|value| trim_to_option(&value)),
                        credited_as: credit
                            .credited_as
                            .and_then(|value| trim_to_option(&value))
                            .filter(|printed| *printed != name),
                        name,
                    },
                }
            }
        }
    }

    /// The name the artist is shown by — what the desktop import list's
    /// filter tests a row's credits against, what identification searches,
    /// and what decides a compilation.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Picked { artist, .. } => &artist.name,
            Self::Credit { credit } => &credit.name,
        }
    }

    /// Whether an album credited to `album_artists` is a compilation: its one
    /// credit is the various-artists artist. Every write of an album's credits
    /// derives the flag from them, so it never outlives an edit of them.
    pub fn credit_a_compilation(album_artists: &[Credited]) -> bool {
        matches!(
            album_artists,
            [only] if bae_types::text_match::is_various_artists(only.artist.name())
        )
    }

    pub(crate) fn is_blank(&self) -> bool {
        match self {
            Self::Picked { artist, .. } => {
                artist.artist_id.trim().is_empty() || artist.name.trim().is_empty()
            }
            Self::Credit { credit } => credit.name.trim().is_empty(),
        }
    }
}
