//! The window, the rows before a key and the key at a position of the Storage
//! Manager list a search narrows. Each reaches the releases by the albums the
//! search finds and sorts only those, so the sort is a parameter.

// The window of the list a search narrows: the releases from the window's
// first key on, in the sort's order, each with its key and its row. The
// subquery picks the window's releases, so only their rows are read.
sqlive_compose::read!(
    writer FoundWindow(
        sort: String,
        search: Option<String>,
        stored: Option<i64>,
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue
    },
    r#"
    SELECT picked.lead AS key_0, picked.lead_back AS key_1, picked.album_title_sort_key AS key_2,
           picked.created_at AS key_3, picked.release_id AS key_4,
           shown.*
    FROM (SELECT sorted_found.lead, sorted_found.lead_back, sorted_found.album_title_sort_key,
                 sorted_found.created_at, sorted_found.release_id
          FROM @crate::storage::order_fragments::StorageOrder(sort: :sort) AS sorted_found
          WHERE sorted_found.album_id IN (
                    SELECT found.album_id
                    FROM @crate::album_search::AlbumsFound(pattern: :search) AS found)
            AND (:stored IS NULL OR sorted_found.stored = :stored)
            AND (sorted_found.lead, sorted_found.lead_back, sorted_found.album_title_sort_key,
                 sorted_found.created_at, sorted_found.release_id)
                >= (:key_0, :key_1, :key_2, :key_3, :key_4)
          ORDER BY sorted_found.lead, sorted_found.lead_back, sorted_found.album_title_sort_key,
                   sorted_found.created_at, sorted_found.release_id
          LIMIT :limit) AS picked
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = picked.release_id
    ORDER BY picked.lead, picked.lead_back, picked.album_title_sort_key,
             picked.created_at, picked.release_id
"#
);

// [`FoundWindow`] from the list's top.
sqlive_compose::read!(
    writer FoundTop(
        sort: String,
        search: Option<String>,
        stored: Option<i64>,
        limit: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue
    },
    r#"
    SELECT picked.lead AS key_0, picked.lead_back AS key_1, picked.album_title_sort_key AS key_2,
           picked.created_at AS key_3, picked.release_id AS key_4,
           shown.*
    FROM (SELECT sorted_found.lead, sorted_found.lead_back, sorted_found.album_title_sort_key,
                 sorted_found.created_at, sorted_found.release_id
          FROM @crate::storage::order_fragments::StorageOrder(sort: :sort) AS sorted_found
          WHERE sorted_found.album_id IN (
                    SELECT found.album_id
                    FROM @crate::album_search::AlbumsFound(pattern: :search) AS found)
            AND (:stored IS NULL OR sorted_found.stored = :stored)
          ORDER BY sorted_found.lead, sorted_found.lead_back, sorted_found.album_title_sort_key,
                   sorted_found.created_at, sorted_found.release_id
          LIMIT :limit) AS picked
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = picked.release_id
    ORDER BY picked.lead, picked.lead_back, picked.album_title_sort_key,
             picked.created_at, picked.release_id
"#
);

// How many releases of the list a search narrows come before the row at a key.
sqlive_compose::read!(
    writer FoundBefore(
        sort: String,
        search: Option<String>,
        stored: Option<i64>,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM @crate::storage::order_fragments::StorageOrder(sort: :sort) AS entry
    WHERE entry.album_id IN (
              SELECT found.album_id
              FROM @crate::album_search::AlbumsFound(pattern: :search) AS found)
      AND (:stored IS NULL OR entry.stored = :stored)
      AND (entry.lead, entry.lead_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3, :key_4)
"#
);

// The key at a position of the list a search narrows, or of its last row when
// it ends before it; no row for an empty list, since SQLite reads its offset
// of -1 as 0.
sqlive_compose::read!(
    writer FoundKeyAt(sort: String, search: Option<String>, stored: Option<i64>, position: i64) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue,
        key_4: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.lead AS key_0, entry.lead_back AS key_1, entry.album_title_sort_key AS key_2,
           entry.created_at AS key_3, entry.release_id AS key_4
    FROM @crate::storage::order_fragments::StorageOrder(sort: :sort) AS entry
    WHERE entry.album_id IN (
              SELECT found.album_id
              FROM @crate::album_search::AlbumsFound(pattern: :search) AS found)
      AND (:stored IS NULL OR entry.stored = :stored)
    ORDER BY entry.lead, entry.lead_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position, (
        SELECT COUNT(*)
        FROM release_listing AS counted
        WHERE counted.album_id IN (
                  SELECT found.album_id
                  FROM @crate::album_search::AlbumsFound(pattern: :search) AS found)
          AND (:stored IS NULL OR counted.stored = :stored)) - 1)
"#
);

// How many releases a search finds under a filter or none, and their files'
// total size.
sqlive_compose::read!(
    writer FoundTotals(search: Option<String>, stored: Option<i64>) { found: u64, total_size: u64 },
    r#"
    SELECT COUNT(*) AS found, COALESCE(SUM(entry.total_size), 0) AS total_size
    FROM release_listing AS entry
    WHERE entry.album_id IN (
              SELECT matched.album_id
              FROM @crate::album_search::AlbumsFound(pattern: :search) AS matched)
      AND (:stored IS NULL OR entry.stored = :stored)
"#
);
