//! What the import list offers the candidate pane: where the list places a candidate, what it can do, the mapping of its files to tracks, and its album artists.

coven::fragment!(
    Listed(key: crate::CandidateKey) {
        listed_key: crate::CandidateKey,
        tab: String,
        import_failed: Option<bool>,
        actionable: Option<bool>,
        pending_state: Option<String>,
        needs_you_reason: Option<String>,
        reason_pressings: Option<i64>,
        error_detail: Option<String>,
        actions: String,
        part_count: i64
    },
    r#"
    SELECT :key AS listed_key, p.tab, p.import_failed, p.actionable, p.pending_state,
           p.needs_you_reason, p.reason_pressings, p.error_detail,
           @crate::tables::import_list::row_fragments::ListedActions(parent: p) AS actions,
           (SELECT COUNT(*)
            FROM @crate::tables::folder_scans::row_fragments::CandidatePartRows AS part WHERE part.candidate_key = :key)
               AS part_count
    FROM import_list_placement AS p
    WHERE p.key = :key AND p.item = 'candidate'
"#);

// Each draft track with the place among the folder's units of the one it
// plays: the unit of the same kind, file, sheet and slice. The place is NULL
// where the folder has no such unit, which the reads refuse.
coven::fragment!(
    DraftUnits(key: crate::CandidateKey) {
        draft_track_id: String,
        draft_unit_position: Option<i64>
    },
    r#"
    WITH matched(draft_track_id, draft_unit_position) AS (
    SELECT t.track_id, u.position
    FROM @crate::tables::import_state::row_fragments::DraftTrackRows AS t
    LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateUnits AS u ON u.candidate_key = :key AND
        u.file_kind IS t.file_kind AND u.file_id IS t.file_id AND u.sheet_id IS t.sheet_id
        AND u.slice_index IS t.slice_index
    WHERE t.candidate_key = :key AND t.content_hash =(
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
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
coven::fragment!(
    LeftOutAudio(key: crate::CandidateKey) {
        relative_path: String,
        position: i64
    },
    r#"
    WITH unit_reads(position, file_id) AS (
    SELECT whole.position, whole.file_id
    FROM @crate::tables::folder_scans::row_fragments::CandidateUnits AS whole WHERE whole.candidate_key = :key
    UNION ALL
    SELECT gap.position, gap.pregap_file_id
    FROM @crate::tables::folder_scans::row_fragments::CandidateUnits AS gap WHERE gap.candidate_key = :key
    AND gap.pregap_file_id IS NOT NULL
    ), folder_files(relative_path, position) AS (
    SELECT file.relative_path, file.position
    FROM @crate::tables::folder_scans::row_fragments::CandidateFiles AS file WHERE file.candidate_key = :key
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

// The artist fields of the candidate's draft: the album's, part 0, then each
// track's that names its own, part 1, each with how its credits stand.
coven::fragment!(
    DraftArtistFields(key: crate::CandidateKey) { part: i64, track_id: Option<String>, field: String },
    r#"
    SELECT standing.part, standing.track_id, standing.field
    FROM @bae_records::records::artist_standing::ArtistFieldStanding(
        raw: (SELECT credit.part, credit.track_id, credit.position, credit.assignment_kind,
                   credit.artist_id, credit.name, credit.sort_name,
                   credit.musicbrainz_artist_id, credit.discogs_artist_id,
                   credit.credit_key, credit.credited_as, credit.join_phrase
               FROM @crate::tables::import_state::row_fragments::DraftCreditRows AS credit
               WHERE credit.candidate_key = :key),
        credit_answer:
            @crate::tables::credit_resolutions::row_fragments::StoredCreditAnswers
        ) AS standing
"#);

coven::fragment!(
    Mapping(key: crate::CandidateKey) {
        mapping_key: crate::CandidateKey,
        mapping: Option<String>
    },
    r#"
    SELECT :key AS mapping_key, (
            WITH artist_field AS MATERIALIZED (
            SELECT standing.part, standing.track_id, standing.field
            FROM @crate::tables::import_list::fragments::DraftArtistFields(key: :key) AS standing
            ), candidate AS (
            SELECT scan.content_hash
            FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
            LIMIT 1
            ), f AS (
            SELECT listed.*
            FROM @crate::tables::folder_scans::row_fragments::CandidateFiles AS listed WHERE listed.candidate_key = :key
            ), audio_json(relative_path, json) AS (
            SELECT relative_path, json_object('file_id', relative_path, 'name', file_name,
                'path', hex(absolute_path))
            FROM f
            WHERE role = 'audio'
            ), drafted_mediums AS (
            SELECT drafted.position AS id, drafted.content_hash AS release_id,
                drafted.position, drafted.medium, drafted.side_count
            FROM @crate::tables::import_state::row_fragments::DraftMediumRows AS drafted
            WHERE drafted.candidate_key = :key AND drafted.content_hash =(
                SELECT content_hash
                FROM candidate
            )
            ), drafted_places AS (
            SELECT drafted.track_id, drafted.content_hash, drafted.medium, drafted.side,
                drafted.track_number, drafted.stated_length_ms, drafted.position
            FROM @crate::tables::import_state::row_fragments::DraftTrackRows AS drafted
            WHERE drafted.candidate_key = :key
            ), place AS (
            SELECT placed.*
            FROM @bae_records::records::place_fragments::TrackPlaces(
                mediums: (SELECT * FROM drafted_mediums),
                places: (SELECT * FROM drafted_places)) AS placed
            ), rows AS (
                SELECT t.position, t.title, t.track_id, t.track_json AS track_json, p.side_kind, p.side_ordinal, p.number, u.file_kind,
                u.sheet_id, u.file_id AS unit_file_id, u.slice_index, u.duration_ms AS unit_ms,
                u.position AS unit_position, p.medium_position, p.side AS medium_side,
                u.start_sample, u.end_sample, a.json AS audio, ct.start_cue_frames * 1000 / 75
                AS cue_start_ms, CASE WHEN t.stated_length_ms IS NOT NULL AND u.duration_ms IS
                NOT NULL AND bae_lengths_disagree(u.duration_ms, t.stated_length_ms) THEN
                t.stated_length_ms END AS listed_ms
            FROM @crate::tables::import_state::row_fragments::DraftTrackRows AS t
            JOIN place AS p ON p.track_id = t.track_id AND p.release_id = t.content_hash
                LEFT JOIN @crate::tables::import_list::fragments::DraftUnits(key: :key) AS matched
                ON matched.draft_track_id = t.track_id
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateUnits AS u ON u.candidate_key = :key AND
                u.position = matched.draft_unit_position
            LEFT JOIN audio_json AS a ON a.relative_path = u.file_id
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateCueTracks AS ct ON ct.candidate_key = :key AND
                ct.sheet_relative_path = u.sheet_id AND ct.position = u.cue_track_position
                WHERE t.candidate_key = :key AND t.content_hash =(
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
            SELECT referenced.sheet, referenced.file_reference, referenced.seq
            FROM @crate::tables::folder_scans::row_fragments::CandidateSheetReferences(
                keys: (SELECT :key AS candidate_key)) AS referenced
            ), side_choice_rows AS (
            SELECT choice.*
            FROM @bae_records::records::place_fragments::SheetSideChoices(
                mediums: (SELECT * FROM drafted_mediums)) AS choice
            ORDER BY choice.medium, choice.side
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
                FROM @crate::tables::folder_scans::row_fragments::CandidateSheetOffers AS o
                JOIN f AS fo ON fo.relative_path = o.audio_relative_path
                    WHERE o.candidate_key = :key AND o.sheet_relative_path = r.sheet AND
                    o.file_reference = r.file_reference
            )))
            FROM refs AS r
                LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateSheetAudio AS sa ON sa.candidate_key = :key AND
                    sa.sheet_relative_path = r.sheet AND
                sa.file_reference = r.file_reference
            ), sheet_audio_counts(sheet, count) AS (
            SELECT sheet_relative_path, COUNT(*)
            FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio AS counted WHERE counted.candidate_key = :key
            GROUP BY sheet_relative_path
            ), sheets(relative_path, json) AS (
                SELECT s.relative_path, json_object('sheet_id', s.relative_path, 'name',
                    s.relative_path, 'bound', CASE s.sheet_binding WHEN 'resolved' THEN CASE
                    COALESCE(n.count, 0) WHEN 0 THEN json_object('binding', 'resolved_to_none')
                    WHEN 1 THEN json_object('binding', 'describes', 'audio', json((
                SELECT a.json
                FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio AS sa
                JOIN audio_json AS a ON a.relative_path = sa.audio_relative_path
                    WHERE sa.candidate_key = :key AND sa.sheet_relative_path = s.relative_path
                ))) ELSE json_object('binding', 'describes_files', 'audio_count', n.count) END
                    WHEN 'unresolved' THEN json_object('binding', 'unresolved', 'requested',
                    json((
                SELECT json_group_array(r.file_reference ORDER BY r.seq)
                FROM refs AS r
                    WHERE r.sheet = s.relative_path AND NOT EXISTS (
                    SELECT 1
                    FROM @crate::tables::folder_scans::row_fragments::CandidateSheetAudio AS sa WHERE sa.candidate_key = :key
                        AND sa.sheet_relative_path = s.relative_path AND sa.file_reference = r.file_reference
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
            SELECT free.file_id
            FROM @bae_records::records::file_fragments::UnplayedFiles(
                listed: (SELECT f.relative_path,
                             f.relative_path IN (
                                 SELECT shown.relative_path
                                 FROM @crate::tables::folder_scans::row_fragments::ShowableArtwork AS shown WHERE shown.candidate_key = :key)
                         FROM f),
                track_reads: (SELECT NULL, rows.sheet_id
                              FROM rows
                              UNION ALL
                              SELECT offered.file_id, NULL
                              FROM @crate::tables::folder_scans::row_fragments::CandidateUnits AS offered WHERE offered.candidate_key = :key)) AS free
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

coven::fragment!(
    ArtistField(key: crate::CandidateKey) {
        artist_field_key: crate::CandidateKey,
        artist_field: Option<String>
    },
    r#"
    SELECT :key AS artist_field_key, (
            WITH artist_field AS MATERIALIZED (
            SELECT standing.part, standing.track_id, standing.field
            FROM @crate::tables::import_list::fragments::DraftArtistFields(key: :key) AS standing
        ) SELECT field
        FROM artist_field
        WHERE part = 0
    ) AS artist_field
"#);
