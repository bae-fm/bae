//! The source releases' rows that other domains' fragments spread to reach
//! them.

sqlive_compose::fragment!(
    LinkedAlbumGroup on import_candidate_release_link {
        source_group_id: Option<String>
    },
    r#"
    SELECT release.source_group_id
    FROM source_release AS release
    WHERE release.catalog = @parent.source AND release.release_id = @parent.release_id
"#);
