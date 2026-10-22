//! A credit list as it prints: every album's and track's artist line is built
//! here, from each credit's printed name and what its source prints after it.

use serde::{Deserialize, Serialize};

/// What is printed after a credit when the source states nothing: the
/// separator of a list with no joining words.
pub const DEFAULT_JOIN: &str = ", ";

/// One of an artist's other printed names (`artist_credits`): the folded name
/// that keys it, and the name as printed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreditName {
    pub key: String,
    pub name: String,
}

impl CreditName {
    /// The credit name `name` prints as, keyed by its folded form.
    pub(crate) fn of(name: &str) -> Self {
        Self {
            key: crate::text_match::artist_name_key(name),
            name: name.to_string(),
        }
    }
}

/// One of an artist's other printed names, with the artist whose row holds
/// it: the artist itself, or one merged into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistCreditName {
    pub artist_id: String,
    pub credit: CreditName,
}

/// The line a credit list prints: each credit's printed name, then its join
/// ([`DEFAULT_JOIN`] where it states none). The last credit's join is never
/// printed: nothing follows it.
pub fn credit_line<'a>(credits: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> String {
    let mut line = String::new();
    let mut pending_join: Option<&str> = None;
    for (name, join) in credits {
        if let Some(previous) = pending_join {
            line.push_str(previous);
        }
        line.push_str(name);
        pending_join = Some(join.unwrap_or(DEFAULT_JOIN));
    }
    line
}

/// A join as bae keeps it: `None` for nothing stated, so the default prints.
pub(crate) fn stated_join(join: Option<&str>) -> Option<String> {
    join.filter(|join| !join.is_empty()).map(str::to_string)
}

/// The joins of a credit list as stored: the last credit's join is dropped,
/// since nothing follows it, so removing the credit after another leaves that
/// one printing no join.
pub(crate) fn drop_last_join<T>(credits: &mut [T], join: impl Fn(&mut T) -> &mut Option<String>) {
    if let Some(last) = credits.last_mut() {
        *join(last) = None;
    }
}

/// One credit of an album or track as the library holds it: the artist it
/// names (the merge survivor), the other printed name it names if any, and
/// what follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryCredit {
    pub artist: crate::db::DbArtist,
    pub credit: Option<CreditName>,
    pub join: Option<String>,
}

impl LibraryCredit {
    /// What the credit prints: its credit name, else the artist's own name.
    pub fn printed(&self) -> &str {
        self.credit
            .as_ref()
            .map_or(self.artist.name.as_str(), |credit| credit.name.as_str())
    }
}

/// The line `credits` print.
pub fn library_line(credits: &[LibraryCredit]) -> String {
    credit_line(
        credits
            .iter()
            .map(|credit| (credit.printed(), credit.join.as_deref())),
    )
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
