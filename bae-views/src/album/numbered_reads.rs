//! The reads over the album grid's cards numbered in its order, flat or
//! grouped by artist: where an album's card is, the albums between two cards,
//! chosen albums in the grid's order and every album. Each numbers the whole
//! grid, so the sort it sorts by first is a parameter too.

// Where the album's card is in the flat grid: the card in the group of the
// artist `section` when that group holds one, else its first card. No row when
// the grid shows no card of the album.
sqlive_compose::read!(
    FlatAlbumPlaceRead(
        first: String,
        second: String,
        third: String,
        fourth: String,
        album: String,
        section: Option<String>
    ) {
        position: u64,
        section_id: Option<String>
    },
    r#"
    SELECT numbered.position, numbered.section_id
    FROM @crate::album::order_fragments::FlatAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered
    WHERE numbered.album_id = :album
    ORDER BY :section IS NOT NULL AND numbered.section_id = :section DESC, numbered.position
    LIMIT 1
"#
);

// The albums of the cards of the flat grid from one card to another, both
// included, in the grid's order, each album once. Each end is its album's card
// in the group of its section when that group holds one, else its album's
// first card. With no card of the first end's album the albums are the second
// end's alone, and with no card of the second end's there are none.
sqlive_compose::read!(
    FlatAlbumsBetween(
        first: String,
        second: String,
        third: String,
        fourth: String,
        from_album: String,
        from_section: Option<String>,
        to_album: String,
        to_section: Option<String>,
    ),
    r#"
    WITH card AS MATERIALIZED (
             SELECT numbered.album_id, numbered.section_id, numbered.position
             FROM @crate::album::order_fragments::FlatAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered),
         ends (low, high) AS (
             SELECT min(coalesce(f, t), t), max(coalesce(f, t), t)
             FROM (SELECT (SELECT numbered.position FROM card AS numbered
                           WHERE numbered.album_id = :from_album
    ORDER BY :from_section IS NOT NULL AND numbered.section_id = :from_section DESC, numbered.position
    LIMIT 1) AS f,
                          (SELECT numbered.position FROM card AS numbered
                           WHERE numbered.album_id = :to_album
    ORDER BY :to_section IS NOT NULL AND numbered.section_id = :to_section DESC, numbered.position
    LIMIT 1) AS t))
    SELECT card.album_id
    FROM card
    JOIN ends ON card.position BETWEEN ends.low AND ends.high
    GROUP BY card.album_id
    ORDER BY min(card.position)
"#
);

// The albums of `albums`, a JSON array of ids, that the flat grid lists,
// each once at its first card, in the grid's order.
sqlive_compose::read!(
    FlatAlbumsInOrder(first: String, second: String, third: String, fourth: String, albums: String),
    r#"
    WITH listed AS (
             SELECT numbered.album_id, min(numbered.position) AS position
             FROM @crate::album::order_fragments::FlatAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered
             WHERE numbered.album_id IN (SELECT picked.value FROM json_each(:albums) AS picked)
             GROUP BY numbered.album_id)
    SELECT summary.*
    FROM listed
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS summary
            ON summary.summary_album_id = listed.album_id
    ORDER BY listed.position
"#
);

// Where the album's card is in the grid grouped by artist: the card in the group of the
// artist `section` when that group holds one, else its first card. No row when
// the grid shows no card of the album.
sqlive_compose::read!(
    GroupedAlbumPlaceRead(
        first: String,
        second: String,
        third: String,
        fourth: String,
        album: String,
        section: Option<String>
    ) {
        position: u64
    },
    r#"
    SELECT numbered.position, numbered.section_id
    FROM @crate::album::order_fragments::GroupedAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered
    WHERE numbered.album_id = :album
    ORDER BY :section IS NOT NULL AND numbered.section_id = :section DESC, numbered.position
    LIMIT 1
"#
);

// The albums of the cards of the grid grouped by artist from one card to another, both
// included, in the grid's order, each album once. Each end is its album's card
// in the group of its section when that group holds one, else its album's
// first card. With no card of the first end's album the albums are the second
// end's alone, and with no card of the second end's there are none.
sqlive_compose::read!(
    GroupedAlbumsBetween(
        first: String,
        second: String,
        third: String,
        fourth: String,
        from_album: String,
        from_section: Option<String>,
        to_album: String,
        to_section: Option<String>,
    ),
    r#"
    WITH card AS MATERIALIZED (
             SELECT numbered.album_id, numbered.section_id, numbered.position
             FROM @crate::album::order_fragments::GroupedAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered),
         ends (low, high) AS (
             SELECT min(coalesce(f, t), t), max(coalesce(f, t), t)
             FROM (SELECT (SELECT numbered.position FROM card AS numbered
                           WHERE numbered.album_id = :from_album
    ORDER BY :from_section IS NOT NULL AND numbered.section_id = :from_section DESC, numbered.position
    LIMIT 1) AS f,
                          (SELECT numbered.position FROM card AS numbered
                           WHERE numbered.album_id = :to_album
    ORDER BY :to_section IS NOT NULL AND numbered.section_id = :to_section DESC, numbered.position
    LIMIT 1) AS t))
    SELECT card.album_id
    FROM card
    JOIN ends ON card.position BETWEEN ends.low AND ends.high
    GROUP BY card.album_id
    ORDER BY min(card.position)
"#
);

// The albums of `albums`, a JSON array of ids, that the grid grouped by artist lists,
// each once at its first card, in the grid's order.
sqlive_compose::read!(
    GroupedAlbumsInOrder(
        first: String,
        second: String,
        third: String,
        fourth: String,
        albums: String,
    ),
    r#"
    WITH listed AS (
             SELECT numbered.album_id, min(numbered.position) AS position
             FROM @crate::album::order_fragments::GroupedAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered
             WHERE numbered.album_id IN (SELECT picked.value FROM json_each(:albums) AS picked)
             GROUP BY numbered.album_id)
    SELECT summary.*
    FROM listed
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS summary
            ON summary.summary_album_id = listed.album_id
    ORDER BY listed.position
"#
);

// Every album, in the flat grid's order.
sqlive_compose::read!(
    EveryAlbum(first: String, second: String, third: String, fourth: String),
    r#"
    SELECT album.*
    FROM @crate::album::order_fragments::FlatAlbumCards(
             first: :first, second: :second, third: :third, fourth: :fourth) AS numbered
    CROSS JOIN @bae_records::records::fragments::AlbumRow AS album
            ON album.row_album_id = numbered.album_id
    ORDER BY numbered.position
"#
);
