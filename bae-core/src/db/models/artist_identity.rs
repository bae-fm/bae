//! Which library artist a credit names, as the rule every write of an
//! artist credit decides by reads it (see `db::client::artist_resolution`):
//! a library artist as a write names it, what the library holds for one
//! credit, and the conflict a person settles.

use super::*;
use crate::catalog_model::credits::ArtistCredit;
use serde::{Deserialize, Serialize};

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

impl From<DbArtist> for ExistingArtist {
    fn from(artist: DbArtist) -> Self {
        Self {
            artist_id: artist.id,
            name: artist.name,
            sort_name: artist.sort_name,
            musicbrainz_artist_id: artist.musicbrainz_artist_id,
            discogs_artist_id: artist.discogs_artist_id,
        }
    }
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

/// Two exact provider identities that currently belong to different library
/// artists. Import stops until the person chooses how this credit is used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artist '{incoming_artist_name}' has source IDs belonging to different library artists")]
pub struct ArtistIdentityConflict {
    pub incoming_artist_name: String,
    pub discogs_artist_id: String,
    pub musicbrainz_artist_id: String,
    pub discogs_artist: ExistingArtist,
    pub musicbrainz_artist: ExistingArtist,
}

/// Whether every catalog id both artists carry agrees. An absent id makes no
/// claim; two present ids of one catalog must be equal.
pub(crate) fn artist_source_ids_are_compatible(
    artist: &DbArtist,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) -> bool {
    source_ids_are_compatible(artist.discogs_artist_id.as_deref(), discogs_artist_id)
        && source_ids_are_compatible(artist.musicbrainz_artist_id.as_deref(), musicbrainz_artist_id)
}
