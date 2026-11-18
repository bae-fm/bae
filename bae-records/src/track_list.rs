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

/// Which of a list's tracks a read of it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackReading {
    /// Every track the list names, in its order. A picked track the library
    /// lacks comes first, so a write that adds the tracks refuses it as it
    /// refuses any track the library lacks.
    Named,
    /// Only the tracks the library holds, in the list's order.
    Held,
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
    pub fn track_count(&self) -> usize {
        match self {
            Self::Given(ids) => ids.len(),
            Self::Picked(ids) => ids.len(),
        }
    }

    /// The list's ids as a JSON array, for `json_each`.
    pub fn ids_json(&self) -> String {
        match self {
            Self::Given(ids) => serde_json::to_string(ids),
            Self::Picked(ids) => serde_json::to_string(ids),
        }
        .expect("track ids serialize as JSON")
    }

    /// The list's ids in its order, read in `sql`: every one it names, or
    /// only those the library holds, as `reading` says.
    pub fn ordered_on(
        &self,
        sql: &impl coven::SqlRead,
        reading: TrackReading,
    ) -> Result<Vec<String>, DbError> {
        let statement = match (self, reading) {
            (Self::Given(ids), TrackReading::Named) => return Ok(ids.clone()),
            (Self::Picked(_), TrackReading::Named) => format!(
                "SELECT j.value FROM json_each(?1) j LEFT JOIN tracks t ON t.id = j.value \
                 ORDER BY t.id IS NOT NULL, {}, j.key",
                self.order_sql()
            ),
            (_, TrackReading::Held) => format!(
                "SELECT t.id FROM json_each(?1) j JOIN tracks t ON t.id = j.value ORDER BY {}",
                self.order_sql()
            ),
        };
        Ok(sql.query(&statement, [self.ids_json()], |row| row.get(0))?)
    }
}

#[cfg(test)]
#[path = "track_list_tests.rs"]
mod tests;
