//! How a surface outside bae numbers a release's tracks: a saved file's
//! track and disc tags and file name, and a Subsonic song. Such a surface
//! knows discs and track numbers, not sides, so a track's number counts the
//! tracks of its medium in playing order across the medium's sides, and its
//! disc is its medium's position, stated only for a release of several
//! mediums.

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

#[cfg(test)]
#[path = "export_numbers_tests.rs"]
mod tests;
