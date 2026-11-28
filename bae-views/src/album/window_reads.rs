//! The album grid's window as one compiled read for each sort it can sort by
//! first, flat or grouped by artist. The sorts after the first are
//! parameters. Each is the grid's rows from the window's first key on, in the
//! order's terms, each row with its key and its album's card; grouped by
//! artist, with the album's other credits too.

// The window of the flat grid sorted first by title ascending.
coven::read!(
    writer FlatWindowTitle(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.title_sort_key, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.title_sort_key, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by title descending.
coven::read!(
    writer FlatWindowTitleBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_title_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_title_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by artist ascending.
coven::read!(
    writer FlatWindowArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_artist, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by artist descending.
coven::read!(
    writer FlatWindowArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_artist_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by year ascending.
coven::read!(
    writer FlatWindowYear(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_year, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_year, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by year descending.
coven::read!(
    writer FlatWindowYearBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_year_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_year_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by date added, oldest first.
coven::read!(
    writer FlatWindowAdded(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.created_at, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.created_at, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the flat grid sorted first by date added, newest first.
coven::read!(
    writer FlatWindowAddedBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    WHERE (entry.sort_added_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_added_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the grid grouped by artist sorted first by the artists ascending.
coven::read!(
    writer GroupedWindowArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*,
           beside.*
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    CROSS JOIN @bae_records::records::fragments::AlbumCreditsBeside AS beside
            ON beside.beside_album_id = entry.album_id
           AND beside.beside_artist_id = entry.artist_id
    WHERE (entry.sort_artist, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// The window of the grid grouped by artist sorted first by the artists descending.
coven::read!(
    writer GroupedWindowArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
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
           entry.fourth_back AS key_6, entry.album_id AS key_7,
           card.*,
           beside.*
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    CROSS JOIN @crate::album::card::AlbumCardRow AS card
            ON card.summary_album_id = entry.album_id
    CROSS JOIN @bae_records::records::fragments::AlbumCreditsBeside AS beside
            ON beside.beside_album_id = entry.album_id
           AND beside.beside_artist_id = entry.artist_id
    WHERE (entry.sort_artist_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          >= (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);
