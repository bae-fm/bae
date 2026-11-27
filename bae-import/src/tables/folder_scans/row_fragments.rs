//! The folder scans' rows that other domains' fragments spread to reach them.

sqlive_compose::fragment!(
    ScanContent(key: crate::CandidateKey) {
        content_hash: Option<String>,
        file_edit_revision: i64
    },
    r#"
    SELECT scan.content_hash, scan.file_edit_revision
    FROM scan_candidate AS scan
    WHERE scan.path = :key
"#);

sqlive_compose::fragment!(
    ScanGrouping(key: crate::CandidateKey) { grouping_key: Option<crate::CandidateKey> },
    r#"
    SELECT scan.grouping_key
    FROM scan_candidate AS scan
    WHERE scan.path = :key
"#);

sqlive_compose::fragment!(
    CandidatePartRows(key: crate::CandidateKey),
    r#"
    SELECT part.position
    FROM scan_candidate AS candidate
    JOIN scan_candidate_part AS part ON part.watched_folder_path =
        candidate.watched_folder_path AND part.candidate_path = candidate.path
    WHERE candidate.path = :key
"#
);

sqlive_compose::fragment!(
    CandidateFiles(key: crate::CandidateKey),
    r#"
    SELECT file.position, file.relative_path, file.file_name, file.absolute_path, file.size,
        file.modified_at_ns, file.role, file.sheet_binding, file.sheet_binding_codec,
        file.sheet_place, file.sheet_medium, file.sheet_side
    FROM scan_candidate_file AS file
    WHERE file.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND file.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateTagSnapshot(key: crate::CandidateKey),
    r#"
    SELECT snapshot.candidate_path, snapshot.embedded_cover_source_relative_path
    FROM scan_candidate_tag_snapshot AS snapshot
    WHERE snapshot.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND snapshot.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateFileTags(key: crate::CandidateKey),
    r#"
    SELECT tags.relative_path, tags.file_size, tags.modified_at_ns
    FROM scan_candidate_file_tag AS tags
    WHERE tags.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND tags.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateUnits(key: crate::CandidateKey),
    r#"
    SELECT unit.position, unit.file_kind, unit.file_id, unit.sheet_id, unit.slice_index,
        unit.cue_track_position, unit.duration_ms, unit.start_sample, unit.end_sample,
        unit.pregap_file_id
    FROM scan_candidate_unit AS unit
    WHERE unit.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND unit.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateCueTracks(key: crate::CandidateKey),
    r#"
    SELECT track.sheet_relative_path, track.position, track.start_cue_frames
    FROM scan_cue_track AS track
    WHERE track.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND track.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateSheetAudio(key: crate::CandidateKey),
    r#"
    SELECT bound.sheet_relative_path, bound.file_reference, bound.audio_relative_path
    FROM scan_sheet_audio_file AS bound
    WHERE bound.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND bound.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateSheetOffers(key: crate::CandidateKey),
    r#"
    SELECT offered.sheet_relative_path, offered.file_reference, offered.audio_relative_path,
        offered.offer, offered.codec
    FROM scan_candidate_sheet_offer AS offered
    WHERE offered.watched_folder_path = (
        SELECT candidate.watched_folder_path
        FROM scan_candidate AS candidate
        WHERE candidate.path = :key
        LIMIT 1
    ) AND offered.candidate_path = :key
"#
);

sqlive_compose::fragment!(
    CandidateSheetReferences(key: crate::CandidateKey) {
        sheet: String,
        file_reference: String,
        seq: i64
    },
    r#"
    SELECT sheet, file_reference, MIN(seq) AS seq
    FROM (
        SELECT sheet, file_reference, ROW_NUMBER() OVER (PARTITION BY sheet ORDER BY
            track_position, part, index_position) AS seq
        FROM (
            SELECT ct.sheet_relative_path AS sheet, ci.file_reference, ct.position AS
                track_position, 0 AS part, ci.position AS index_position
            FROM scan_cue_track AS ct
            JOIN scan_cue_index AS ci ON (ci.watched_folder_path, ci.candidate_path,
                ci.sheet_relative_path, ci.track_position) = (ct.watched_folder_path,
                ct.candidate_path, ct.sheet_relative_path, ct.position)
            WHERE ct.watched_folder_path = (
                SELECT candidate.watched_folder_path
                FROM scan_candidate AS candidate
                WHERE candidate.path = :key
                LIMIT 1
            ) AND ct.candidate_path = :key AND ct.mode = 'audio'
            UNION ALL SELECT ct.sheet_relative_path, ct.file_reference, ct.position, 1, 0
            FROM scan_cue_track AS ct
            WHERE ct.watched_folder_path = (
                SELECT candidate.watched_folder_path
                FROM scan_candidate AS candidate
                WHERE candidate.path = :key
                LIMIT 1
            ) AND ct.candidate_path = :key AND ct.mode = 'audio'
        )
    )
    GROUP BY sheet, file_reference
"#);

// The keys of what a pass over the watched folder `root` knows of it, a row
// for each, with the other columns NULL: its entries, its sidecars,
// the content hashes in `decided` and the groupings in `anchored`. A read
// joins what is stored under each key to its row.
sqlive_compose::fragment!(
    RootKeys(root: Vec<u8>) with decided(content_hash TEXT), anchored(grouping_key BLOB) {
        item_key: Option<crate::CandidateKey>,
        sidecar_key: Option<Vec<u8>>,
        decided_hash: Option<String>,
        anchored_key: Option<crate::CandidateKey>
    },
    r#"
    SELECT candidate.path AS item_key, NULL AS sidecar_key, NULL AS decided_hash,
        NULL AS anchored_key
    FROM scan_candidate AS candidate
    WHERE candidate.watched_folder_path = :root
    UNION ALL
    SELECT NULL, sidecar.folder, NULL, NULL
    FROM scan_sidecar AS sidecar
    WHERE sidecar.watched_folder_path = :root
    UNION ALL
    SELECT NULL, NULL, decided.content_hash, NULL FROM decided
    UNION ALL
    SELECT NULL, NULL, NULL, anchored.grouping_key FROM anchored
"#
);
