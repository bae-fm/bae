//! Who a catalog credits, and in what role.

use crate::catalog::Catalog;
use serde::{Deserialize, Serialize};

/// What a source or a person said about an artist: the name it goes by, the
/// catalog entries that name it, and the name the source printed for it where
/// that is another (a MusicBrainz credit name, a Discogs `anv`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistCredit {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
    pub credited_as: Option<String>,
}

impl ArtistCredit {
    /// A credit known only by `name`, printed as itself.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
            credited_as: None,
        }
    }

    /// The credit as its source prints it, `printed`: kept, trimmed, where it
    /// is not blank and is another name than the credit's own, the spaces
    /// around that aside.
    #[must_use]
    pub fn printed_as(mut self, printed: Option<&str>) -> Self {
        let name = self.name.trim();
        self.credited_as = printed
            .map(str::trim)
            .filter(|printed| !printed.is_empty() && *printed != name)
            .map(str::to_string);
        self
    }

    /// What the credit prints: the name its source printed, else its name.
    pub fn printed(&self) -> &str {
        self.credited_as.as_deref().unwrap_or(&self.name)
    }
}

/// One place in a credit list as a source states it: the artist, and what the
/// source prints after it (`None` where it prints nothing of its own).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceCredit {
    pub credit: ArtistCredit,
    pub join: Option<String>,
}

impl SourceCredit {
    /// `credit`, followed by the default separator: what a source that prints
    /// no joining words states.
    pub fn plain(credit: ArtistCredit) -> Self {
        Self { credit, join: None }
    }

    /// The line `credits` print, as the picker's row shows a release's
    /// artists.
    pub fn line(credits: &[Self]) -> String {
        bae_types::credits::credit_line(
            credits.iter().map(|credit| (credit.credit.printed(), credit.join.as_deref())),
        )
    }
}

/// One artist a source credits in a role: who, which catalog said so, and
/// the words it credits the role with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditedRole {
    pub artist: ArtistCredit,
    pub source: Catalog,
    pub role: Option<String>,
}
