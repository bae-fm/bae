//! The folder scans' rows that other domains' fragments spread to reach them.

// Every candidate the scans hold: the key its rows are under, and the files it
// reads.
coven::fragment!(
    HeldCandidates,
    r#"
    SELECT scan.watched_folder_path, scan.path, scan.content_hash
    FROM scan_candidate AS scan
"#
);

// Each candidate's content hash and the revision of its file decisions,
// under its key.
coven::fragment!(
    ScanContent {
        content_hash: Option<String>,
        file_edit_revision: i64
    },
    r#"
    SELECT scan.path AS candidate_key, scan.content_hash, scan.file_edit_revision
    FROM scan_candidate AS scan
"#);

// Each candidate's grouping, under its key.
coven::fragment!(
    ScanGrouping { grouping_key: Option<crate::CandidateKey> },
    r#"
    SELECT scan.path AS candidate_key, scan.grouping_key
    FROM scan_candidate AS scan
"#);

// Each candidate's parts, under its key.
coven::fragment!(
    CandidatePartRows,
    r#"
    SELECT candidate.path AS candidate_key, part.position
    FROM scan_candidate AS candidate
    JOIN scan_candidate_part AS part ON part.watched_folder_path =
        candidate.watched_folder_path AND part.candidate_path = candidate.path
"#
);

// Each candidate's files, under its key.
coven::fragment!(
    CandidateFiles,
    r#"
    SELECT candidate.path AS candidate_key, file.position, file.relative_path, file.file_name,
        file.absolute_path, file.size, file.modified_at_ns, file.role, file.sheet_binding,
        file.sheet_binding_codec, file.sheet_place, file.sheet_medium, file.sheet_side
    FROM scan_candidate AS candidate
    JOIN scan_candidate_file AS file ON file.watched_folder_path =
        candidate.watched_folder_path AND file.candidate_path = candidate.path
"#
);

// The files of each candidate that bae can show as its pictures: artwork of a
// format a cover can be made from. The cover picker offers them, and the pane
// shows them as images and leaves them out of its Files list.
coven::fragment!(
    ShowableArtwork,
    r#"
    SELECT offered.*
    FROM @crate::tables::folder_scans::row_fragments::CandidateFiles AS offered
    WHERE offered.role = 'artwork' AND bae_supported_cover(offered.relative_path)
"#
);

// Each candidate's tag snapshot, under its key.
coven::fragment!(
    CandidateTagSnapshot,
    r#"
    SELECT candidate.path AS candidate_key, snapshot.embedded_cover_source_relative_path
    FROM scan_candidate AS candidate
    JOIN scan_candidate_tag_snapshot AS snapshot ON snapshot.watched_folder_path =
        candidate.watched_folder_path AND snapshot.candidate_path = candidate.path
"#
);

// Each candidate's files' tags, under its key.
coven::fragment!(
    CandidateFileTags,
    r#"
    SELECT candidate.path AS candidate_key, tags.relative_path, tags.file_size,
        tags.modified_at_ns
    FROM scan_candidate AS candidate
    JOIN scan_candidate_file_tag AS tags ON tags.watched_folder_path =
        candidate.watched_folder_path AND tags.candidate_path = candidate.path
"#
);

// Each candidate's units, under its key.
coven::fragment!(
    CandidateUnits,
    r#"
    SELECT candidate.path AS candidate_key, unit.position, unit.file_kind, unit.file_id,
        unit.sheet_id, unit.slice_index, unit.cue_track_position, unit.duration_ms,
        unit.start_sample, unit.end_sample, unit.pregap_file_id
    FROM scan_candidate AS candidate
    JOIN scan_candidate_unit AS unit ON unit.watched_folder_path =
        candidate.watched_folder_path AND unit.candidate_path = candidate.path
"#
);

// Each candidate's CUE tracks, under its key.
coven::fragment!(
    CandidateCueTracks,
    r#"
    SELECT candidate.path AS candidate_key, track.sheet_relative_path, track.position,
        track.start_cue_frames
    FROM scan_candidate AS candidate
    JOIN scan_cue_track AS track ON track.watched_folder_path =
        candidate.watched_folder_path AND track.candidate_path = candidate.path
"#
);

// The audio each candidate's CUE sheets name, under its key.
coven::fragment!(
    CandidateSheetAudio,
    r#"
    SELECT candidate.path AS candidate_key, bound.sheet_relative_path, bound.file_reference,
        bound.audio_relative_path
    FROM scan_candidate AS candidate
    JOIN scan_sheet_audio_file AS bound ON bound.watched_folder_path =
        candidate.watched_folder_path AND bound.candidate_path = candidate.path
"#
);

// The audio each candidate offers for its CUE sheets' references, under its
// key.
coven::fragment!(
    CandidateSheetOffers,
    r#"
    SELECT candidate.path AS candidate_key, offered.sheet_relative_path,
        offered.file_reference, offered.audio_relative_path, offered.offer, offered.codec
    FROM scan_candidate AS candidate
    JOIN scan_candidate_sheet_offer AS offered ON offered.watched_folder_path =
        candidate.watched_folder_path AND offered.candidate_path = candidate.path
"#
);

// The file references of the CUE sheets of each candidate in `keys`, under
// its key, each with its first place in its sheet's audio tracks. The read
// takes its keys, where the other row sets leave the read that spreads them to
// narrow them, because coven passes no condition into a read that calls a
// window function.
coven::fragment!(
    CandidateSheetReferences with keys(candidate_key BLOB) {
        candidate_key: crate::CandidateKey,
        sheet: String,
        file_reference: String,
        seq: i64
    },
    r#"
    SELECT candidate_key, sheet, file_reference, MIN(seq) AS seq
    FROM (
        SELECT candidate_key, sheet, file_reference, ROW_NUMBER() OVER (
            PARTITION BY candidate_key, sheet ORDER BY track_position, part, index_position) AS seq
        FROM (
            SELECT ct.candidate_path AS candidate_key, ct.sheet_relative_path AS sheet,
                ci.file_reference, ct.position AS track_position, 0 AS part,
                ci.position AS index_position
            FROM scan_cue_track AS ct
            JOIN scan_cue_index AS ci ON (ci.watched_folder_path, ci.candidate_path,
                ci.sheet_relative_path, ci.track_position) = (ct.watched_folder_path,
                ct.candidate_path, ct.sheet_relative_path, ct.position)
            WHERE (ct.watched_folder_path, ct.candidate_path) IN (
                SELECT candidate.watched_folder_path, candidate.path
                FROM keys
                JOIN scan_candidate AS candidate ON candidate.path = keys.candidate_key)
                AND ct.mode = 'audio'
            UNION ALL SELECT ct.candidate_path, ct.sheet_relative_path, ct.file_reference,
                ct.position, 1, 0
            FROM scan_cue_track AS ct
            WHERE (ct.watched_folder_path, ct.candidate_path) IN (
                SELECT candidate.watched_folder_path, candidate.path
                FROM keys
                JOIN scan_candidate AS candidate ON candidate.path = keys.candidate_key)
                AND ct.mode = 'audio'
        )
    )
    GROUP BY candidate_key, sheet, file_reference
"#);

// The keys of what a pass over the watched folder `root` knows of it, a row
// for each, with the other columns NULL: its entries, its sidecars,
// the content hashes in `decided` and the groupings in `anchored`. A read
// joins what is stored under each key to its row.
coven::fragment!(
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
