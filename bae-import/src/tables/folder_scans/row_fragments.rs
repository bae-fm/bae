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
