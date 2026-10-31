//! What library search finds of albums (§2.3), as SQL conditions on an
//! album: read from the search tables' indexes for the albums a pattern
//! finds, or tested on one album's own text in those tables.

/// The albums whose credits library search finds (§2.3) for the search
/// pattern the parameter `pattern` binds (see
/// [`crate::library::search::search_pattern`]): the name of the artist a
/// credit shows, or the other name it prints, holds it. Each part reads its
/// search table's index, and an album two credits find is listed twice, so
/// the list is only read through `IN`.
pub(super) fn albums_credits_find_sql(pattern: &str) -> String {
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
/// [`crate::library::search::search_pattern`]): its title, or the name of an
/// artist one of its credits shows or the other name a credit prints, holds
/// it. Each part reads its search table's index, so the test costs what the
/// search finds, read once per statement; an album two parts find is listed
/// twice, which the `IN` list holds once, so no part is sorted to merge them.
pub(super) fn album_matches_sql(album_id: &str, pattern: &str) -> String {
    format!(
        "{album_id} IN ( \
             SELECT album_id FROM album_search WHERE title GLOB {pattern} \
             UNION ALL {credits})",
        credits = albums_credits_find_sql(pattern),
    )
}

/// Whether the album `album_id` names has a credit library search finds for
/// the search pattern `pattern`, read from the search tables' indexes like
/// [`album_matches_sql`].
pub(super) fn album_credits_found_sql(album_id: &str, pattern: &str) -> String {
    format!("{album_id} IN ({})", albums_credits_find_sql(pattern))
}

/// Whether the title of the album `album_id` names holds the search pattern
/// `pattern`, tested on the text its search table keeps for it, which that
/// table's own matching tests (§2.3). The text is read by the album's entry
/// and `+` keeps the pattern from SQLite's search of the table's index, so
/// the test costs one lookup however many albums the pattern finds.
fn album_title_holds_sql(album_id: &str, pattern: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM album_search_entry title_entry \
             JOIN album_search title_text ON title_text.rowid = title_entry.entry \
             WHERE title_entry.album_id = {album_id} AND +title_text.title GLOB {pattern})"
    )
}

/// Whether the album `album_id` names is one library search finds for the
/// search pattern `pattern`, tested on that album alone: its title (see
/// [`album_title_holds_sql`]), then the names its credits show and print,
/// each read by its key from the text its search table keeps. The test costs
/// a few lookups however many albums the pattern finds, so a statement
/// testing the rows an index reaches reads only those.
pub(super) fn album_holds_sql(album_id: &str, pattern: &str) -> String {
    format!(
        "({title} \
          OR EXISTS (SELECT 1 FROM album_artists credit \
              JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
              JOIN artist_search_entry artist_entry \
                  ON artist_entry.artist_id = shown.shown_artist_id \
              JOIN artist_search artist_text ON artist_text.rowid = artist_entry.entry \
              WHERE credit.album_id = {album_id} AND +artist_text.name GLOB {pattern}) \
          OR EXISTS (SELECT 1 FROM album_artists credit \
              JOIN artist_credit_search_entry printed_entry \
                  ON printed_entry.artist_id = credit.artist_id \
                 AND printed_entry.name_key = credit.credit_key \
              JOIN artist_credit_search printed_text \
                  ON printed_text.rowid = printed_entry.entry \
              WHERE credit.album_id = {album_id} AND +printed_text.name GLOB {pattern}))",
        title = album_title_holds_sql(album_id, pattern),
    )
}
