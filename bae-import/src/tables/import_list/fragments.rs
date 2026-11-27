//! What the import list offers the candidate pane: where the list places a candidate, what it can do, the mapping of its files to tracks, and its album artists.

sqlive_compose::fragment!(
    Listed(key: crate::CandidateKey) {
        listed_key: crate::CandidateKey,
        tab: String,
        import_failed: Option<bool>,
        actionable: Option<bool>,
        pending_state: Option<String>,
        needs_you_reason: Option<String>,
        reason_pressings: Option<i64>,
        error_detail: Option<String>,
        can_import: bool,
        can_cancel_import: bool,
        can_identify: bool,
        can_cancel_identification: bool,
        can_retry_identification: bool,
        can_reset_to_file_metadata: bool,
        can_clear_metadata: bool,
        can_combine: bool,
        can_separate: bool,
        can_skip: bool,
        can_restore: bool,
        can_reveal_folder: bool,
        part_count: i64
    },
    r#"
    WITH statement(tab, import_failed, actionable, pending_state, needs_you_reason, reason_pressings, error_detail, can_import, can_cancel_import, can_identify, can_cancel_identification, can_retry_identification, can_reset_to_file_metadata, can_clear_metadata, can_combine, can_separate, can_skip, can_restore, can_reveal_folder, part_count) AS (
            SELECT p.tab, p.import_failed, p.actionable, p.pending_state, p.needs_you_reason,
                p.reason_pressings, p.error_detail, COALESCE((p.item = 'candidate' AND
                p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab = 'pending' AND
                ((COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0) AND st.identification = 'reading_pick' AND NOT
                st.import_asked) OR (NOT COALESCE(st.identification IN ('queued', 'running',
                'finalizing', 'reading_pick'), 0) AND p.draft_valid))), 0) AS can_import,
                COALESCE((p.item = 'candidate' AND p.actionable AND st.import_step IN ('queued',
                'validating_source_files', 'fetching_artist_pictures', 'reading_files',
                'measuring_loudness')), 0) AS can_cancel_import, COALESCE((p.item = 'candidate'
                AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab = 'pending' AND
                NOT COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0) AND p.lookup IS NULL AND COALESCE(st.identification <>
                'inapplicable', 1)), 0) AS can_identify, COALESCE((p.item = 'candidate' AND
                p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab = 'pending' AND
                COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0)), 0) AS can_cancel_identification, COALESCE((p.item =
                'candidate' AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab =
                'pending' AND NOT COALESCE(st.identification IN ('queued', 'running',
                'finalizing', 'reading_pick'), 0) AND (p.lookup = 'failed' OR st.identification
                = 'not_written')), 0) AS can_retry_identification, COALESCE((p.item = 'candidate'
                AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab = 'pending' AND
                NOT COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0)), 0) AS can_reset_to_file_metadata, COALESCE((p.item =
                'candidate' AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab =
                'pending' AND NOT COALESCE(st.identification IN ('queued', 'running',
                'finalizing', 'reading_pick'), 0)), 0) AS can_clear_metadata, COALESCE((p.item =
                'candidate' AND NOT (p.tab = 'done' OR st.import_step IS NOT NULL OR
                COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0)) AND NOT p.separable AND p.actionable), 0) AS can_combine,
                COALESCE((p.item = 'candidate' AND NOT (p.tab = 'done' OR st.import_step IS NOT
                NULL OR COALESCE(st.identification IN ('queued', 'running', 'finalizing',
                'reading_pick'), 0)) AND p.separable), 0) AS can_separate, COALESCE((p.item =
                'candidate' AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab =
                'pending' AND NOT p.import_failed), 0) AS can_skip, COALESCE((p.item =
                'candidate' AND p.actionable AND NOT (st.import_step IS NOT NULL) AND p.tab =
                'skipped'), 0) AS can_restore, COALESCE((p.item = 'candidate'), 0) AS
                can_reveal_folder,(
            SELECT COUNT(*)
            FROM @crate::tables::folder_scans::row_fragments::CandidatePartRows(key: :key) AS part
        ) AS part_count
        FROM import_list_placement AS p
        LEFT JOIN @crate::tables::import_state::fragments::Standing(
            keys: (SELECT :key AS candidate_key)) AS st ON st.standing_key = p.key
        WHERE p.key = :key AND p.item = 'candidate'
    )
    SELECT :key AS listed_key, tab, import_failed, actionable, pending_state, needs_you_reason, reason_pressings, error_detail, can_import, can_cancel_import, can_identify, can_cancel_identification, can_retry_identification, can_reset_to_file_metadata, can_clear_metadata, can_combine, can_separate, can_skip, can_restore, can_reveal_folder, part_count
    FROM statement
"#);

// Each draft track with the place among the folder's units of the one it
// plays: the unit of the same kind, file, sheet and slice. The place is NULL
// where the folder has no such unit, which the reads refuse.
sqlive_compose::fragment!(
    DraftUnits(key: crate::CandidateKey) {
        draft_track_id: String,
        draft_unit_position: Option<i64>
    },
    r#"
    WITH matched(draft_track_id, draft_unit_position) AS (
    SELECT t.track_id, u.position
    FROM @crate::tables::import_state::row_fragments::DraftTrackRows(key: :key) AS t
    LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateUnits(key: :key) AS u ON
        u.file_kind IS t.file_kind AND u.file_id IS t.file_id AND u.sheet_id IS t.sheet_id
        AND u.slice_index IS t.slice_index
    WHERE t.content_hash =(
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
        LIMIT 1
    )
    )
    SELECT draft_track_id, draft_unit_position
    FROM matched
"#);

// The folder's audio files that no draft track reads, which an import of the
// draft leaves out: of the files the folder's units read, those that no unit a
// draft track plays reads. A unit reads the file it plays from, and a CUE
// track also the file its pregap is in. The pane names these files and the
// import writes none of them.
sqlive_compose::fragment!(
    LeftOutAudio(key: crate::CandidateKey) {
        relative_path: String,
        position: i64
    },
    r#"
    WITH unit_reads(position, file_id) AS (
    SELECT whole.position, whole.file_id
    FROM @crate::tables::folder_scans::row_fragments::CandidateUnits(key: :key) AS whole
    UNION ALL
    SELECT gap.position, gap.pregap_file_id
    FROM @crate::tables::folder_scans::row_fragments::CandidateUnits(key: :key) AS gap
    WHERE gap.pregap_file_id IS NOT NULL
    ), folder_files(relative_path, position) AS (
    SELECT file.relative_path, file.position
    FROM @crate::tables::folder_scans::row_fragments::CandidateFiles(key: :key) AS file
    )
    SELECT relative_path, position
    FROM folder_files
    WHERE relative_path IN (
        SELECT file_id
        FROM unit_reads
    ) AND relative_path NOT IN (
        SELECT taken.file_id
        FROM unit_reads AS taken
        JOIN @crate::tables::import_list::fragments::DraftUnits(key: :key) AS played ON
            played.draft_unit_position = taken.position
    )
"#);

sqlive_compose::fragment!(
    Mapping(key: crate::CandidateKey) {
        mapping_key: crate::CandidateKey,
        mapping: Option<String>
    },
    r#"
    SELECT :key AS mapping_key, (
            WITH artist_field AS MATERIALIZED (
            SELECT part, track_id, field
            FROM @bae_records::records::artist_standing::ArtistFieldStanding(
                raw: @crate::tables::import_state::row_fragments::DraftCreditRows(key: :key),
                credit_answer:
                    @crate::tables::credit_resolutions::row_fragments::StoredCreditAnswers
                ) AS standing
            ), candidate AS (
            SELECT scan.content_hash
            FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
            LIMIT 1
            ), f AS (
            SELECT listed.*
            FROM @crate::tables::folder_scans::row_fragments::CandidateFiles(key: :key) AS listed
            ), audio_json(relative_path, json) AS (
            SELECT relative_path, json_object('file_id', relative_path, 'name', file_name,
                'path', hex(absolute_path))
            FROM f
            WHERE role = 'audio'
            ), medium AS (
            SELECT m.id, m.release_id, m.position, m.medium, m.side_count, COALESCE(m.medium IN
                ('vinyl', 'shellac', 'acetate', 'cassette', 'dcc', 'edison_disc', 'pathe_disc'),
                0) AS sided, COALESCE(SUM(CASE WHEN COALESCE(m.medium IN ('vinyl', 'shellac',
                'acetate', 'cassette', 'dcc', 'edison_disc', 'pathe_disc'), 0) THEN m.side_count
                ELSE 0 END) OVER (PARTITION BY m.release_id ORDER BY m.position ROWS BETWEEN
                UNBOUNDED PRECEDING AND 1 PRECEDING), 0) AS sides_before, COUNT(*) OVER
                (PARTITION BY m.release_id) AS medium_count
                FROM (
                SELECT position AS id, content_hash AS release_id, position, medium, side_count
                FROM @crate::tables::import_state::row_fragments::DraftMediumRows(key: :key) AS drafted
            ) m
                WHERE m.release_id =(
                SELECT content_hash
                FROM candidate
            )
            ), place AS (
            SELECT p.id, p.release_id, p.position, p.medium_id, p.side, p.track_number AS
                number, p.stated_length_ms, m.position AS medium_position, m.medium AS carrier,
                CASE WHEN m.sided THEN 'sided' WHEN m.medium_count > 1 THEN 'disc' ELSE 'flat'
                END AS side_kind, CASE WHEN m.sided THEN m.sides_before + p.side WHEN
                m.medium_count > 1 THEN m.position ELSE 0 END AS side_ordinal
                FROM (
                SELECT track_id AS id, content_hash AS release_id, medium AS medium_id, side,
                    track_number, stated_length_ms, position
                FROM @crate::tables::import_state::row_fragments::DraftTrackRows(key: :key) AS drafted
            ) p
            JOIN medium m ON m.id = p.medium_id AND m.release_id = p.release_id
            ), rows AS (
                SELECT t.position, t.title, t.track_id, t.track_json AS track_json, p.side_kind, p.side_ordinal, p.number, u.file_kind,
                u.sheet_id, u.file_id AS unit_file_id, u.slice_index, u.duration_ms AS unit_ms,
                u.position AS unit_position, p.medium_position, p.side AS medium_side,
                u.start_sample, u.end_sample, a.json AS audio, ct.start_cue_frames * 1000 / 75
                AS cue_start_ms, CASE WHEN t.stated_length_ms IS NOT NULL AND u.duration_ms IS
                NOT NULL AND bae_lengths_disagree(u.duration_ms, t.stated_length_ms) THEN
                t.stated_length_ms END AS listed_ms
            FROM @crate::tables::import_state::row_fragments::DraftTrackRows(key: :key) AS t
            JOIN place AS p ON p.id = t.track_id AND p.release_id = t.content_hash
                LEFT JOIN @crate::tables::import_list::fragments::DraftUnits(key: :key) AS matched
                ON matched.draft_track_id = t.track_id
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateUnits(key: :key) AS u ON
                u.position = matched.draft_unit_position
            LEFT JOIN audio_json AS a ON a.relative_path = u.file_id
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateCueTracks(key: :key) AS ct ON
                ct.sheet_relative_path = u.sheet_id AND ct.position = u.cue_track_position
                WHERE t.content_hash =(
                SELECT content_hash
                FROM candidate
            )
            ), marked AS (
            SELECT *, side_kind IS NOT LAG(side_kind) OVER w OR side_ordinal IS NOT
                LAG(side_ordinal) OVER w AS side_start, side_kind IS NOT LAG(side_kind) OVER w
                OR side_ordinal IS NOT LAG(side_ordinal) OVER w OR sheet_id IS NOT LAG(sheet_id)
                OVER w AS run_start
            FROM rows WINDOW w AS (ORDER BY position)
            ), numbered AS (
            SELECT *, SUM(side_start) OVER (ORDER BY position) AS side_no, SUM(run_start) OVER
                (ORDER BY position) AS run_no
            FROM marked
            ), refs(sheet, file_reference, seq) AS (
            SELECT referenced.*
            FROM @crate::tables::folder_scans::row_fragments::CandidateSheetReferences(key: :key) AS referenced
            ), side_choice_rows AS (
            SELECT m.position AS medium, side.value AS side, CASE WHEN m.sided THEN 'sided' ELSE
                'disc' END AS kind, CASE WHEN m.sided THEN m.sides_before + side.value ELSE
                m.position END AS ordinal
                FROM medium m
                JOIN
                    json_each('[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]')
                    AS side ON side.value = 1 OR (m.sided AND side.value <= m.side_count)
                ORDER BY m.position, side.value
            ), side_choices AS (
            SELECT json_group_array(json_object('medium', c.medium, 'side', c.side, 'kind',
                c.kind, 'ordinal', c.ordinal) ORDER BY c.medium, c.side) AS json
            FROM side_choice_rows AS c
            ), reference_json(sheet, seq, bound, json) AS (
                SELECT r.sheet, r.seq, sa.audio_relative_path IS NOT NULL,
                    json_object('file_reference', r.file_reference, 'file_id',
                    sa.audio_relative_path, 'options', json((
                SELECT json_group_array(json_object('file_id', o.audio_relative_path, 'offer',
                    o.offer, 'codec', o.codec) ORDER BY fo.position)
                FROM @crate::tables::folder_scans::row_fragments::CandidateSheetOffers(key: :key) AS o
                JOIN f AS fo ON fo.relative_path = o.audio_relative_path
                    WHERE o.sheet_relative_path = r.sheet AND
                    o.file_reference = r.file_reference
            )))
            FROM refs AS r
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateSheetAudio(key: :key) AS sa ON
                    sa.sheet_relative_path = r.sheet AND
                sa.file_reference = r.file_reference
            ), sheet_audio_counts(sheet, count) AS (
            SELECT sheet_relative_path, COUNT(*)
            FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio(key: :key) AS counted
            GROUP BY sheet_relative_path
            ), sheets(relative_path, json) AS (
                SELECT s.relative_path, json_object('sheet_id', s.relative_path, 'name',
                    s.relative_path, 'bound', CASE s.sheet_binding WHEN 'resolved' THEN CASE
                    COALESCE(n.count, 0) WHEN 0 THEN json_object('binding', 'resolved_to_none')
                    WHEN 1 THEN json_object('binding', 'describes', 'audio', json((
                SELECT a.json
                FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio(key: :key) AS sa
                JOIN audio_json AS a ON a.relative_path = sa.audio_relative_path
                    WHERE sa.sheet_relative_path = s.relative_path
                ))) ELSE json_object('binding', 'describes_files', 'audio_count', n.count) END
                    WHEN 'unresolved' THEN json_object('binding', 'unresolved', 'requested',
                    json((
                SELECT json_group_array(r.file_reference ORDER BY r.seq)
                FROM refs AS r
                    WHERE r.sheet = s.relative_path AND NOT EXISTS (
                    SELECT 1
                    FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio(key: :key) AS sa
                        WHERE sa.sheet_relative_path = s.relative_path AND sa.file_reference = r.file_reference
                )
                ))) WHEN 'refused_codec' THEN json_object('binding', 'refused_codec', 'codec',
                    s.sheet_binding_codec) ELSE json_object('binding', s.sheet_binding) END,
                    'place', s.sheet_place, 'assigned', json((
                SELECT json_object('medium', c.medium, 'side', c.side, 'kind', c.kind,
                    'ordinal', c.ordinal)
                FROM (
                    SELECT medium_position, medium_side
                    FROM rows
                    WHERE rows.sheet_id = s.relative_path
                    ORDER BY position
                    LIMIT 1
                ) AS first_track
                JOIN side_choice_rows AS c ON c.medium = first_track.medium_position AND c.side =
                    CASE WHEN c.kind = 'sided' THEN first_track.medium_side ELSE 1 END
                )), 'bound_audio', json((
                SELECT CASE COUNT(*) WHEN 0 THEN json_object('kind', 'none') WHEN 1 THEN
                    json_object('kind', 'one', 'reference', json(MIN(r.json))) ELSE
                    json_object('kind', 'several', 'references', json_group_array(json(r.json)
                    ORDER BY r.seq)) END
                FROM reference_json AS r
                WHERE r.sheet = s.relative_path AND r.bound
                )), 'unbound_references', CASE WHEN EXISTS (
                SELECT 1
                FROM reference_json AS r
                WHERE r.sheet = s.relative_path AND NOT r.bound
                ) THEN json((
                SELECT json_group_array(json(r.json) ORDER BY r.seq)
                FROM reference_json AS r
                WHERE r.sheet = s.relative_path AND NOT r.bound
                )) END, 'side_choices', CASE WHEN s.sheet_binding = 'resolved' THEN json((
                SELECT json
                FROM side_choices
            )) ELSE json('[]') END)
            FROM f AS s
            LEFT JOIN sheet_audio_counts AS n ON n.sheet = s.relative_path
            WHERE s.role = 'track_sheet'
            ), runs AS (
                SELECT side_no, run_no, MIN(side_kind) AS side_kind, MIN(side_ordinal) AS
                    side_ordinal, json_object('sheet', json((
                SELECT json
                FROM sheets
                WHERE relative_path = n.sheet_id
                )), 'rows', json_group_array(json_object('track', json(track_json), 'artists', json((
                SELECT field
                FROM artist_field
                WHERE part = 1 AND track_id = n.track_id
            )), 'number', number, 'listed_ms', listed_ms, 'unit', CASE WHEN file_kind IS NOT
                NULL THEN json_object('position', unit_position, 'audio', json(audio),
                'length_ms', unit_ms, 'source', CASE
                file_kind WHEN 'sheet_slice' THEN json_object('kind', file_kind, 'start_ms',
                cue_start_ms, 'start', start_sample, 'end', end_sample) ELSE json_object('kind',
                file_kind) END) END) ORDER BY position)) AS json
            FROM numbered AS n
            GROUP BY side_no, run_no
            ), sides AS (
            SELECT json_group_array(json_object('kind', side_kind, 'ordinal', side_ordinal,
                'runs', json(runs_json)) ORDER BY side_no) AS json
                FROM (
                SELECT side_no, MIN(side_kind) AS side_kind, MIN(side_ordinal) AS side_ordinal,
                    json_group_array(json(json) ORDER BY run_no) AS runs_json
                FROM runs
                GROUP BY side_no
            )
            ), unplayed_files(file_id) AS (
            SELECT listed.file_id
                FROM (
                SELECT f.relative_path AS file_id,(f.role = 'artwork' AND
                    bae_supported_cover(f.relative_path)) AS showable_image
                FROM f
            ) AS listed
                WHERE NOT listed.showable_image AND NOT EXISTS (
                SELECT 1
                    FROM rows
                    WHERE rows.sheet_id = listed.file_id
            ) AND NOT EXISTS (
                SELECT 1
                FROM @crate::tables::folder_scans::row_fragments::CandidateUnits(key: :key) AS
                    offered
                    WHERE offered.file_id = listed.file_id
            )
            ), left_out(relative_path, position) AS (
            SELECT unread.relative_path, unread.position
            FROM @crate::tables::import_list::fragments::LeftOutAudio(key: :key) AS unread
            ) SELECT json_object('images', json(NULLIF((
            SELECT json_group_array(json_object('file_id', relative_path, 'name', file_name,
                'path', hex(absolute_path), 'size', size, 'modified_at_ns', modified_at_ns)
                ORDER BY position)
            FROM f
            WHERE role = 'artwork'
            ), '[]')), 'sides', json(NULLIF((
            SELECT json
            FROM sides
            ), '[]')), 'files', json(NULLIF((
            SELECT json_group_array(CASE f.role WHEN 'track_sheet' THEN json_object('sheet',
                json(s.json)) ELSE json_object('file', json_object('file_id', f.relative_path,
                'name', f.relative_path, 'size', f.size, 'role', f.role)) END ORDER BY
                f.position)
            FROM f
            LEFT JOIN sheets AS s ON s.relative_path = f.relative_path
                WHERE f.relative_path IN (
                SELECT file_id
                FROM unplayed_files
            )
            ), '[]')), 'left_out', json((
            SELECT CASE WHEN COUNT(*) > 0 THEN json_object('file_count', COUNT(*), 'files',
                json_group_array(relative_path ORDER BY position)) END
            FROM left_out
            )), 'untitled',(
            SELECT COUNT(*) FILTER (WHERE bae_trim(title) = '')
            FROM rows
            ), 'tracks',(
            SELECT COUNT(*)
            FROM rows
            ), 'length_ms',(
            SELECT COALESCE(SUM(unit_ms), 0)
            FROM rows
        ))
    ) AS mapping
"#);

sqlive_compose::fragment!(
    ArtistField(key: crate::CandidateKey) {
        artist_field_key: crate::CandidateKey,
        artist_field: Option<String>
    },
    r#"
    SELECT :key AS artist_field_key, (
            WITH artist_field AS MATERIALIZED (
            SELECT part, track_id, field
            FROM @bae_records::records::artist_standing::ArtistFieldStanding(
                raw: @crate::tables::import_state::row_fragments::DraftCreditRows(key: :key),
                credit_answer:
                    @crate::tables::credit_resolutions::row_fragments::StoredCreditAnswers
                ) AS standing
        ) SELECT field
        FROM artist_field
        WHERE part = 0
    ) AS artist_field
"#);
