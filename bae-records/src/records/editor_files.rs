//! The release editor's Files rows ([`EditorReleaseFiles`]): the release's
//! files as the edits its form makes to the CUE sheets lay them out.

// The release editor's Files rows, as the form's edits `edits` (the JSON
// object of a [`SheetEdits`](super::sheet_files::SheetEdits)) lay the
// release's files out, with the CUE sheets none of whose tracks stay listed by
// their captions, whose place choices are the sides of the mediums the form
// lays the release out on: an applied catalog's (the JSON array `mediums`)
// where `applied`, else the release's own. Its common table expressions, in
// order:
//
// - `picked`, `ignored` and `line_files`: the audio files the edits pick for
//   `FILE` lines, the sheets they ignore, and each line of the release's
//   sheets with the file its tracks' bodies play from now;
// - `moved_lines`: the lines picked, outside the ignored sheets, onto a file
//   they do not play from now;
// - `dropped`: each track the save deletes, under an ignored sheet or a line
//   that plays no audio;
// - `kept`: each segment of the tracks that stay, on the file it then reads;
// - `added_files`: each file that deleted tracks played and no track that
//   stays plays once the picks are written, which the save adds a track for;
// - `laid_lines`, `laid_segments` and `laid_files`: each line with the file it
//   plays from once picked, each segment of the release's tracks on the file
//   it then reads (a track the edits add has one, whole file, marked
//   `added`), and the release's files with each picked file laid out as a CUE
//   image, each added track's file as a whole-file track and each file a line
//   moved off that no segment then reads as one no track plays from;
// - `captions`: each sheet's bound line count and its bound and unbound lines
//   with each one's audio options, which the stored tracks keep when the form
//   drops the tracks a line carves;
// - `choices`, `first_places` and `sheets`: the caption of each sheet none of
//   whose tracks stay, with the side it was on if the form still places it.
sqlive_compose::read!(
    EditorReleaseFiles(release: String, applied: bool, mediums: String, edits: String),
    r#"
    WITH picked AS (
        SELECT picked.value ->> 'sheet_file_id' AS sheet_file_id,
               picked.value ->> 'file_reference' AS file_reference,
               picked.value ->> 'audio_file_id' AS audio_file_id
        FROM json_each(json_extract(:edits, '$.picks')) AS picked),
    ignored AS (
        SELECT ignored.value AS sheet_file_id
        FROM json_each(json_extract(:edits, '$.ignored')) AS ignored),
    line_files AS (
        SELECT DISTINCT cut.sheet_file_id, cut.file_reference, played.file_id
        FROM carved_tracks AS cut
        JOIN tracks AS held ON held.id = cut.track_id
        JOIN audio_formats AS format ON format.track_id = cut.track_id
        JOIN audio_format_segments AS played
          ON played.audio_format_id = format.id AND played.role = 'main'
        WHERE held.release_id = :release),
    moved_lines AS (
        SELECT line.sheet_file_id, line.file_reference, line.file_id AS from_file_id,
               pick.audio_file_id AS to_file_id
        FROM line_files AS line
        JOIN picked AS pick
          ON pick.sheet_file_id = line.sheet_file_id
         AND pick.file_reference = line.file_reference
        WHERE pick.sheet_file_id NOT IN (SELECT sheet_file_id FROM ignored)
          AND pick.audio_file_id <> line.file_id),
    dropped AS (
        SELECT cut.track_id, cut.sheet_file_id, held.position
        FROM carved_tracks AS cut
        JOIN tracks AS held ON held.id = cut.track_id
        LEFT JOIN picked AS pick
               ON pick.sheet_file_id = cut.sheet_file_id
              AND pick.file_reference = cut.file_reference
        WHERE held.release_id = :release
          AND (cut.sheet_file_id IN (SELECT sheet_file_id FROM ignored)
               OR (pick.sheet_file_id IS NOT NULL AND pick.audio_file_id IS NULL))),
    kept AS (
        SELECT played.id AS segment_id, format.track_id, played.segment_index, played.role,
               COALESCE(moved.to_file_id, played.file_id) AS file_id,
               played.start_sample, played.end_sample, format.sample_rate,
               moved.to_file_id IS NOT NULL AS moved
        FROM audio_format_segments AS played
        JOIN audio_formats AS format ON format.id = played.audio_format_id
        JOIN tracks AS held ON held.id = format.track_id
        LEFT JOIN carved_tracks AS cut ON cut.track_id = format.track_id
        LEFT JOIN (SELECT sheet_file_id, from_file_id, MIN(to_file_id) AS to_file_id
                   FROM moved_lines GROUP BY sheet_file_id, from_file_id) AS moved
               ON moved.sheet_file_id = cut.sheet_file_id AND moved.from_file_id = played.file_id
        WHERE held.release_id = :release
          AND held.id NOT IN (SELECT track_id FROM dropped)),
    added_files AS (
        SELECT played.file_id, gone.sheet_file_id, MIN(gone.position) AS first_position
        FROM dropped AS gone
        JOIN audio_formats AS format ON format.track_id = gone.track_id
        JOIN audio_format_segments AS played
          ON played.audio_format_id = format.id AND played.role = 'main'
        WHERE NOT EXISTS (
            SELECT 1 FROM tracks AS stays
            JOIN audio_formats AS stays_format ON stays_format.track_id = stays.id
            JOIN audio_format_segments AS heard
              ON heard.audio_format_id = stays_format.id AND heard.role = 'main'
            LEFT JOIN carved_tracks AS cut ON cut.track_id = stays.id
            LEFT JOIN picked AS pick
                   ON pick.sheet_file_id = cut.sheet_file_id
                  AND pick.file_reference = cut.file_reference
            WHERE stays.release_id = :release
              AND stays.id NOT IN (SELECT track_id FROM dropped)
              AND COALESCE(pick.audio_file_id, heard.file_id) = played.file_id)
        GROUP BY played.file_id),
    laid_lines AS (
        SELECT line.sheet_file_id, line.file_reference,
               CASE WHEN pick.sheet_file_id IS NULL THEN line.file_id
                    ELSE pick.audio_file_id END AS file_id,
               line.file_id AS stored_file_id
        FROM line_files AS line
        LEFT JOIN picked AS pick
               ON pick.sheet_file_id = line.sheet_file_id
              AND pick.file_reference = line.file_reference),
    laid_segments AS (
        SELECT segment_id, track_id, segment_index, role, file_id, start_sample, end_sample,
               sample_rate, moved, 0 AS added
        FROM kept
        UNION ALL
        SELECT file_id, file_id, 0, 'main', file_id, 0, NULL, 1, 0, 1
        FROM added_files),
    laid_files AS (
        SELECT file.id, file.release_id, file.original_filename, file.content_type,
               file.created_at, file.file, file.role, file.document_kind,
               CASE WHEN heard.added THEN 'file'
                    WHEN file.id IN (SELECT to_file_id FROM moved_lines) THEN 'cue'
                    WHEN file.id IN (SELECT from_file_id FROM moved_lines)
                         AND heard.file_id IS NULL THEN NULL
                    ELSE file.source_audio_layout END AS source_audio_layout,
               file.source_audio_content_type, file.source_audio_container,
               file.source_audio_duration_ms, file.source_audio_sample_rate_hz,
               file.source_audio_bits_per_sample, file.source_audio_bitrate_kbps,
               file.source_audio_channels
        FROM release_files AS file
        LEFT JOIN (SELECT file_id, MAX(added) AS added FROM laid_segments GROUP BY file_id) AS heard
               ON heard.file_id = file.id
        WHERE file.release_id = :release),
    line_ends AS (
        SELECT cut.sheet_file_id, cut.file_reference,
               MAX(COALESCE(played.end_sample, played.start_sample) * 1000
                   / format.sample_rate) AS end_ms,
               MIN(held.position) AS position
        FROM carved_tracks AS cut
        JOIN tracks AS held ON held.id = cut.track_id
        JOIN audio_formats AS format ON format.track_id = cut.track_id
        JOIN audio_format_segments AS played
          ON played.audio_format_id = format.id AND played.role = 'main'
        WHERE held.release_id = :release
        GROUP BY cut.sheet_file_id, cut.file_reference),
    line_audio_options AS (
        SELECT line.sheet_file_id, line.file_reference, line.file_id,
               json_group_array(json_object(
                   'file_id', file.id, 'name', file.original_filename,
                   'content_type', file.source_audio_content_type,
                   'bound', json(CASE WHEN file.id = line.file_id THEN 'true'
                                      ELSE 'false' END),
                   'fits', json(CASE WHEN ends.end_ms <= file.source_audio_duration_ms
                                     THEN 'true'
                                     WHEN ends.end_ms > file.source_audio_duration_ms
                                     THEN 'false' END))
                   ORDER BY file.original_filename COLLATE natural_order, file.id)
                   AS options,
               ends.position
        FROM laid_lines AS line
        JOIN line_ends AS ends
          ON ends.sheet_file_id = line.sheet_file_id
         AND ends.file_reference = line.file_reference
        JOIN laid_files AS file ON file.role = 'audio'
        LEFT JOIN coven_file_reads AS readable ON readable.file = file.file
        WHERE file.id = line.file_id OR file.id = line.stored_file_id
           OR (file.source_audio_layout IS NULL AND readable.source IS NOT NULL)
        GROUP BY line.sheet_file_id, line.file_reference),
    captions AS (
        SELECT listed.sheet_file_id, COUNT(listed.file_id) AS bound_count,
               json_group_array(json_object(
                   'file_reference', listed.file_reference, 'options', json(listed.options))
                   ORDER BY listed.position) FILTER (WHERE listed.file_id IS NOT NULL) AS bound,
               CASE WHEN COUNT(*) > COUNT(listed.file_id) THEN
                   json_group_array(json_object(
                       'file_reference', listed.file_reference, 'options', json(listed.options))
                       ORDER BY listed.position) FILTER (WHERE listed.file_id IS NULL) END
                   AS unbound
        FROM line_audio_options AS listed
        GROUP BY listed.sheet_file_id),
    base_mediums AS (
        SELECT given.value ->> 'position' AS medium_id, :release AS release_id,
               given.value ->> 'position' AS position, given.value ->> 'carrier' AS medium,
               given.value ->> 'sides' AS side_count
        FROM json_each(:mediums) AS given
        WHERE :applied
        UNION ALL
        SELECT held.position, :release, held.position, held.medium, held.side_count
        FROM release_mediums AS held
        WHERE held.release_id = :release AND NOT :applied),
    choices AS MATERIALIZED (
        SELECT choice.*
        FROM @crate::records::place_fragments::SheetSideChoices(
            mediums: (SELECT * FROM base_mediums)) AS choice),
    placed AS (
        SELECT place.*
        FROM @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                             medium.side_count
                      FROM release_mediums AS medium WHERE medium.release_id = :release),
            places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                            held.track_number, held.stated_length_ms, held.position
                     FROM tracks AS held WHERE held.release_id = :release)) AS place),
    first_places AS (
        SELECT cut.sheet_file_id, place.medium_position,
               CASE WHEN place.side_kind = 'sided' THEN place.side ELSE 1 END AS side,
               CASE WHEN place.side_kind = 'sided' THEN 'sided' ELSE 'disc' END AS kind,
               CASE WHEN place.side_kind = 'sided' THEN place.side_ordinal
                    ELSE place.medium_position END AS ordinal,
               ROW_NUMBER() OVER (PARTITION BY cut.sheet_file_id ORDER BY place.position) AS n
        FROM carved_tracks AS cut
        JOIN placed AS place ON place.track_id = cut.track_id),
    sheets AS (
        SELECT sheet.id AS file_id,
               json_object(
                   'file_id', first.sheet_file_id, 'name', sheet.original_filename,
                   'place', json(CASE WHEN skipped.sheet_file_id IS NULL THEN json_object(
                       'medium', first.medium_position, 'side', first.side,
                       'kind', first.kind, 'ordinal', first.ordinal) END),
                   'bound_count', caption.bound_count,
                   'bound', json(caption.bound), 'unbound', json(caption.unbound),
                   'side_choices', json((SELECT json_group_array(json_object(
                           'medium', medium, 'side', side, 'kind', kind, 'ordinal', ordinal)
                           ORDER BY medium, side)
                       FROM choices))) AS sheet
        FROM release_files AS sheet
        JOIN captions AS caption ON caption.sheet_file_id = sheet.id
        LEFT JOIN ignored AS skipped ON skipped.sheet_file_id = sheet.id
        JOIN first_places AS first ON first.sheet_file_id = sheet.id AND first.n = 1
        WHERE sheet.release_id = :release
          AND NOT EXISTS (SELECT 1 FROM carved_tracks AS cut
                          WHERE cut.sheet_file_id = sheet.id
                            AND cut.track_id NOT IN (SELECT track_id FROM dropped)))
    SELECT @crate::records::file_fragments::ReleaseFiles(
               parent: r,
               files: (SELECT * FROM laid_files),
               segments: (SELECT file_id FROM laid_segments),
               carved: (SELECT cut.sheet_file_id
                        FROM release_files AS carving
                        JOIN carved_tracks AS cut ON cut.sheet_file_id = carving.id
                        WHERE carving.release_id = :release
                          AND cut.track_id NOT IN (SELECT track_id FROM dropped)),
               sheets: (SELECT * FROM sheets)) AS files
    FROM releases AS r
    WHERE r.id = :release
"#
);
