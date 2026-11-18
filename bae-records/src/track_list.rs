//! The tracks an action names, and the order the action puts them in.

use bae_store::DbError;
use std::collections::BTreeSet;

/// The tracks an action names, and the order it puts them in: the order a
/// caller gives, or, for tracks a person picked, the order their release
/// lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackList {
    /// In the order given; a track named twice goes in twice.
    Given(Vec<String>),
    /// Tracks a person picked, which have no order of their own: each goes
    /// in once, in the order its release lists it (by release, then stored
    /// position).
    Picked(BTreeSet<String>),
}

/// A list in the order given.
impl From<Vec<String>> for TrackList {
    fn from(ids: Vec<String>) -> Self {
        Self::Given(ids)
    }
}

impl TrackList {
    /// How a statement orders the rows of
    /// `json_each(<ids_json>) j JOIN tracks t ON t.id = j.value` as the list
    /// puts them.
    pub fn order_sql(&self) -> &'static str {
        match self {
            Self::Given(_) => "j.key",
            Self::Picked(_) => "t.release_id, t.position, t.id",
        }
    }

    /// How many tracks the list names: a given list counts a track named
    /// twice twice, a picked list each pick once.
    pub fn len(&self) -> usize {
        match self {
            Self::Given(ids) => ids.len(),
            Self::Picked(ids) => ids.len(),
        }
    }

    /// Whether the list names no track.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The list's ids as a JSON array, for `json_each`.
    pub fn ids_json(&self) -> String {
        match self {
            Self::Given(ids) => serde_json::to_string(ids),
            Self::Picked(ids) => serde_json::to_string(ids),
        }
        .expect("track ids serialize as JSON")
    }

    /// The list's ids in its order, read in `sql`. A picked id the library
    /// lacks comes first, so a write that adds the tracks refuses it as it
    /// refuses any track the library lacks.
    pub fn ordered_on(&self, sql: &impl coven::SqlRead) -> Result<Vec<String>, DbError> {
        match self {
            Self::Given(ids) => Ok(ids.clone()),
            Self::Picked(_) => Ok(sql.query(
                "SELECT j.value FROM json_each(?1) j LEFT JOIN tracks t ON t.id = j.value \
                 ORDER BY t.id IS NOT NULL, t.release_id, t.position, t.id, j.value",
                [self.ids_json()],
                |row| row.get(0),
            )?),
        }
    }
}

#[cfg(test)]
#[path = "track_list_tests.rs"]
mod tests;
