//! What release groupings offer the candidate pane: whether a grouping took the candidate in, why it cannot be worked on, and whether the candidate is skipped.

sqlive_compose::fragment!(
    GroupingOfCandidate(key: crate::CandidateKey) {
        grouping_of_key: crate::CandidateKey,
        taken_in: bool,
        skipped: Option<bool>
    },
    r#"
    SELECT scan.path AS grouping_of_key,
           EXISTS (SELECT 1 FROM release_grouping_member WHERE member_key = scan.path) AS taken_in,
           CASE WHEN scan.grouping_key IS NOT NULL THEN grouping.skipped
                ELSE EXISTS (SELECT 1 FROM skipped_import_candidates
                             WHERE candidate_path = scan.path) END AS skipped,
           grouping.blocked, grouping.blocked_subject, grouping.blocked_holder
    FROM scan_candidate AS scan
    LEFT JOIN release_grouping AS grouping ON grouping.key = scan.grouping_key
    WHERE scan.path = :key
"#);
