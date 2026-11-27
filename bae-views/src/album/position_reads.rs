//! How many rows of the album grid come before the row at a key, as one
//! compiled read for each sort the grid can sort by first, over the terms its
//! window reads.

// The rows before a key in the flat grid sorted first by title ascending.
sqlive_compose::read!(
    FlatBeforeTitle(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.title_sort_key, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by title descending.
sqlive_compose::read!(
    FlatBeforeTitleBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_title_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by artist ascending.
sqlive_compose::read!(
    FlatBeforeArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_artist, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by artist descending.
sqlive_compose::read!(
    FlatBeforeArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_artist_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by year ascending.
sqlive_compose::read!(
    FlatBeforeYear(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_year, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by year descending.
sqlive_compose::read!(
    FlatBeforeYearBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_year_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by date added, oldest first.
sqlive_compose::read!(
    FlatBeforeAdded(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.created_at, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the flat grid sorted first by date added, newest first.
sqlive_compose::read!(
    FlatBeforeAddedBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::FlatAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_added_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the grid grouped by artist sorted first by the artists ascending.
sqlive_compose::read!(
    GroupedBeforeArtist(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_artist, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);

// The rows before a key in the grid grouped by artist sorted first by the artists descending.
sqlive_compose::read!(
    GroupedBeforeArtistBack(
        first: String,
        second: String,
        third: String,
        fourth: String,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue,
        key_5: bae_store::list_windows::SortValue,
        key_6: bae_store::list_windows::SortValue,
        key_7: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::album::order_fragments::GroupedAlbumOrder(
             first: :first, second: :second, third: :third, fourth: :fourth) AS entry
    WHERE (entry.sort_artist_back, entry.second, entry.second_back,
          entry.third, entry.third_back, entry.fourth,
          entry.fourth_back, entry.album_id)
          < (:key_0, :key_1, :key_2, :key_3,
          :key_4, :key_5, :key_6, :key_7)
"#
);
