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
