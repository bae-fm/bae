//! What a release editor's form does to the release's CUE sheets, as
//! fragments over the release `release` and the form's edits `edits`, the
//! JSON object of a [`SheetEdits`](super::sheet_files::SheetEdits): which
//! audio file each sheet's `FILE` line then plays from, which tracks a save
//! deletes and adds, and how the release's files and tracks' audio are then
//! laid out. The editor's draft, its Files rows and the save read these, so
//! they never disagree.

// The audio files the edits pick for `FILE` lines, a line that plays no audio
// with a NULL `audio_file_id`.
sqlive_compose::fragment!(
    PickedSheetLines(edits: String) {
        sheet_file_id: Option<String>,
        file_reference: Option<String>,
        audio_file_id: Option<String>
    },
    r#"
    SELECT picked.value ->> 'sheet_file_id' AS sheet_file_id,
           picked.value ->> 'file_reference' AS file_reference,
           picked.value ->> 'audio_file_id' AS audio_file_id
    FROM json_each(json_extract(:edits, '$.picks')) AS picked
"#
);

// The sheets the edits ignore.
sqlive_compose::fragment!(
    IgnoredSheets(edits: String) { sheet_file_id: Option<String> },
    r#"
    SELECT ignored.value AS sheet_file_id
    FROM json_each(json_extract(:edits, '$.ignored')) AS ignored
"#
);

// Each `FILE` line of the release's sheets with the file its tracks' bodies
// play from now.
sqlive_compose::fragment!(
    SheetLineFiles(release: String),
    r#"
    SELECT DISTINCT cut.sheet_file_id, cut.file_reference, played.file_id
    FROM carved_tracks AS cut
    JOIN tracks AS held ON held.id = cut.track_id
    JOIN audio_formats AS format ON format.track_id = cut.track_id
    JOIN audio_format_segments AS played
      ON played.audio_format_id = format.id AND played.role = 'main'
    WHERE held.release_id = :release
"#
);

// The lines the edits pick, outside the sheets they ignore, onto a file the
// lines do not play from now.
sqlive_compose::fragment!(
    MovedSheetLines(release: String, edits: String) { to_file_id: Option<String> },
    r#"
    WITH lines AS (
        SELECT line.*
        FROM @crate::records::sheet_fragments::SheetLineFiles(release: :release) AS line),
    picks AS (
        SELECT picked.*
        FROM @crate::records::sheet_fragments::PickedSheetLines(edits: :edits) AS picked),
    skipped AS (
        SELECT ignored.*
        FROM @crate::records::sheet_fragments::IgnoredSheets(edits: :edits) AS ignored)
    SELECT lines.sheet_file_id, lines.file_reference, lines.file_id AS from_file_id,
           picks.audio_file_id AS to_file_id
    FROM lines
    JOIN picks
      ON picks.sheet_file_id = lines.sheet_file_id
     AND picks.file_reference = lines.file_reference
    WHERE picks.sheet_file_id NOT IN (SELECT sheet_file_id FROM skipped)
      AND picks.audio_file_id <> lines.file_id
"#
);

// Each track the save deletes, under an ignored sheet or a line that plays no
// audio, with its sheet and its place in the release's playing order.
sqlive_compose::fragment!(
    DroppedSheetTracks(release: String, edits: String),
    r#"
    WITH picks AS (
        SELECT picked.*
        FROM @crate::records::sheet_fragments::PickedSheetLines(edits: :edits) AS picked),
    skipped AS (
        SELECT ignored.*
        FROM @crate::records::sheet_fragments::IgnoredSheets(edits: :edits) AS ignored)
    SELECT cut.track_id, cut.sheet_file_id, held.position
    FROM carved_tracks AS cut
    JOIN tracks AS held ON held.id = cut.track_id
    LEFT JOIN picks AS pick
           ON pick.sheet_file_id = cut.sheet_file_id
          AND pick.file_reference = cut.file_reference
    WHERE held.release_id = :release
      AND (cut.sheet_file_id IN (SELECT sheet_file_id FROM skipped)
           OR (pick.sheet_file_id IS NOT NULL AND pick.audio_file_id IS NULL))
"#
);

// Each file that deleted tracks played and no track that stays plays once the
// picks are written, which the save adds a track for, with the sheet and the
// place in the release of the first deleted track that played it.
sqlive_compose::fragment!(
    SheetAddedFiles(release: String, edits: String) {
        sheet_file_id: String,
        first_position: i64
    },
    r#"
    WITH gone AS MATERIALIZED (
        SELECT dropped.*
        FROM @crate::records::sheet_fragments::DroppedSheetTracks(
            release: :release, edits: :edits) AS dropped),
    picks AS (
        SELECT picked.*
        FROM @crate::records::sheet_fragments::PickedSheetLines(edits: :edits) AS picked)
    SELECT played.file_id, gone.sheet_file_id, MIN(gone.position) AS first_position
    FROM gone
    JOIN audio_formats AS format ON format.track_id = gone.track_id
    JOIN audio_format_segments AS played
      ON played.audio_format_id = format.id AND played.role = 'main'
    WHERE NOT EXISTS (
        SELECT 1 FROM tracks AS stays
        JOIN audio_formats AS stays_format ON stays_format.track_id = stays.id
        JOIN audio_format_segments AS heard
          ON heard.audio_format_id = stays_format.id AND heard.role = 'main'
        LEFT JOIN carved_tracks AS cut ON cut.track_id = stays.id
        LEFT JOIN picks AS pick
               ON pick.sheet_file_id = cut.sheet_file_id
              AND pick.file_reference = cut.file_reference
        WHERE stays.release_id = :release
          AND stays.id NOT IN (SELECT track_id FROM gone)
          AND COALESCE(pick.audio_file_id, heard.file_id) = played.file_id)
    GROUP BY played.file_id
"#
);

// Each segment of the release's tracks on the file it then reads: a segment
// of a sheet's track on a moved line's file moves to the picked one, a pregap
// at the tail of that file among them. A deleted track has none, and a track
// the edits add has one, whole file, marked `added`, whose track id is its
// file's.
sqlive_compose::fragment!(
    LaidSegments(release: String, edits: String) {
        track_id: String,
        segment_index: i64,
        role: String,
        file_id: String,
        start_sample: i64,
        end_sample: Option<i64>,
        sample_rate: i64,
        moved: bool,
        added: bool
    },
    r#"
    WITH gone AS MATERIALIZED (
        SELECT dropped.track_id
        FROM @crate::records::sheet_fragments::DroppedSheetTracks(
            release: :release, edits: :edits) AS dropped),
    moved_files AS (
        SELECT moved.sheet_file_id, moved.from_file_id, MIN(moved.to_file_id) AS to_file_id
        FROM @crate::records::sheet_fragments::MovedSheetLines(
            release: :release, edits: :edits) AS moved
        GROUP BY moved.sheet_file_id, moved.from_file_id),
    fresh AS (
        SELECT added.*
        FROM @crate::records::sheet_fragments::SheetAddedFiles(
            release: :release, edits: :edits) AS added)
    SELECT format.track_id, played.segment_index, played.role,
           COALESCE(moved.to_file_id, played.file_id) AS file_id,
           played.start_sample, played.end_sample, format.sample_rate,
           moved.to_file_id IS NOT NULL AS moved, 0 AS added
    FROM audio_format_segments AS played
    JOIN audio_formats AS format ON format.id = played.audio_format_id
    JOIN tracks AS held ON held.id = format.track_id
    LEFT JOIN carved_tracks AS cut ON cut.track_id = format.track_id
    LEFT JOIN moved_files AS moved
           ON moved.sheet_file_id = cut.sheet_file_id AND moved.from_file_id = played.file_id
    WHERE held.release_id = :release
      AND held.id NOT IN (SELECT track_id FROM gone)
    UNION ALL
    SELECT file_id, 0, 'main', file_id, 0, NULL, 1, 0, 1
    FROM fresh
"#
);

// The release's files, with the columns of `release_files`, each picked file
// laid out as a CUE image, each file a track is added for laid out as a
// whole-file track and each file a line moved off that no segment then reads
// as one no track plays from.
sqlive_compose::fragment!(
    LaidFiles(release: String, edits: String) { source_audio_layout: Option<String> },
    r#"
    WITH moved AS MATERIALIZED (
        SELECT lines.from_file_id, lines.to_file_id
        FROM @crate::records::sheet_fragments::MovedSheetLines(
            release: :release, edits: :edits) AS lines),
    heard AS MATERIALIZED (
        SELECT segment.file_id, MAX(segment.added) AS added
        FROM @crate::records::sheet_fragments::LaidSegments(
            release: :release, edits: :edits) AS segment
        GROUP BY segment.file_id)
    SELECT file.id, file.release_id, file.original_filename, file.content_type,
           file.created_at, file.file, file.role, file.document_kind,
           CASE WHEN heard.added THEN 'file'
                WHEN file.id IN (SELECT to_file_id FROM moved) THEN 'cue'
                WHEN file.id IN (SELECT from_file_id FROM moved)
                     AND heard.file_id IS NULL THEN NULL
                ELSE file.source_audio_layout END AS source_audio_layout,
           file.source_audio_content_type, file.source_audio_container,
           file.source_audio_duration_ms, file.source_audio_sample_rate_hz,
           file.source_audio_bits_per_sample, file.source_audio_bitrate_kbps,
           file.source_audio_channels
    FROM release_files AS file
    LEFT JOIN heard ON heard.file_id = file.id
    WHERE file.release_id = :release
"#
);

// Each sheet's caption: its count of lines that play audio, those lines and
// the ones that play none (NULL when every line plays audio), each as a JSON
// array of `{file_reference, options}` in the order the sheet's tracks reach
// them. A line's options are the audio files it can play from, in file name
// order, the one it plays from flagged. The stored tracks keep a line its
// options when the edits drop the tracks it carves.
sqlive_compose::fragment!(
    SheetLineCaptions(release: String, edits: String) {
        sheet_file_id: String,
        bound_count: i64,
        bound: String,
        unbound: Option<String>
    },
    r#"
    WITH lines AS (
        SELECT stored.*
        FROM @crate::records::sheet_fragments::SheetLineFiles(release: :release) AS stored),
    picks AS (
        SELECT picked.*
        FROM @crate::records::sheet_fragments::PickedSheetLines(edits: :edits) AS picked),
    files AS MATERIALIZED (
        SELECT laid.*
        FROM @crate::records::sheet_fragments::LaidFiles(
            release: :release, edits: :edits) AS laid),
    laid_lines AS (
        SELECT line.sheet_file_id, line.file_reference,
               CASE WHEN pick.sheet_file_id IS NULL THEN line.file_id
                    ELSE pick.audio_file_id END AS file_id,
               line.file_id AS stored_file_id
        FROM lines AS line
        LEFT JOIN picks AS pick
               ON pick.sheet_file_id = line.sheet_file_id
              AND pick.file_reference = line.file_reference),
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
        JOIN files AS file ON file.role = 'audio'
        LEFT JOIN coven_file_reads AS readable ON readable.file = file.file
        WHERE file.id = line.file_id OR file.id = line.stored_file_id
           OR (file.source_audio_layout IS NULL AND readable.source IS NOT NULL)
        GROUP BY line.sheet_file_id, line.file_reference)
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
    GROUP BY listed.sheet_file_id
"#
);
