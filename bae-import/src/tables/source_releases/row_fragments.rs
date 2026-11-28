//! The source releases' rows that other domains' statements spread to reach
//! them.

coven::fragment!(
    LinkedAlbumGroup on import_candidate_release_link {
        source_group_id: Option<String>
    },
    r#"
    SELECT release.source_group_id
    FROM source_release AS release
    WHERE release.catalog = @parent.source AND release.release_id = @parent.release_id
"#);

coven::fragment!(
    MatchAlbumFirstYear on import_candidate_match {
        album_first_year: Option<i32>
    },
    r#"
    SELECT release.album_first_year
    FROM source_release AS release
    WHERE release.catalog = @parent.source AND release.release_id = @parent.release_id
"#);

// The own albums of each stored release `listed` names, in the order
// surfaces list them by `ord`: the one its catalog files it under, then those
// its records name, in their catalogs' order.
coven::fragment!(
    OwnAlbums with listed(at INTEGER, catalog TEXT, release_id TEXT) {
        at: i64,
        catalog: String,
        release_id: String,
        ord: i64,
        album_catalog: String,
        album_key: String
    },
    r#"
    SELECT listed.at, release.catalog, release.release_id, 0 AS ord,
           release.catalog AS album_catalog, release.source_group_id AS album_key
    FROM listed
    JOIN source_release AS release
        ON release.catalog = listed.catalog AND release.release_id = listed.release_id
    WHERE release.source_group_id IS NOT NULL
    UNION ALL
    SELECT listed.at, record.catalog, record.release_id,
           1 + bae_catalog_rank(record.record_catalog) AS ord,
           record.record_catalog AS album_catalog,
           CASE record.kind WHEN 'album' THEN record.key ELSE record.album_key END AS album_key
    FROM listed
    JOIN source_release_record AS record
        ON record.catalog = listed.catalog AND record.release_id = listed.release_id
    WHERE record.kind = 'album' OR record.album_key IS NOT NULL
"#
);
