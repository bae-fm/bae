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
