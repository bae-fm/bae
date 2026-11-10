//! Which library artist a credit names, as the rule every write of an
//! artist credit decides by reads it (see `client::artist_resolution`):
//! a library artist as a write names it, what the library holds for one
//! credit, and the conflict a person settles.

use super::*;
use bae_catalogs::credits::ArtistCredit;
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
}

/// One credit and what it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCredit {
    pub credit: ArtistCredit,
    pub resolution: CreditResolution,
}
