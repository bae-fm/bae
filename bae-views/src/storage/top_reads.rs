//! The Storage Manager list's window from its top, whole or under the Local
//! or Uploaded filter, as one compiled read for each sort:
//! [`super::window_reads`] and [`super::kept_window_reads`] with no key to
//! start from.

// [`super::window_reads::AllWindowTitle`] from the list's top.
coven::read!(
    writer AllTopTitle(
        limit: i64
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
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowTitleBack`] from the list's top.
coven::read!(
    writer AllTopTitleBack(
        limit: i64
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
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowArtist`] from the list's top.
coven::read!(
    writer AllTopArtist(
        limit: i64
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
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowArtistBack`] from the list's top.
coven::read!(
    writer AllTopArtistBack(
        limit: i64
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
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowMedia`] from the list's top.
coven::read!(
    writer AllTopMedia(
        limit: i64
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
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowMediaBack`] from the list's top.
coven::read!(
    writer AllTopMediaBack(
        limit: i64
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
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowFiles`] from the list's top.
coven::read!(
    writer AllTopFiles(
        limit: i64
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
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowFilesBack`] from the list's top.
coven::read!(
    writer AllTopFilesBack(
        limit: i64
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
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowSize`] from the list's top.
coven::read!(
    writer AllTopSize(
        limit: i64
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
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowSizeBack`] from the list's top.
coven::read!(
    writer AllTopSizeBack(
        limit: i64
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
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowStorage`] from the list's top.
coven::read!(
    writer AllTopStorage(
        limit: i64
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
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowStorageBack`] from the list's top.
coven::read!(
    writer AllTopStorageBack(
        limit: i64
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
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowAdded`] from the list's top.
coven::read!(
    writer AllTopAdded(
        limit: i64
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
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::window_reads::AllWindowAddedBack`] from the list's top.
coven::read!(
    writer AllTopAddedBack(
        limit: i64
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
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowTitle`] from the list's top.
coven::read!(
    writer KeptTopTitle(
        stored: i64,
        limit: i64
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
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowTitleBack`] from the list's top.
coven::read!(
    writer KeptTopTitleBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowArtist`] from the list's top.
coven::read!(
    writer KeptTopArtist(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowArtistBack`] from the list's top.
coven::read!(
    writer KeptTopArtistBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowMedia`] from the list's top.
coven::read!(
    writer KeptTopMedia(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowMediaBack`] from the list's top.
coven::read!(
    writer KeptTopMediaBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowFiles`] from the list's top.
coven::read!(
    writer KeptTopFiles(
        stored: i64,
        limit: i64
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
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowFilesBack`] from the list's top.
coven::read!(
    writer KeptTopFilesBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowSize`] from the list's top.
coven::read!(
    writer KeptTopSize(
        stored: i64,
        limit: i64
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
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowSizeBack`] from the list's top.
coven::read!(
    writer KeptTopSizeBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowStorage`] from the list's top.
coven::read!(
    writer KeptTopStorage(
        stored: i64,
        limit: i64
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
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowStorageBack`] from the list's top.
coven::read!(
    writer KeptTopStorageBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowAdded`] from the list's top.
coven::read!(
    writer KeptTopAdded(
        stored: i64,
        limit: i64
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
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);

// [`super::kept_window_reads::KeptWindowAddedBack`] from the list's top.
coven::read!(
    writer KeptTopAddedBack(
        stored: i64,
        limit: i64
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
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT :limit
"#
);
