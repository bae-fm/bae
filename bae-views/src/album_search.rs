//! What library search finds of albums and artists (§2.3), as fragments that
//! read the search tables' indexes.

// The ids of the artists library search finds (§2.3) for the search pattern
// `pattern` (see [`bae_util::search_text::search_pattern`]), none for no
// pattern: those whose name or sort name holds it, and the artist each other
// printed name holding it shows as. Each part reads its search table's index;
// the artist picker finds by these too.
sqlive_compose::fragment!(
    ArtistsFound(pattern: Option<String>) { artist_id: Option<String> },
    r#"
    SELECT named.artist_id FROM artist_search AS named WHERE named.name GLOB :pattern
    UNION
    SELECT sorted.artist_id FROM artist_search AS sorted WHERE sorted.sort_name GLOB :pattern
    UNION
    SELECT shown.shown_artist_id
    FROM artist_credit_search AS matched_credit
    JOIN @bae_records::records::browse_fragments::ShownArtist AS shown
      ON shown.artist_id = matched_credit.artist_id
    WHERE matched_credit.name GLOB :pattern
"#
);

// The ids of the albums library search finds (§2.3) for the search pattern
// `pattern` (see [`bae_util::search_text::search_pattern`]), none for no
// pattern: those whose title, or the name of an artist one of whose credits
// shows or the other name a credit prints, holds it. Each part reads its search
// table's index, so the read costs what the search finds. An album two parts
// find is listed twice, so the list is only read through `IN` or `DISTINCT`.
sqlive_compose::fragment!(
    AlbumsFound(pattern: Option<String>) { album_id: Option<String> },
    r#"
    SELECT titled.album_id FROM album_search AS titled WHERE titled.title GLOB :pattern
    UNION ALL
    SELECT credit.album_id
    FROM artist_search AS matched_artist
    JOIN @bae_records::records::browse_fragments::ShownArtist AS shown
      ON shown.shown_artist_id = matched_artist.artist_id
    JOIN @bae_records::records::spread_fragments::AlbumArtistRow AS credit
      ON credit.artist_id = shown.artist_id
    WHERE matched_artist.name GLOB :pattern
    UNION ALL
    SELECT credit.album_id
    FROM artist_credit_search AS matched_credit
    JOIN @bae_records::records::spread_fragments::AlbumArtistRow AS credit
      ON credit.artist_id = matched_credit.artist_id
     AND credit.credit_key = matched_credit.name_key
    WHERE matched_credit.name GLOB :pattern
"#
);
