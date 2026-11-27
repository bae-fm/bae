//! What the folder scans offer other reads: the entries at a set of keys, and
//! the candidate pane's files.

sqlive_compose::fragment!(
    ScanItems with keys(candidate_key BLOB) {
        watched_folder_path: Vec<u8>,
        path: crate::CandidateKey,
        generation: i64,
        kind: String,
        source_kind: String,
        name: String,
        relative_folder: crate::RelativeFolder,
        folder: Vec<u8>,
        file_root: Vec<u8>,
        scope: String,
        file_edit_revision: i64,
        grouping_key: Option<crate::CandidateKey>,
        files: String,
        parts: String,
        skipped_files: String,
        invalid_folder: Vec<u8>,
        invalid_separable: bool,
        invalid_reason: Option<String>,
        invalid_shared_name: Option<String>,
        invalid_damaged_audio: Option<i64>,
        invalid_album_title: Option<String>,
        invalid_credits: Option<String>,
        invalid_cover_path: Option<Vec<u8>>,
        invalid_cover_size: Option<i64>,
        invalid_cover_modified: Option<i64>
    },
    r#"
    SELECT candidate.watched_folder_path, candidate.path, candidate.generation,
        candidate.kind, candidate.source_kind, candidate.name, candidate.relative_folder,
        candidate.folder, candidate.file_root, candidate.scope, candidate.file_edit_revision,
        candidate.grouping_key,(
            SELECT json_group_array(json_object('relative_path', file.relative_path,
                'absolute_path', hex(file.absolute_path), 'size', file.size,
                'modified_at_ns', file.modified_at_ns, 'audio_content_type',
                file.audio_content_type, 'audio_container', file.audio_container,
                'audio_duration_ms', file.audio_duration_ms, 'audio_sample_rate_hz',
                file.audio_sample_rate_hz, 'audio_bits_per_sample',
                file.audio_bits_per_sample, 'audio_bitrate_kbps', file.audio_bitrate_kbps,
                'audio_channels', file.audio_channels, 'file_name', file.file_name, 'role',
                file.role, 'document_kind', file.document_kind, 'sheet_binding',
                file.sheet_binding, 'sheet_binding_codec', file.sheet_binding_codec,
                'sheet_place', file.sheet_place, 'sheet_medium', file.sheet_medium,
                'sheet_side', file.sheet_side, 'sheet', json((
                SELECT json_object('title', sheet.title, 'performer', sheet.performer,
                    'catalog', sheet.catalog, 'date', sheet.date, 'ripper', sheet.ripper,
                    'tracks', json((
                    SELECT json_group_array(json_object('number', track.number, 'mode',
                        track.mode, 'mode_other', track.mode_other, 'title', track.title,
                        'performer', track.performer, 'file_reference',
                        track.file_reference, 'start_cue_frames', track.start_cue_frames,
                        'end_cue_frames', track.end_cue_frames, 'generated_pregap_frames',
                        track.generated_pregap_frames, 'indexes', json((
                    SELECT json_group_array(json_object('number', cue_index.number,
                        'frames', cue_index.frames, 'file_reference',
                        cue_index.file_reference) ORDER BY cue_index.position)
                    FROM scan_cue_index AS cue_index
                    WHERE cue_index.watched_folder_path = track.watched_folder_path AND
                        cue_index.candidate_path = track.candidate_path AND
                        cue_index.sheet_relative_path = track.sheet_relative_path AND
                        cue_index.track_position = track.position
                ))) ORDER BY track.position)
                FROM scan_cue_track AS track
                WHERE track.watched_folder_path = sheet.watched_folder_path AND
                    track.candidate_path = sheet.candidate_path AND
                    track.sheet_relative_path = sheet.sheet_relative_path
            )))
            FROM scan_cue_sheet AS sheet
            WHERE sheet.watched_folder_path = file.watched_folder_path AND
                sheet.candidate_path = file.candidate_path AND sheet.sheet_relative_path =
                file.relative_path
            )), 'sheet_audio', json((
            SELECT json_group_array(json_object('file_reference', described.file_reference,
                'file_id', described.audio_relative_path) ORDER BY described.position)
            FROM scan_sheet_audio_file AS described
            WHERE described.watched_folder_path = file.watched_folder_path AND
                described.candidate_path = file.candidate_path AND
                described.sheet_relative_path = file.relative_path
        ))) ORDER BY file.position)
        FROM scan_candidate_file AS file
        WHERE file.watched_folder_path = candidate.watched_folder_path AND
            file.candidate_path = candidate.path
        ) AS files,(
        SELECT json_group_array(json_object('folder', hex(part.folder), 'prefix',
            part.prefix) ORDER BY part.position)
        FROM scan_candidate_part AS part
        WHERE part.watched_folder_path = candidate.watched_folder_path AND
            part.candidate_path = candidate.path
        ) AS parts,(
        SELECT json_group_array(json_object('relative_path', skipped.relative_path,
            'absolute_path', hex(skipped.absolute_path), 'size', skipped.size,
            'modified_at_ns', skipped.modified_at_ns, 'role', skipped.role, 'audio_damage',
            skipped.audio_damage, 'document_kind', skipped.document_kind) ORDER BY
            skipped.position)
        FROM scan_skipped_file AS skipped
        WHERE skipped.watched_folder_path = candidate.watched_folder_path AND
            skipped.candidate_path = candidate.path
    ) AS skipped_files, candidate.folder AS invalid_folder, candidate.grouping_key IS NOT
        NULL AS invalid_separable, candidate.invalid_reason AS invalid_reason,
        candidate.invalid_shared_name AS invalid_shared_name,
        candidate.invalid_damaged_audio AS invalid_damaged_audio, invalid_tags.album_title
        AS invalid_album_title, invalid_tags.credits AS invalid_credits,
        invalid_cover.absolute_path AS invalid_cover_path, invalid_cover.size AS
        invalid_cover_size, invalid_cover.modified_at_ns AS invalid_cover_modified
    FROM keys
    JOIN scan_candidate AS candidate ON candidate.path = keys.candidate_key
    LEFT JOIN scan_skipped_tags AS invalid_tags ON invalid_tags.watched_folder_path =
        candidate.watched_folder_path AND invalid_tags.candidate_path = candidate.path
        LEFT JOIN scan_skipped_file AS invalid_cover ON candidate.kind = 'invalid' AND
            invalid_cover.watched_folder_path = candidate.watched_folder_path AND
            invalid_cover.candidate_path = candidate.path AND invalid_cover.position =(
        SELECT MIN(image.position)
        FROM scan_skipped_file AS image
        WHERE image.watched_folder_path = candidate.watched_folder_path AND
            image.candidate_path = candidate.path AND image.role = 'artwork' AND
            image.relative_path = candidate.blank_cover_file
    )
"#);

sqlive_compose::fragment!(
    PaneFiles(key: crate::CandidateKey) {
        pane_files_key: crate::CandidateKey,
        pane_files: String
    },
    r#"
    WITH candidate AS (
            SELECT watched_folder_path, path
            FROM scan_candidate
            WHERE path = :key
            LIMIT 1
            ), f AS (
            SELECT file.*
            FROM scan_candidate_file AS file
            JOIN candidate ON file.watched_folder_path = candidate.watched_folder_path AND
                file.candidate_path = candidate.path
            ), sheet_tracks(relative_path, count) AS (
            SELECT track.sheet_relative_path, COUNT(*)
            FROM scan_cue_track AS track
            JOIN candidate ON track.watched_folder_path = candidate.watched_folder_path AND
                track.candidate_path = candidate.path
            WHERE track.mode = 'audio'
            GROUP BY track.sheet_relative_path
            ), carved(relative_path) AS (
            SELECT DISTINCT bound.audio_relative_path
            FROM scan_sheet_audio_file AS bound
            JOIN candidate ON bound.watched_folder_path = candidate.watched_folder_path AND
                bound.candidate_path = candidate.path
            JOIN f AS sheet ON sheet.relative_path = bound.sheet_relative_path
            JOIN sheet_tracks ON sheet_tracks.relative_path = sheet.relative_path
            WHERE sheet.sheet_binding = 'resolved' AND sheet.sheet_place = 'side'
            ), entries AS (
            SELECT f.position, f.role,(f.role = 'artwork' AND
                bae_supported_cover(f.relative_path)) AS cover, json_object('name',
                f.relative_path, 'size', f.size, 'file_name', f.file_name, 'path',
                hex(f.absolute_path), 'modified_at_ns', f.modified_at_ns, 'role', f.role,
                'content_type', f.audio_content_type, 'sample_rate_hz', f.audio_sample_rate_hz,
                'bits_per_sample', f.audio_bits_per_sample, 'bitrate_kbps',
                f.audio_bitrate_kbps, 'channels', f.audio_channels, 'sheet_tracks', CASE WHEN
                f.role = 'track_sheet' THEN COALESCE(sheet_tracks.count, 0) END) AS json
            FROM f
            LEFT JOIN sheet_tracks ON sheet_tracks.relative_path = f.relative_path
            ) SELECT :key AS pane_files_key, json_object('files', json((
            SELECT json_group_array(json(json) ORDER BY position)
            FROM entries
            )), 'cover_files', json((
            SELECT json_group_array(json(json) ORDER BY position)
            FROM entries
            WHERE cover
            )), 'images', json(NULLIF((
            SELECT json_group_array(json(json) ORDER BY position)
            FROM entries
            WHERE role = 'artwork'
            ), '[]')), 'track_count',(
            SELECT COUNT(*)
            FROM scan_candidate_unit AS unit
            JOIN candidate ON unit.watched_folder_path = candidate.watched_folder_path AND
                unit.candidate_path = candidate.path
            ), 'source_audio', json((
                WITH audio AS (
                SELECT position, layout, bae_audio_codec(content_type) AS codec, sample_rate_hz,
                    bits_per_sample, bitrate_kbps, channels, COALESCE(duration_ms, 0) AS
                    duration_ms
                    FROM (
                    SELECT f.position, CASE WHEN carved.relative_path IS NULL THEN 'file' ELSE
                        'cue' END AS layout, f.audio_content_type AS content_type,
                        f.audio_sample_rate_hz AS sample_rate_hz, f.audio_bits_per_sample AS
                        bits_per_sample, f.audio_bitrate_kbps AS bitrate_kbps, f.audio_channels
                        AS channels, f.audio_duration_ms AS duration_ms
                    FROM f
                    LEFT JOIN carved ON carved.relative_path = f.relative_path
                    WHERE f.role = 'audio'
                )
                WHERE layout IS NOT NULL AND content_type IS NOT NULL AND sample_rate_hz IS NOT
                    NULL AND channels IS NOT NULL
                ), rated AS (
                SELECT bitrate_kbps, duration_ms
                FROM audio
                WHERE bitrate_kbps IS NOT NULL
                ) SELECT CASE WHEN EXISTS (
                SELECT 1
                FROM audio
                ) THEN json_object('uniform',(
                SELECT COUNT(*)
                    FROM (
                    SELECT DISTINCT layout, codec, sample_rate_hz, bits_per_sample, channels
                    FROM audio
                )
                ) = 1, 'first', json((
                SELECT json_object('layout', layout, 'codec', codec, 'sample_rate_hz',
                    sample_rate_hz, 'bits_per_sample', bits_per_sample, 'channels', channels)
                FROM audio
                ORDER BY position
                LIMIT 1
                )), 'bitrate_kbps',(
                SELECT CASE WHEN COUNT(*) = 0 THEN NULL WHEN SUM(duration_ms) > 0 THEN
                    (SUM(bitrate_kbps * duration_ms) + SUM(duration_ms) / 2) / SUM(duration_ms)
                    ELSE (SUM(bitrate_kbps) + COUNT(*) / 2) / COUNT(*) END
                FROM rated
                ), 'layouts', json((
                SELECT json_group_array(layout ORDER BY first)
                    FROM (
                    SELECT layout, MIN(position) AS first
                    FROM audio
                    WHERE layout IS NOT NULL
                    GROUP BY layout
                )
                HAVING COUNT(*) >= 2
                )), 'codecs', json((
                SELECT json_group_array(codec ORDER BY first)
                    FROM (
                    SELECT codec, MIN(position) AS first
                    FROM audio
                    WHERE codec IS NOT NULL
                    GROUP BY codec
                )
                HAVING COUNT(*) >= 2
                )), 'sample_rates_hz', json((
                SELECT json_group_array(sample_rate_hz ORDER BY first)
                    FROM (
                    SELECT sample_rate_hz, MIN(position) AS first
                    FROM audio
                    WHERE sample_rate_hz IS NOT NULL
                    GROUP BY sample_rate_hz
                )
                HAVING COUNT(*) >= 2
                )), 'bits_per_sample', json((
                SELECT json_group_array(bits_per_sample ORDER BY first)
                    FROM (
                    SELECT bits_per_sample, MIN(position) AS first
                    FROM audio
                    WHERE bits_per_sample IS NOT NULL
                    GROUP BY bits_per_sample
                )
                HAVING COUNT(*) >= 2
                )), 'channels', json((
                SELECT json_group_array(channels ORDER BY first)
                    FROM (
                    SELECT channels, MIN(position) AS first
                    FROM audio
                    WHERE channels IS NOT NULL
                    GROUP BY channels
                )
                HAVING COUNT(*) >= 2
            ))) END
        ))) AS pane_files
"#);

// What was read from the tags of each file of a stored file-tag snapshot, in
// the files' order, each with the path of its file, as a JSON array.
sqlive_compose::fragment!(
    SnapshotFileTags on scan_candidate_tag_snapshot { tag_files: String },
    r#"
    SELECT json_group_array(json_object('relative_path', tags.relative_path,
        'absolute_path', hex(files.absolute_path),
        'file_size', tags.file_size, 'modified_at_ns', tags.modified_at_ns,
        'unparseable_reason', tags.unparseable_reason, 'title', tags.title,
        'track_artists', tags.track_artists, 'album_title', tags.album_title,
        'album_artists', tags.album_artists, 'compilation', tags.compilation,
        'years', tags.years, 'track_number', tags.track_number,
        'disc_number', tags.disc_number) ORDER BY files.position) AS tag_files
    FROM scan_candidate_file_tag AS tags
    JOIN scan_candidate_file AS files
        ON files.watched_folder_path = tags.watched_folder_path
        AND files.candidate_path = tags.candidate_path
        AND files.relative_path = tags.relative_path
    WHERE tags.watched_folder_path = @parent.watched_folder_path
        AND tags.candidate_path = @parent.candidate_path
"#
);

// The file-tag snapshot stored for each candidate in `wanted`, under the
// watched folder storing the candidate's key, each row carrying its `at`. The
// picture's bytes come with a row whose `found` is NULL, and otherwise only
// when the file the picture came from is, as it was read, one of `found`: a
// JSON array of files by relative path, size and modification time.
sqlive_compose::fragment!(
    TagSnapshot with wanted(at INTEGER, candidate_key BLOB, found TEXT) {
        snapshot_at: i64,
        snapshot_key: crate::CandidateKey,
        scan_generation: i64,
        tag_file_edit_revision: i64,
        cover_source_relative_path: Option<String>,
        cover_content_type: Option<String>,
        has_cover_data: bool,
        cover_data: Option<Vec<u8>>
    },
    r#"
    SELECT wanted.at AS snapshot_at,
        snapshot.candidate_path AS snapshot_key,
        snapshot.scan_generation,
        snapshot.file_edit_revision AS tag_file_edit_revision,
        snapshot.embedded_cover_source_relative_path AS cover_source_relative_path,
        snapshot.embedded_cover_content_type AS cover_content_type,
        snapshot.embedded_cover_data IS NOT NULL AS has_cover_data,
        CASE WHEN wanted.found IS NULL OR EXISTS (
            SELECT 1
            FROM scan_candidate_file_tag AS source
            JOIN json_each(wanted.found) AS found
                ON found.value ->> 'relative_path' = source.relative_path
                AND found.value ->> 'size' = source.file_size
                AND found.value ->> 'modified_at_ns' = source.modified_at_ns
            WHERE source.watched_folder_path = snapshot.watched_folder_path
                AND source.candidate_path = snapshot.candidate_path
                AND source.relative_path = snapshot.embedded_cover_source_relative_path
        ) THEN snapshot.embedded_cover_data END AS cover_data,
        @crate::tables::folder_scans::fragments::SnapshotFileTags(parent: snapshot)
            AS file_tags
    FROM wanted
    CROSS JOIN scan_candidate_tag_snapshot AS snapshot
        ON snapshot.watched_folder_path = (
            SELECT candidate.watched_folder_path
            FROM scan_candidate AS candidate
            WHERE candidate.path = wanted.candidate_key
            LIMIT 1
        )
        AND snapshot.candidate_path = wanted.candidate_key
"#
);

// Each stored sidecar, its files riding on it as a JSON array in their order.
sqlive_compose::fragment!(
    Sidecars {
        sidecar_folder: Vec<u8>,
        sidecar_root: Vec<u8>,
        sidecar_files: String
    },
    r#"
    SELECT sidecar.folder AS sidecar_folder,
        sidecar.watched_folder_path AS sidecar_root, sidecar.state,(
            SELECT json_group_array(json_object('relative_path', file.relative_path,
                'absolute_path', hex(file.absolute_path), 'size', file.size,
                'modified_at_ns', file.modified_at_ns, 'role', file.role,
                'audio_damage', file.audio_damage, 'document_kind', file.document_kind)
                ORDER BY file.position)
            FROM scan_sidecar_file AS file
            WHERE file.folder = sidecar.folder
        ) AS sidecar_files
    FROM scan_sidecar AS sidecar
"#
);

// The entries the scans stored that read any of the files of each coverage in
// `covered`, each with its coverage's `at`: those reading the coverage's
// folder, those reading a folder of `above` (the folders above it, up to its
// watched folder) with all below it, and those reading a folder in the range
// from `below_first` to `below_past`, which is NULL for a coverage of one
// folder alone. A coverage whose `besides` is NULL takes the settled entries,
// valid or invalid, and one that names a key takes every entry but that one.
//
// Each part is a lookup in `idx_scan_candidate_file_root` by the root and the
// folders it reads. The parts read different folders, so no entry is in two of
// them. They are separate selects in no order: joined by `OR` in one select,
// sorted by key, or told apart with `UNION`, SQLite may plan them as passes
// over every entry under the root, as it does with no statistics, and with
// those coven gathers of a large watched folder. `CROSS JOIN` keeps the
// coverages the outer loop.
sqlive_compose::fragment!(
    FolderReaders with covered(
        at INTEGER, root BLOB, folder BLOB, above TEXT, below_first BLOB, below_past BLOB,
        besides BLOB
    ) {
        reader_at: i64,
        reader_key: crate::CandidateKey
    },
    r#"
    SELECT covered.at AS reader_at, candidate.path AS reader_key
    FROM covered
    CROSS JOIN scan_candidate AS candidate
        ON candidate.watched_folder_path = covered.root
        AND candidate.file_root = covered.folder
    WHERE candidate.source_kind = 'folder'
        AND CASE WHEN covered.besides IS NULL THEN candidate.kind IN ('valid', 'invalid')
            ELSE candidate.path != covered.besides END
    UNION ALL
    SELECT covered.at, candidate.path
    FROM covered
    CROSS JOIN json_each(covered.above) AS above
    CROSS JOIN scan_candidate AS candidate
        ON candidate.watched_folder_path = covered.root
        AND candidate.file_root = unhex(above.value)
    WHERE candidate.source_kind = 'folder' AND candidate.scope = 'recursive'
        AND CASE WHEN covered.besides IS NULL THEN candidate.kind IN ('valid', 'invalid')
            ELSE candidate.path != covered.besides END
    UNION ALL
    SELECT covered.at, candidate.path
    FROM covered
    CROSS JOIN scan_candidate AS candidate
        ON candidate.watched_folder_path = covered.root
        AND candidate.file_root >= covered.below_first
        AND candidate.file_root < covered.below_past
    WHERE candidate.source_kind = 'folder'
        AND CASE WHEN covered.besides IS NULL THEN candidate.kind IN ('valid', 'invalid')
            ELSE candidate.path != covered.besides END
"#
);
