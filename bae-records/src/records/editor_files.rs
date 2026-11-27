//! The release editor's Files rows ([`EditorReleaseFiles`]): the release's
//! files as the edits its form makes to the CUE sheets lay them out.

// The release editor's Files rows, as the form's edits `edits` (the JSON
// object of a [`SheetEdits`](super::sheet_files::SheetEdits)) lay the
// release's files out ([`LaidFiles`](super::sheet_fragments::LaidFiles)),
// with each CUE sheet none of whose tracks stay listed by its caption, with
// the side it was on if the form still places it. Its place choices are the
// sides of the mediums the form lays the release out on
// ([`BaseMediums`](super::layout_fragments::BaseMediums)).
sqlive_compose::read!(
    EditorReleaseFiles(release: String, applied: bool, mediums: String, edits: String),
    r#"
    WITH gone AS MATERIALIZED (
        SELECT dropped.*
        FROM @crate::records::sheet_fragments::DroppedSheetTracks(
            release: :release, edits: :edits) AS dropped),
    skipped AS (
        SELECT ignored.*
        FROM @crate::records::sheet_fragments::IgnoredSheets(edits: :edits) AS ignored),
    captions AS (
        SELECT caption.*
        FROM @crate::records::sheet_fragments::SheetLineCaptions(
            release: :release, edits: :edits) AS caption),
    starting AS (
        SELECT base.*
        FROM @crate::records::layout_fragments::BaseMediums(
            release: :release, applied: :applied, mediums: :mediums) AS base),
    choices AS (
        SELECT listed.*
        FROM @crate::records::place_fragments::SheetSideChoicesJson(
            mediums: (SELECT position, :release, position, carrier, sides FROM starting))
            AS listed),
    placed AS (
        SELECT place.*
        FROM @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                             medium.side_count
                      FROM release_mediums AS medium WHERE medium.release_id = :release),
            places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                            held.track_number, held.stated_length_ms, held.position
                     FROM tracks AS held WHERE held.release_id = :release)) AS place),
    sheeted AS (
        SELECT carried.*
        FROM @crate::records::place_fragments::PlacedWithSheetSides(
            placed: (SELECT track_id, position, medium_position, side, number, side_kind,
                            side_ordinal, stated_length_ms
                     FROM placed)) AS carried),
    first_places AS (
        SELECT DISTINCT sheet_file_id, sheet_medium, sheet_side, sheet_kind, sheet_ordinal
        FROM sheeted
        WHERE sheet_file_id IS NOT NULL),
    sheets AS (
        SELECT sheet.id AS file_id,
               json_object(
                   'file_id', first.sheet_file_id, 'name', sheet.original_filename,
                   'place', json(CASE WHEN skipped.sheet_file_id IS NULL THEN json_object(
                       'medium', first.sheet_medium, 'side', first.sheet_side,
                       'kind', first.sheet_kind, 'ordinal', first.sheet_ordinal) END),
                   'bound_count', caption.bound_count,
                   'bound', json(caption.bound), 'unbound', json(caption.unbound),
                   'side_choices', json((SELECT choices FROM choices))) AS sheet
        FROM release_files AS sheet
        JOIN captions AS caption ON caption.sheet_file_id = sheet.id
        LEFT JOIN skipped ON skipped.sheet_file_id = sheet.id
        JOIN first_places AS first ON first.sheet_file_id = sheet.id
        WHERE sheet.release_id = :release
          AND NOT EXISTS (SELECT 1 FROM carved_tracks AS cut
                          WHERE cut.sheet_file_id = sheet.id
                            AND cut.track_id NOT IN (SELECT track_id FROM gone))),
    files AS (
        SELECT laid.*
        FROM @crate::records::sheet_fragments::LaidFiles(
            release: :release, edits: :edits) AS laid),
    segments AS (
        SELECT segment.*
        FROM @crate::records::sheet_fragments::LaidSegments(
            release: :release, edits: :edits) AS segment)
    SELECT @crate::records::file_fragments::ReleaseFiles(
               parent: r,
               files: (SELECT * FROM files),
               segments: (SELECT file_id FROM segments),
               carved: (SELECT cut.sheet_file_id
                        FROM release_files AS carving
                        JOIN carved_tracks AS cut ON cut.sheet_file_id = carving.id
                        WHERE carving.release_id = :release
                          AND cut.track_id NOT IN (SELECT track_id FROM gone)),
               sheets: (SELECT * FROM sheets)) AS files
    FROM releases AS r
    WHERE r.id = :release
"#
);
