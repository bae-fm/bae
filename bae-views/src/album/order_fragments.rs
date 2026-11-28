//! The album grid's order as SQL: its rows with the terms it sorts them by,
//! which the grid's window, page, position and key reads compare and sort, and
//! its cards numbered in that order.

// The rows of the flat grid, each album, with every value it
// can sort them by first, and for each of the four places of its sort, the two
// terms the code that place's parameter names gives: the column that sorts the
// code's field its way, as the first term when the column compares by its
// bytes and as the second, in the `descending` collation, when it compares by
// that. The other term of a pair, and both for the empty code of an unused
// place, hold one value for every row, which is never NULL: a row value with a
// NULL compares as neither before nor after another.
coven::fragment!(
    FlatAlbumOrder(first: String, second: String, third: String, fourth: String) {
        first: bae_store::list_windows::SortValue, first_back: bae_store::list_windows::SortValue,
        second: bae_store::list_windows::SortValue, second_back: bae_store::list_windows::SortValue,
        third: bae_store::list_windows::SortValue, third_back: bae_store::list_windows::SortValue,
        fourth: bae_store::list_windows::SortValue, fourth_back: bae_store::list_windows::SortValue
    },
    r#"
    SELECT sorted.album_id, sorted.title_sort_key, sorted.sort_title_back,
           sorted.sort_artist, sorted.sort_artist_back, sorted.sort_year,
           sorted.sort_year_back, sorted.created_at, sorted.sort_added_back,
           CASE :first
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS first,
           CASE :first
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS first_back,
           CASE :second
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS second,
           CASE :second
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS second_back,
           CASE :third
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS third,
           CASE :third
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS third_back,
           CASE :fourth
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS fourth,
           CASE :fourth
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS fourth_back
    FROM album_listing AS sorted
"#
);

// The rows of the grid grouped by artist, each album under each artist its credits show, with every value it
// can sort them by first, and for each of the four places of its sort, the two
// terms the code that place's parameter names gives: the column that sorts the
// code's field its way, as the first term when the column compares by its
// bytes and as the second, in the `descending` collation, when it compares by
// that. The other term of a pair, and both for the empty code of an unused
// place, hold one value for every row, which is never NULL: a row value with a
// NULL compares as neither before nor after another.
coven::fragment!(
    GroupedAlbumOrder(first: String, second: String, third: String, fourth: String) {
        first: bae_store::list_windows::SortValue, first_back: bae_store::list_windows::SortValue,
        second: bae_store::list_windows::SortValue, second_back: bae_store::list_windows::SortValue,
        third: bae_store::list_windows::SortValue, third_back: bae_store::list_windows::SortValue,
        fourth: bae_store::list_windows::SortValue, fourth_back: bae_store::list_windows::SortValue
    },
    r#"
    SELECT sorted.artist_id, sorted.album_id, sorted.title_sort_key, sorted.sort_title_back,
           sorted.sort_artist, sorted.sort_artist_back, sorted.sort_year,
           sorted.sort_year_back, sorted.created_at, sorted.sort_added_back,
           CASE :first
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS first,
           CASE :first
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS first_back,
           CASE :second
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS second,
           CASE :second
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS second_back,
           CASE :third
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS third,
           CASE :third
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS third_back,
           CASE :fourth
               WHEN 'title' THEN sorted.title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'year' THEN sorted.sort_year
               WHEN 'year_back' THEN sorted.sort_year_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS fourth,
           CASE :fourth
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS fourth_back
    FROM album_artist_link AS sorted
"#
);

// The cards of the flat grid: each card's album, the artist
// whose group holds it, none here, and its position counted from 0 in the
// grid's order.
coven::fragment!(
    FlatAlbumCards(first: String, second: String, third: String, fourth: String) {
        section_id: Option<String>, position: u64
    },
    r#"
    SELECT entry.album_id, NULL AS section_id,
           ROW_NUMBER() OVER (
               ORDER BY entry.first, entry.first_back, entry.second, entry.second_back,
                        entry.third, entry.third_back, entry.fourth, entry.fourth_back,
                        entry.album_id) - 1 AS position
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
"#
);

// The cards of the grid grouped by artist: each card's album, the artist
// whose group holds it, and its position counted from 0 in the
// grid's order.
coven::fragment!(
    GroupedAlbumCards(first: String, second: String, third: String, fourth: String) {
        position: u64
    },
    r#"
    SELECT entry.album_id, entry.artist_id AS section_id,
           ROW_NUMBER() OVER (
               ORDER BY entry.first, entry.first_back, entry.second, entry.second_back,
                        entry.third, entry.third_back, entry.fourth, entry.fourth_back,
                        entry.album_id) - 1 AS position
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
"#
);
