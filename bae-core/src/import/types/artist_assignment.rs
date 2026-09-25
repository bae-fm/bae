//! An artist credited on an album or track, as a draft or an edit holds it,
//! and how such a credit stands to the library when it is read.

use super::trim_to_option;
use serde::{Deserialize, Serialize};

/// One artist credited on an album or track.
///
/// Either a library artist a person picked in the artist field, linked by its
/// library id, or a credit: what a catalog, the files' tags, or the person's
/// typing said. A credit claims nothing about the library. Which library
/// artist it names, if any, is read against the library whenever the draft is
/// shown ([`CreditResolution`]), and decided for good inside the write that
/// commits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtistAssignment {
    Picked { artist: ExistingArtist },
    Credit { credit: ArtistCredit },
}

/// One artist already in the library, with the fields an editor needs to show
/// and distinguish the selection. Candidate storage persists only `artist_id`;
/// loading the candidate resolves the rest from the canonical artist row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingArtist {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

impl From<crate::db::DbArtist> for ExistingArtist {
    fn from(artist: crate::db::DbArtist) -> Self {
        Self {
            artist_id: artist.id,
            name: artist.name,
            sort_name: artist.sort_name,
            musicbrainz_artist_id: artist.musicbrainz_artist_id,
            discogs_artist_id: artist.discogs_artist_id,
        }
    }
}

/// What a source or a person said about an artist: the name it goes by, and
/// the catalog entries that name it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistCredit {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

/// What the library holds for one credit, as it stands when read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditResolution {
    /// The credit names this library artist.
    Library { artist: ExistingArtist },
    /// No library artist is this one: committing creates it.
    New,
    /// Several library artists carry the credit's name and no catalog id tells
    /// them apart. Committing creates a new artist unless the person picks one.
    Ambiguous { artists: Vec<ExistingArtist> },
    /// The credit's catalog ids point at library artists that disagree with
    /// it. Committing fails until the person picks one, or merges them.
    Conflicting { artists: Vec<ExistingArtist> },
}

/// One credit and what it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCredit {
    pub credit: ArtistCredit,
    pub resolution: CreditResolution,
}

/// How one assigned artist stands to the library — what its badge says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtistStanding {
    /// The artist is in the library: picked there, or credited to one there.
    Library,
    /// Committing creates the artist.
    New,
    /// Several library artists could be this one; `choices` are the ones the
    /// person may pick.
    Choose { choices: Vec<ExistingArtist> },
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
        Self::Credit {
            credit: ArtistCredit {
                name: name.into(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
        }
    }

    pub fn picked(artist: ExistingArtist) -> Self {
        Self::Picked { artist }
    }

    /// How this artist stands to the library, read from `resolutions`: a
    /// picked artist is in it; a credit stands as it resolved. `None` for a
    /// credit `resolutions` has no answer for.
    pub fn standing(&self, resolutions: &[ResolvedCredit]) -> Option<ArtistStanding> {
        let credit = match self {
            Self::Picked { .. } => return Some(ArtistStanding::Library),
            Self::Credit { credit } => credit,
        };
        let resolved = resolutions
            .iter()
            .find(|resolved| resolved.credit == *credit)?;
        Some(match &resolved.resolution {
            CreditResolution::Library { .. } => ArtistStanding::Library,
            CreditResolution::New => ArtistStanding::New,
            CreditResolution::Ambiguous { artists } | CreditResolution::Conflicting { artists } => {
                ArtistStanding::Choose {
                    choices: artists.clone(),
                }
            }
        })
    }

    /// The artist row this assignment stands for in a write: a picked library
    /// artist as itself, by its own id; a credit as a fresh row carrying what
    /// it says, which the write resolves to a library artist when it commits.
    pub(crate) fn write_row(
        &self,
        ids: &dyn coven::IdProvider,
        now: chrono::DateTime<chrono::Utc>,
    ) -> crate::db::DbArtist {
        match self {
            Self::Picked { artist } => crate::db::DbArtist {
                id: artist.artist_id.clone(),
                name: artist.name.clone(),
                sort_name: artist.sort_name.clone(),
                discogs_artist_id: artist.discogs_artist_id.clone(),
                musicbrainz_artist_id: artist.musicbrainz_artist_id.clone(),
                created_at: now,
            },
            Self::Credit { credit } => crate::db::DbArtist {
                id: ids.new_id(),
                name: credit.name.clone(),
                sort_name: credit.sort_name.clone(),
                discogs_artist_id: credit.discogs_artist_id.clone(),
                musicbrainz_artist_id: credit.musicbrainz_artist_id.clone(),
                created_at: now,
            },
        }
    }

    pub(super) fn normalized(self) -> Self {
        match self {
            Self::Picked { artist } => Self::Picked { artist },
            Self::Credit { credit } => Self::Credit {
                credit: ArtistCredit {
                    name: credit.name.trim().to_string(),
                    sort_name: credit.sort_name.and_then(|value| trim_to_option(&value)),
                    musicbrainz_artist_id: credit
                        .musicbrainz_artist_id
                        .and_then(|value| trim_to_option(&value)),
                    discogs_artist_id: credit
                        .discogs_artist_id
                        .and_then(|value| trim_to_option(&value)),
                },
            },
        }
    }

    /// The name the artist is shown by — what the desktop import list's
    /// filter tests a row's credits against, and what identification searches.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Picked { artist } => &artist.name,
            Self::Credit { credit } => &credit.name,
        }
    }

    /// The credit this assignment leaves to be resolved, if it is one.
    pub fn credit(&self) -> Option<&ArtistCredit> {
        match self {
            Self::Picked { .. } => None,
            Self::Credit { credit } => Some(credit),
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        match self {
            Self::Picked { artist } => {
                artist.artist_id.trim().is_empty() || artist.name.trim().is_empty()
            }
            Self::Credit { credit } => credit.name.trim().is_empty(),
        }
    }
}

/// How the artist field holding `assignments` stands to the library, read
/// from `resolutions`. `None` for an empty field, or one with a credit
/// `resolutions` has no answer for.
pub fn artists_standing(
    assignments: &[ArtistAssignment],
    resolutions: &[ResolvedCredit],
) -> Option<ArtistsStanding> {
    let standings = assignments
        .iter()
        .map(|assignment| assignment.standing(resolutions))
        .collect::<Option<Vec<_>>>()?;
    let count = |wanted: fn(&ArtistStanding) -> bool| {
        u32::try_from(standings.iter().filter(|standing| wanted(standing)).count())
            .expect("an artist field holds fewer than 2^32 artists")
    };
    let to_choose = count(|standing| matches!(standing, ArtistStanding::Choose { .. }));
    let new = count(|standing| matches!(standing, ArtistStanding::New));
    Some(match standings.as_slice() {
        [] => return None,
        [ArtistStanding::Choose { choices }] => ArtistsStanding::Choose {
            choices: u32::try_from(choices.len())
                .expect("a name is shared by fewer than 2^32 artists"),
        },
        _ if to_choose > 0 => ArtistsStanding::SomeToChoose { count: to_choose },
        all if new as usize == all.len() => ArtistsStanding::New,
        _ if new == 0 => ArtistsStanding::Library,
        _ => ArtistsStanding::SomeNew { count: new },
    })
}

#[cfg(test)]
#[path = "artist_assignment_tests.rs"]
mod tests;
