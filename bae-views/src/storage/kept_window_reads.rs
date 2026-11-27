//! A window of the Storage Manager list under the Local or Uploaded filter,
//! one compiled read for each sort, as [`super::window_reads`] reads the whole
//! list. Each reads from the sort's index after `stored`.

// The window of the Local or Uploaded list sorted by album title ascending.
sqlive_compose::read!(
    writer KeptWindowTitle(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.album_title_sort_key, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by album title descending.
sqlive_compose::read!(
    writer KeptWindowTitleBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_title_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by artist ascending.
sqlive_compose::read!(
    writer KeptWindowArtist(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_artist, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by artist descending.
sqlive_compose::read!(
    writer KeptWindowArtistBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_artist_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by media ascending.
sqlive_compose::read!(
    writer KeptWindowMedia(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_media, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by media descending.
sqlive_compose::read!(
    writer KeptWindowMediaBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_media_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by file count ascending.
sqlive_compose::read!(
    writer KeptWindowFiles(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.file_count, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by file count descending.
sqlive_compose::read!(
    writer KeptWindowFilesBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_files_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by size ascending.
sqlive_compose::read!(
    writer KeptWindowSize(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.total_size, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by size descending.
sqlive_compose::read!(
    writer KeptWindowSizeBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_size_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by storage ascending.
sqlive_compose::read!(
    writer KeptWindowStorage(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.storage_order, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by storage descending.
sqlive_compose::read!(
    writer KeptWindowStorageBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_storage_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by date added, oldest first.
sqlive_compose::read!(
    writer KeptWindowAdded(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.created_at, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// The window of the Local or Uploaded list sorted by date added, newest first.
sqlive_compose::read!(
    writer KeptWindowAddedBack(
        stored: i64,
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
    WHERE entry.stored = :stored
      AND (entry.sort_added_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          >= (:key_0, :key_1, :key_2, :key_3)
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);
