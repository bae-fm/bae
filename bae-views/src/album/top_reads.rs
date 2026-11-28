//! The album grid's window from its top as one compiled read for each sort
//! it can sort by first, flat or grouped by artist: [`super::window_reads`]
//! with no key to start from.

// [`super::window_reads::FlatWindowTitle`] from the grid's top.
coven::read!(
    writer FlatTopTitle(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.title_sort_key, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowTitleBack`] from the grid's top.
coven::read!(
    writer FlatTopTitleBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_title_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowArtist`] from the grid's top.
coven::read!(
    writer FlatTopArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowArtistBack`] from the grid's top.
coven::read!(
    writer FlatTopArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowYear`] from the grid's top.
coven::read!(
    writer FlatTopYear(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_year, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowYearBack`] from the grid's top.
coven::read!(
    writer FlatTopYearBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_year_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowAdded`] from the grid's top.
coven::read!(
    writer FlatTopAdded(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.created_at, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::FlatWindowAddedBack`] from the grid's top.
coven::read!(
    writer FlatTopAddedBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_added_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::GroupedWindowArtist`] from the grid's top.
coven::read!(
    writer GroupedTopArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_artist, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);

// [`super::window_reads::GroupedWindowArtistBack`] from the grid's top.
coven::read!(
    writer GroupedTopArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        limit: i64
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
    ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
             entry.third, entry.third_back, entry.fourth,
             entry.fourth_back, entry.album_id
    LIMIT :limit
"#
);
