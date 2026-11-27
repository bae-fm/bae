//! What release groupings offer other reads: whether a grouping took the
//! candidate in, why it cannot be worked on, and whether the candidate is
//! skipped; and a candidate's entry with where it stands, which composes that
//! with the scan's rows, the standing and the release imported from it.

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

sqlive_compose::fragment!(
    EnteredCandidate(key: crate::CandidateKey),
    r#"
    SELECT scan.*, grouping.*, standing.*, imported.*
    FROM @crate::tables::folder_scans::fragments::ScanItems(
        keys: (SELECT :key AS candidate_key)) AS scan
    LEFT JOIN @crate::tables::release_groupings::fragments::GroupingOfCandidate(key: :key)
        AS grouping ON 1
    LEFT JOIN @crate::tables::import_state::fragments::Standing(
        keys: (SELECT :key AS candidate_key)) AS standing ON 1
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(key: :key)
        AS imported ON 1
"#
);

// Each grouping in `keys`: the folder it is anchored at, if any, how it reads
// and who decided that, and the releases a grouping with no anchor takes in,
// as a JSON array in play order.
sqlive_compose::fragment!(
    Groupings with keys(grouping_key BLOB) {
        struct StoredMember { key: hex }
        stored_key: crate::CandidateKey,
        combined: bool,
        members: json<Vec<StoredMember>>
    },
    r#"
    SELECT stored.key AS stored_key, stored.anchor_folder, stored.combined, stored.author,
        (SELECT json_group_array(json_object('key', hex(member.member_key))
                    ORDER BY member.position)
         FROM release_grouping_member AS member
         WHERE member.grouping_key = stored.key) AS members
    FROM keys
    JOIN release_grouping AS stored ON stored.key = keys.grouping_key
"#
);
