//! The owner of a candidate's current state: the scan stored at a key, with
//! the state its content has at the scan's file-edit revision. The pane's
//! reads spread it to hand the state and cards fragments the owner they read,
//! and the cover picker's statement asks whether a key has one.

sqlive_compose::fragment!(
    CurrentOwner(key: crate::CandidateKey) { content_hash: Option<String>, identify_run: Option<i64> },
    r#"
    SELECT scanned.content_hash AS content_hash, NULL AS identify_run
    FROM scan_candidate AS scanned
    JOIN import_candidate_state AS state ON state.content_hash = scanned.content_hash
        AND state.edit_revision = scanned.file_edit_revision
    WHERE scanned.path = :key
"#);

sqlive_compose::read!(
    CurrentOwnerAt(key: crate::CandidateKey),
    r#"
    SELECT owner.* FROM @crate::tables::import_state::current_owner::CurrentOwner(key: :key)
        AS owner
"#
);
