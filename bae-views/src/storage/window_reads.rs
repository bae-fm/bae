//! A window of the whole Storage Manager list, one compiled read for each
//! sort: the releases from the window's first key on, in the sort's order,
//! each with its key and its row. Each reads from the sort's index.

// The window of the whole list sorted by album title ascending.
coven::read!(
    writer AllWindowTitle(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.album_title_sort_key AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.album_title_sort_key, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by album title descending.
coven::read!(
    writer AllWindowTitleBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_title_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_title_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by artist ascending.
coven::read!(
    writer AllWindowArtist(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_artist, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by artist descending.
coven::read!(
    writer AllWindowArtistBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_artist_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by media ascending.
coven::read!(
    writer AllWindowMedia(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_media, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by media descending.
coven::read!(
    writer AllWindowMediaBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_media_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by file count ascending.
coven::read!(
    writer AllWindowFiles(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.file_count AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.file_count, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by file count descending.
coven::read!(
    writer AllWindowFilesBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_files_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_files_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by size ascending.
coven::read!(
    writer AllWindowSize(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.total_size AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.total_size, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by size descending.
coven::read!(
    writer AllWindowSizeBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_size_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_size_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by storage ascending.
coven::read!(
    writer AllWindowStorage(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.storage_order AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.storage_order, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by storage descending.
coven::read!(
    writer AllWindowStorageBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_storage_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_storage_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by date added, oldest first.
coven::read!(
    writer AllWindowAdded(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.created_at AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.created_at, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the whole list sorted by date added, newest first.
coven::read!(
    writer AllWindowAddedBack(
        limit: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_added_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3,
           shown.*
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE (entry.sort_added_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);
