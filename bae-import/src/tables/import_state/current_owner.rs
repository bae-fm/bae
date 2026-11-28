//! The owners of candidates' current states: for each key of `keys` the scan
//! stores, the state its content has at the scan's file-edit revision. The
//! pane's reads spread it to hand the state and cards fragments the owner they
//! read, and the cover picker's statement spreads it to ask whether a key has
//! one.

coven::fragment!(
    CurrentOwner with keys(candidate_key BLOB) {
        content_hash: Option<String>,
        identify_run: Option<i64>
    },
    r#"
    SELECT state.content_hash AS content_hash, NULL AS identify_run
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scanned
        ON scanned.candidate_key = keys.candidate_key
    JOIN import_candidate_state AS state ON state.content_hash = scanned.content_hash
        AND state.edit_revision = scanned.file_edit_revision
"#);
