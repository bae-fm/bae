//! A page of the flat album grid as one compiled read for each sort it can
//! sort by first. Each picks the page's albums by their order alone, then
//! reads the cards of the picked ones.

// A page of the flat grid sorted first by title ascending.
coven::read!(
    writer FlatPageTitle(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.title_sort_key AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.title_sort_key, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by title descending.
coven::read!(
    writer FlatPageTitleBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_title_back AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_title_back, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by artist ascending.
coven::read!(
    writer FlatPageArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_artist AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_artist, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by artist descending.
coven::read!(
    writer FlatPageArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_artist_back AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_artist_back, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by year ascending.
coven::read!(
    writer FlatPageYear(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_year AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_year, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by year descending.
coven::read!(
    writer FlatPageYearBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_year_back AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_year_back, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by date added, oldest first.
coven::read!(
    writer FlatPageAdded(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.created_at AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.created_at, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);

// A page of the flat grid sorted first by date added, newest first.
coven::read!(
    writer FlatPageAddedBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT card.*
    FROM (SELECT entry.sort_added_back AS key_0, entry.second AS key_1,
                  entry.second_back AS key_2, entry.third AS key_3,
                  entry.third_back AS key_4, entry.fourth AS key_5,
                  entry.fourth_back AS key_6, entry.album_id AS key_7
          FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
          ORDER BY entry.sort_added_back, entry.second, entry.second_back,
                   entry.third, entry.third_back, entry.fourth,
                   entry.fourth_back, entry.album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = picked.key_7
    ORDER BY picked.key_0, picked.key_1, picked.key_2, picked.key_3,
             picked.key_4, picked.key_5, picked.key_6, picked.key_7
"#
);
