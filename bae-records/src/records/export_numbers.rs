//! How a surface outside bae numbers a release's tracks: a saved file's
//! track and disc tags and file name, and a Subsonic song. Such a surface
//! knows discs and track numbers, not sides, so a track's number counts the
//! tracks of its medium in playing order across the medium's sides, and its
//! disc is its medium's position, stated only for a release of several
//! mediums.

use super::*;

// Each track of one release, whose mediums and tracks' places the caller
// gives, with its number among its medium's tracks in playing order and, for
// a release of several mediums, its medium's position as its disc.
sqlive_compose::fragment!(
    ExportNumbers
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ), places(
        id TEXT, release_id TEXT, medium_id TEXT, side INTEGER, track_number INTEGER,
        stated_length_ms INTEGER, position INTEGER
    ) {
        number: u32,
        disc: Option<u32>
    },
    r#"
    SELECT place.*,
           ROW_NUMBER() OVER (PARTITION BY place.medium_position ORDER BY place.position)
               AS number,
           CASE WHEN (SELECT COUNT(*) FROM mediums) > 1 THEN place.medium_position END
               AS disc
    FROM @crate::records::place_fragments::TrackPlaces(
        mediums: (SELECT * FROM mediums),
        places: (SELECT * FROM places)) AS place
"#
);

// A track's row, as a numbered track holds it.
sqlive_compose::fragment!(
    NumberedTrackRow {
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        duration_ms: u64,
        stated_length_ms: Option<u64>,
    },
    r#"
    SELECT track.id, track.release_id, track.title, track.medium_id, track.side,
           track.track_number, track.duration_ms, track.stated_length_ms, track.created_at,
           track.position
    FROM tracks AS track
"#
);

// Each track of one release, in playing order, with its numbers.
sqlive_compose::read!(
    writer NumberedTracksOfRelease(release: String),
    r#"
    SELECT track.*, numbered.*
    FROM @crate::records::export_numbers::ExportNumbers(
             mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                              medium.side_count
                       FROM release_mediums AS medium WHERE medium.release_id = :release),
             places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                             held.track_number, held.stated_length_ms, held.position
                      FROM tracks AS held WHERE held.release_id = :release)) AS numbered
    JOIN @crate::records::export_numbers::NumberedTrackRow AS track ON track.id = numbered.track_id
    WHERE track.release_id = :release
    ORDER BY track.position, track.id
"#
);

// One track with its numbers; no row for a track the library does not hold.
sqlive_compose::read!(
    writer OneNumberedTrack(track: String),
    r#"
    SELECT track.*, numbered.*
    FROM @crate::records::export_numbers::ExportNumbers(
             mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                              medium.side_count
                       FROM tracks AS asked
                       JOIN release_mediums AS medium ON medium.release_id = asked.release_id
                       WHERE asked.id = :track),
             places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                             held.track_number, held.stated_length_ms, held.position
                      FROM tracks AS asked
                      JOIN tracks AS held ON held.release_id = asked.release_id
                      WHERE asked.id = :track)) AS numbered
    JOIN @crate::records::export_numbers::NumberedTrackRow AS track ON track.id = numbered.track_id
    WHERE track.id = :track
"#
);

fn numbered_track(
    track: NumberedTrackRow,
    numbered: ExportNumbers,
) -> Result<DbNumberedTrack, DbError> {
    let created_at = bae_util::timestamp::parse_rfc3339(&track.created_at)
        .map_err(|_unparsed| DbError::unreadable("a track's created_at", &track.created_at))?;
    Ok(DbNumberedTrack {
        track: DbTrack {
            id: track.id,
            release_id: track.release_id,
            title: track.title,
            medium_id: track.medium_id,
            side: track.side,
            track_number: track.track_number,
            duration_ms: track.duration_ms,
            stated_length_ms: track.stated_length_ms,
            created_at,
        },
        number: numbered.number,
        disc: numbered.disc,
    })
}

/// Each track of `release_id` in playing order, with how a surface outside
/// bae numbers it.
pub(super) fn numbered_tracks_of_release_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Vec<DbNumberedTrack>, DbError> {
    let statement = NumberedTracksOfReleaseParams { release: release_id.to_owned() };
    bae_store::statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| numbered_track(row.track, row.numbered))
        .collect()
}

/// The track `track_id` with how a surface outside bae numbers it, or `None`
/// when the library holds no such track.
pub(super) fn numbered_track_on(
    sql: &impl bae_store::WriterRead,
    track_id: &str,
) -> Result<Option<DbNumberedTrack>, DbError> {
    let statement = OneNumberedTrackParams { track: track_id.to_owned() };
    bae_store::statement_optional(sql, &statement)?
        .map(|row| numbered_track(row.track, row.numbered))
        .transpose()
}

#[cfg(test)]
#[path = "export_numbers_tests.rs"]
mod tests;
