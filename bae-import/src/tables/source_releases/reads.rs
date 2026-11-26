//! The statements that read stored catalog releases.

sqlive_compose::read!(
    ReleasesWithKeptAlbums(releases: String) { at: Option<i64> },
    r#"
    SELECT wanted.key AS at, release.*, kept.*
    FROM json_each(:releases) AS wanted
    JOIN @crate::tables::source_releases::fragments::StoredRelease AS release
        ON release.catalog = wanted.value ->> 0 AND release.release_id = wanted.value ->> 1
    JOIN @crate::tables::album_link_rows::KeptGroupAlbums(
        wanted: (SELECT listed.key AS at FROM json_each(:releases) AS listed),
        asked: (SELECT own.at, own.ord, own.album_catalog AS catalog, own.album_key AS key
                FROM @crate::tables::source_releases::row_fragments::OwnAlbums(
                    listed: (SELECT listed.key AS at, listed.value ->> 0 AS catalog,
                                    listed.value ->> 1 AS release_id
                             FROM json_each(:releases) AS listed)) AS own)) AS kept
        ON kept.at = wanted.key
"#
);
