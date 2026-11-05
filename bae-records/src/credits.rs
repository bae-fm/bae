//! The credits the library holds: an artist's other printed names, and an
//! album's or track's credits with the line they print.

use serde::{Deserialize, Serialize};

/// One of an artist's other printed names (`artist_credits`): the folded name
/// that keys it, and the name as printed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreditName {
    pub key: String,
    pub name: String,
}

impl CreditName {
    /// The credit name `name` prints as, keyed by its folded form.
    pub fn of(name: &str) -> Self {
        Self { key: bae_types::text_match::artist_name_key(name), name: name.to_string() }
    }
}

/// One of an artist's other printed names, with the artist whose row holds
/// it: the artist itself, or one merged into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistCreditName {
    pub artist_id: String,
    pub credit: CreditName,
}

/// One credit of an album or track as the library holds it: the artist it
/// names (the merge survivor), the other printed name it names if any, and
/// what follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryCredit {
    pub artist: crate::DbArtist,
    pub credit: Option<CreditName>,
    pub join: Option<String>,
}

impl LibraryCredit {
    /// What the credit prints: its credit name, else the artist's own name.
    pub fn printed(&self) -> &str {
        self.credit.as_ref().map_or(self.artist.name.as_str(), |credit| credit.name.as_str())
    }
}

/// The line `credits` print.
pub fn library_line(credits: &[LibraryCredit]) -> String {
    bae_types::credits::credit_line(
        credits.iter().map(|credit| (credit.printed(), credit.join.as_deref())),
    )
}
