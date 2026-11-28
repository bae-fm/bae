//! The owner of a candidate's current state: the scan stored at a key, with
//! the state its content has at the scan's file-edit revision. The pane's
//! reads spread it to hand the state and cards fragments the owner they read,
//! and the cover picker's statement spreads it to ask whether a key has one.

coven::fragment!(
    CurrentOwner(key: crate::CandidateKey) { content_hash: Option<String>, identify_run: Option<i64> },
    r#"
    SELECT state.content_hash AS content_hash, NULL AS identify_run
    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scanned
    JOIN import_candidate_state AS state ON state.content_hash = scanned.content_hash
        AND state.edit_revision = scanned.file_edit_revision
"#);
