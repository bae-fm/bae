//! What library search finds of albums and artists (§2.3), as SQL
//! conditions read from the search tables' indexes.

/// The ids of the artists library search finds (§2.3) for the search
/// pattern the parameter `pattern` binds (see
/// [`bae_util::search_text::search_pattern`]): those whose name or sort name
/// holds it, and the artist each other printed name holding it shows as. Each
/// part reads its search table's index; the artist picker finds by these too.
pub(crate) fn artists_found_sql(pattern: &str) -> String {
    format!(
        "SELECT artist_id FROM artist_search WHERE name GLOB {pattern} \
         UNION SELECT artist_id FROM artist_search WHERE sort_name GLOB {pattern} \
         UNION SELECT shown.shown_artist_id FROM artist_credit_search matched_credit \
             JOIN artist_shown_as shown ON shown.artist_id = matched_credit.artist_id \
             WHERE matched_credit.name GLOB {pattern}"
    )
}

// The artists [`artists_found_sql`] finds, for the compiled statements.
sqlive_compose::fragment!(
    ArtistsFound(pattern: String) { artist_id: Option<String> },
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

/// The albums whose credits library search finds (§2.3) for the search
/// pattern the parameter `pattern` binds (see
/// [`bae_util::search_text::search_pattern`]): the name of the artist a
/// credit shows, or the other name it prints, holds it. Each part reads its
/// search table's index, and an album two credits find is listed twice, so
/// the list is only read through `IN`.
pub(crate) fn albums_credits_find_sql(pattern: &str) -> String {
    format!(
        "SELECT credit.album_id FROM artist_search matched_artist \
         JOIN artist_shown_as shown ON shown.shown_artist_id = matched_artist.artist_id \
         JOIN album_artists credit ON credit.artist_id = shown.artist_id \
         WHERE matched_artist.name GLOB {pattern} \
         UNION ALL \
         SELECT credit.album_id FROM artist_credit_search matched_credit \
         JOIN album_artists credit \
             ON credit.artist_id = matched_credit.artist_id \
            AND credit.credit_key = matched_credit.name_key \
         WHERE matched_credit.name GLOB {pattern}"
    )
}

/// Whether the album `album_id` names is one library search finds (§2.3)
/// for the search pattern the parameter `pattern` binds (see
/// [`bae_util::search_text::search_pattern`]): its title, or the name of an
/// artist one of its credits shows or the other name a credit prints, holds
/// it. Each part reads its search table's index, so the test costs what the
/// search finds, read once per statement; an album two parts find is listed
/// twice, which the `IN` list holds once, so no part is sorted to merge them.
pub(crate) fn album_matches_sql(album_id: &str, pattern: &str) -> String {
    format!(
        "{album_id} IN ( \
             SELECT album_id FROM album_search WHERE title GLOB {pattern} \
             UNION ALL {credits})",
        credits = albums_credits_find_sql(pattern),
    )
}

// The albums [`album_matches_sql`] finds for the pattern, for the compiled
// statements, none for no pattern. An album two parts find is listed twice, so
// the list is only read through `IN`.
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
