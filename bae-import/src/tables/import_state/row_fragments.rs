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

sqlive_compose::fragment!(
    DraftCreditRows(key: crate::CandidateKey) {
        part: i64,
        track_id: Option<String>,
        position: i64,
        assignment_kind: String,
        artist_id: Option<String>,
        name: Option<String>,
        sort_name: Option<String>,
        musicbrainz_artist_id: Option<String>,
        discogs_artist_id: Option<String>,
        credit_key: Option<String>,
        credited_as: Option<String>,
        join_phrase: Option<String>
    },
    r#"
    SELECT 0 AS part, NULL AS track_id, a.position AS position,
        a.assignment_kind, a.artist_id, a.name, a.sort_name, a.musicbrainz_artist_id,
        a.discogs_artist_id, a.credit_key, a.credited_as, a.join_phrase
    FROM import_candidate_album_artist_assignment AS a
    WHERE a.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
    )
    UNION ALL SELECT 1, t.track_id, a.position, a.assignment_kind,
        a.artist_id, a.name, a.sort_name, a.musicbrainz_artist_id, a.discogs_artist_id,
        a.credit_key, a.credited_as, a.join_phrase
    FROM import_candidate_track_artist_assignment AS a
    JOIN import_candidate_track AS t ON t.content_hash = a.content_hash AND t.track_id =
        a.track_id
    WHERE a.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
    ) AND t.artist_assignment_kind = 'explicit'
"#);
