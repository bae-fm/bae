//! The skipped candidates' rows that other domains' fragments spread to reach
//! them.

coven::fragment!(
    SkippedFolder,
    r#"
    SELECT skipped.candidate_path
    FROM skipped_import_candidates AS skipped
"#
);
