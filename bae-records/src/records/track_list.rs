//! The tracks an action names, and the order the action puts them in.

use bae_store::{statement_rows, DbError};
use std::collections::BTreeSet;

// The tracks of the JSON array `tracks` that the library holds, each with its
// `place` in the order a given list puts them: the array's.
coven::fragment!(
    GivenTracks(tracks: String) { place: i64 },
    r#"
    SELECT track.id AS track_id, row_number() OVER (ORDER BY given.key) AS place
    FROM json_each(:tracks) AS given
    JOIN tracks AS track ON track.id = given.value
"#
);

// [`GivenTracks`] in the order a picked list puts them: the order their
// releases list them, by release, then stored position.
coven::fragment!(
    PickedTracks(tracks: String) { place: i64 },
    r#"
    SELECT track.id AS track_id,
           row_number() OVER (ORDER BY track.release_id, track.position, track.id) AS place
    FROM json_each(:tracks) AS picked
    JOIN tracks AS track ON track.id = picked.value
"#
);

coven::read!(
    writer HeldGivenTracks(tracks: &'p str),
    r#"
    SELECT held.track_id
    FROM @crate::records::track_list::GivenTracks(tracks: :tracks) AS held
    ORDER BY held.place
"#
);

coven::read!(
    writer HeldPickedTracks(tracks: &'p str),
    r#"
    SELECT held.track_id
    FROM @crate::records::track_list::PickedTracks(tracks: :tracks) AS held
    ORDER BY held.place
"#
);

// Every id of a picked list in its order, those the library lacks first: they
// have no place.
coven::read!(
    writer NamedPickedTracks(tracks: &'p str) { track_id: String },
    r#"
    SELECT named.value AS track_id
    FROM json_each(:tracks) AS named
    LEFT JOIN @crate::records::track_list::PickedTracks(tracks: :tracks) AS held
        ON held.track_id = named.value
    WHERE named.value IS NOT NULL
    ORDER BY held.place, named.key
"#
);

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
        sql: &impl bae_store::WriterRead,
        reading: TrackReading,
    ) -> Result<Vec<String>, DbError> {
        let ids = self.ids_json();
        let tracks = ids.as_str();
        Ok(match (self, reading) {
            (Self::Given(ids), TrackReading::Named) => ids.clone(),
            (Self::Given(_), TrackReading::Held) => {
                statement_rows(sql, &HeldGivenTracksParams { tracks })?
                    .into_iter()
                    .map(|row| row.track_id)
                    .collect()
            }
            (Self::Picked(_), TrackReading::Named) => {
                statement_rows(sql, &NamedPickedTracksParams { tracks })?
                    .into_iter()
                    .map(|row| row.track_id)
                    .collect()
            }
            (Self::Picked(_), TrackReading::Held) => {
                statement_rows(sql, &HeldPickedTracksParams { tracks })?
                    .into_iter()
                    .map(|row| row.track_id)
                    .collect()
            }
        })
    }

    /// The list's ids in its order for an action that leaves out what the
    /// person picked and the library lacks: a given list's every id, and a
    /// picked list's held ids. Each pick left out goes in a warning, since
    /// the app shows nothing of them.
    pub fn ordered_leaving_out_missing_picks(
        &self,
        sql: &impl bae_store::WriterRead,
    ) -> Result<Vec<String>, DbError> {
        let reading = match self {
            Self::Given(_) => TrackReading::Named,
            Self::Picked(_) => TrackReading::Held,
        };
        let ordered = self.ordered_on(sql, reading)?;
        if let Self::Picked(picked) = self {
            for id in picked {
                if !ordered.contains(id) {
                    tracing::warn!(track_id = %id, "picked track the library lacks left out");
                }
            }
        }
        Ok(ordered)
    }
}

#[cfg(test)]
#[path = "track_list_tests.rs"]
mod tests;
