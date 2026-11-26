//! What release groupings offer the candidate pane: whether a grouping took the candidate in, why it cannot be worked on, and whether the candidate is skipped.

sqlive_compose::fragment!(
    GroupingOfCandidate(key: crate::CandidateKey) {
        grouping_of_key: crate::CandidateKey,
        taken_in: bool,
        skipped: Option<bool>
    },
    r#"
    SELECT :key AS grouping_of_key,
           EXISTS (SELECT 1 FROM release_grouping_member WHERE member_key = :key) AS taken_in,
           CASE WHEN scan.grouping_key IS NOT NULL THEN grouping.skipped
                ELSE EXISTS (
                    SELECT 1
                    FROM @crate::tables::skipped_candidates::row_fragments::SkippedFolder AS aside
                    WHERE aside.candidate_path = :key) END AS skipped,
           grouping.blocked, grouping.blocked_subject, grouping.blocked_holder
    FROM @crate::tables::folder_scans::row_fragments::ScanGrouping(key: :key) AS scan
    LEFT JOIN release_grouping AS grouping ON grouping.key = scan.grouping_key
"#);
