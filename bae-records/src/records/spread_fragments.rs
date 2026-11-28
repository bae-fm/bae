//! Records' rows that other crates' fragments join to, one row of a table
//! each. The columns are read by the joins' SQL, and no Rust code reads them.

coven::fragment!(
    ArtistRow,
    r#"
    SELECT artist.id AS artist_id, artist.name, artist.sort_name, artist.musicbrainz_artist_id,
           artist.discogs_artist_id
    FROM artists AS artist
"#
);

coven::fragment!(
    AlbumArtistRow,
    r#"
    SELECT credit.album_id, credit.artist_id, credit.credit_key
    FROM album_artists AS credit
"#
);

coven::fragment!(
    ArtistCreditName,
    r#"
    SELECT credit.artist_id, credit.name_key, credit.name
    FROM artist_credits AS credit
"#
);

coven::fragment!(
    ReleaseFileRow,
    r#"
    SELECT file.release_id, file.id AS file_id, file.original_filename, file.content_type,
           file.file AS stored_file
    FROM release_files AS file
"#
);

coven::fragment!(
    HeldPressing,
    r#"
    SELECT held.catalog, held.key
    FROM release_records AS held
    WHERE held.kind = 'pressing'
"#
);

coven::fragment!(
    TrackRow,
    r#"
    SELECT track.id AS track_id, track.release_id, track.position, track.title,
           track.duration_ms
    FROM tracks AS track
"#
);
