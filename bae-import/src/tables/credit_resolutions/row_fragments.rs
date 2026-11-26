//! The credit resolutions' rows that other domains' fragments spread to reach
//! them.

sqlive_compose::fragment!(
    StoredCreditAnswers { artists: String },
    r#"
    SELECT resolution.name_key, resolution.musicbrainz_artist_id, resolution.discogs_artist_id,(
        SELECT json_group_array(json_object('artist_id', named.artist_id, 'name', named.name,
            'sort_name', named.sort_name, 'musicbrainz_artist_id', named.musicbrainz_artist_id,
            'discogs_artist_id', named.discogs_artist_id) ORDER BY named.position)
        FROM import_credit_resolution_artist AS named
        WHERE named.resolution = resolution.id
    ) AS artists
    FROM import_credit_resolution AS resolution
"#
);
