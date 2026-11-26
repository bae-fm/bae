//! The candidate state's rows that other domains' fragments spread to reach
//! them.

// The album's credits and the tracks' are two fragments. SQLite plans a read
// of a fragment of one table as a read of that table, and runs a union of the
// two as a compound subquery.
sqlive_compose::fragment!(
    AlbumCreditNames {
        name_key: Option<String>
    },
    r#"
    SELECT bae_artist_name_key(held.name) AS name_key, held.musicbrainz_artist_id,
        held.discogs_artist_id
    FROM import_candidate_album_artist_assignment AS held
    WHERE held.assignment_kind = 'credit'
"#);

sqlive_compose::fragment!(
    TrackCreditNames {
        name_key: Option<String>
    },
    r#"
    SELECT bae_artist_name_key(held.name) AS name_key, held.musicbrainz_artist_id,
        held.discogs_artist_id
    FROM import_candidate_track_artist_assignment AS held
    WHERE held.assignment_kind = 'credit'
"#);
