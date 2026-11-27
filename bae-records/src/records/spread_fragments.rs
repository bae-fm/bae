//! Records' rows that other crates' fragments join to, one row of a table
//! each. The columns are read by the joins' SQL, and no Rust code reads them.

sqlive_compose::fragment!(
    ArtistRow,
    r#"
    SELECT artist.id AS artist_id, artist.name, artist.sort_name, artist.musicbrainz_artist_id,
           artist.discogs_artist_id
    FROM artists AS artist
"#
);

sqlive_compose::fragment!(
    ArtistCreditName,
    r#"
    SELECT credit.artist_id, credit.name_key, credit.name
    FROM artist_credits AS credit
"#
);

sqlive_compose::fragment!(
    ReleaseFileRow,
    r#"
    SELECT file.release_id, file.file AS stored_file
    FROM release_files AS file
"#
);

sqlive_compose::fragment!(
    HeldPressing,
    r#"
    SELECT held.catalog, held.key
    FROM release_records AS held
    WHERE held.kind = 'pressing'
"#
);
