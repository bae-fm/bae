//! The album's Credits: who did what on a release and on which of its tracks.
//!
//! A release's credits are its own and its tracks'; the Credits list them all,
//! every role. For each track they include the non-writing credits of the
//! works it performs, as that track's: the writing credits of works are the
//! works' (their composers' names), not the release's.

use bae_records::DbCreditRow;
use bae_types::credit_role::CreditedAs;
use bae_types::credits::CreditLine;

/// One row of a release's Credits: a role, the artists who share it over the
/// same tracks, and those tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditRow {
    pub role: CreditedAs,
    pub artists: CreditLine,
    /// The position text of each track the row covers, in playing order;
    /// empty for a row covering every track of the release.
    pub tracks: Vec<String>,
}

impl CreditRow {
    /// The row `raw` holds.
    pub(crate) fn from_raw(raw: DbCreditRow) -> Self {
        CreditRow {
            role: raw.role,
            artists: CreditLine::from_stored(raw.artists.iter().map(|name| (name.as_str(), None))),
            tracks: raw.tracks.iter().map(bae_types::mediums::TrackPosition::text).collect(),
        }
    }
}
