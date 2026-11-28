//! The key of the row at a position of the album grid, or of its last row
//! when it ends before the position, as one compiled read for each sort the
//! grid can sort by first. An empty grid has no row: SQLite reads its offset
//! of -1 as 0.

// The key at a position of the flat grid sorted first by title ascending.
coven::read!(
    writer FlatKeyAtTitle(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.title_sort_key AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.title_sort_key, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by title descending.
coven::read!(
    writer FlatKeyAtTitleBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_title_back AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_title_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by artist ascending.
coven::read!(
    writer FlatKeyAtArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by artist descending.
coven::read!(
    writer FlatKeyAtArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist_back AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by year ascending.
coven::read!(
    writer FlatKeyAtYear(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_year AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_year, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by year descending.
coven::read!(
    writer FlatKeyAtYearBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_year_back AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_year_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by date added, oldest first.
coven::read!(
    writer FlatKeyAtAdded(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.created_at AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.created_at, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the flat grid sorted first by date added, newest first.
coven::read!(
    writer FlatKeyAtAddedBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_added_back AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_added_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_listing AS counted) - 1)
"#
);

// The key at a position of the grid grouped by artist sorted first by the artists ascending.
coven::read!(
    writer GroupedKeyAtArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_artist_link AS counted) - 1)
"#
);

// The key at a position of the grid grouped by artist sorted first by the artists descending.
coven::read!(
    writer GroupedKeyAtArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist_back AS key_0, entry.second AS key_1,
           entry.second_back AS key_2, entry.third AS key_3,
           entry.third_back AS key_4, entry.fourth AS key_5,
           entry.fourth_back AS key_6, entry.album_id AS key_7
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT 1 OFFSET min(:position, (SELECT COUNT(*) FROM album_artist_link AS counted) - 1)
"#
);
