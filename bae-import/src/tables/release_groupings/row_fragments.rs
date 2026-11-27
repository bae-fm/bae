//! The release groupings' rows that other domains' fragments spread to reach
//! them.

// The groupings anchored at the folder `root` or at a folder below it, whose
// paths run from `below_first` to `below_past`.
sqlive_compose::fragment!(
    AnchoredGroupingKeys(root: Vec<u8>, below_first: Vec<u8>, below_past: Vec<u8>) {
        grouping_key: crate::CandidateKey
    },
    r#"
    SELECT stored.key AS grouping_key
    FROM release_grouping AS stored
    WHERE stored.anchor_folder = :root
        OR (stored.anchor_folder >= :below_first AND stored.anchor_folder < :below_past)
"#
);
