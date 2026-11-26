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
