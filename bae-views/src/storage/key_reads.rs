//! The key of the release at a position of the Storage Manager list, or of its
//! last row when it ends before the position, one compiled read for each sort,
//! of the whole list and of the Local or Uploaded one. An empty list has no
//! row: SQLite reads its offset of -1 as 0.

// The key at a position of the whole list sorted by album title ascending.
sqlive_compose::read!(
    writer AllKeyAtTitle(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.album_title_sort_key AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by album title descending.
sqlive_compose::read!(
    writer AllKeyAtTitleBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_title_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by artist ascending.
sqlive_compose::read!(
    writer AllKeyAtArtist(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by artist descending.
sqlive_compose::read!(
    writer AllKeyAtArtistBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by media ascending.
sqlive_compose::read!(
    writer AllKeyAtMedia(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by media descending.
sqlive_compose::read!(
    writer AllKeyAtMediaBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by file count ascending.
sqlive_compose::read!(
    writer AllKeyAtFiles(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.file_count AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by file count descending.
sqlive_compose::read!(
    writer AllKeyAtFilesBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_files_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by size ascending.
sqlive_compose::read!(
    writer AllKeyAtSize(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.total_size AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by size descending.
sqlive_compose::read!(
    writer AllKeyAtSizeBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_size_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by storage ascending.
sqlive_compose::read!(
    writer AllKeyAtStorage(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.storage_order AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by storage descending.
sqlive_compose::read!(
    writer AllKeyAtStorageBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_storage_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by date added, oldest first.
sqlive_compose::read!(
    writer AllKeyAtAdded(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.created_at AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the whole list sorted by date added, newest first.
sqlive_compose::read!(
    writer AllKeyAtAddedBack(
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_added_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by album title ascending.
sqlive_compose::read!(
    writer KeptKeyAtTitle(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.album_title_sort_key AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.album_title_sort_key, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by album title descending.
sqlive_compose::read!(
    writer KeptKeyAtTitleBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_title_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_title_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by artist ascending.
sqlive_compose::read!(
    writer KeptKeyAtArtist(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_artist, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by artist descending.
sqlive_compose::read!(
    writer KeptKeyAtArtistBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_artist_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_artist_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by media ascending.
sqlive_compose::read!(
    writer KeptKeyAtMedia(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_media, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by media descending.
sqlive_compose::read!(
    writer KeptKeyAtMediaBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_media_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_media_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by file count ascending.
sqlive_compose::read!(
    writer KeptKeyAtFiles(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.file_count AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.file_count, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by file count descending.
sqlive_compose::read!(
    writer KeptKeyAtFilesBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_files_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_files_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by size ascending.
sqlive_compose::read!(
    writer KeptKeyAtSize(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.total_size AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.total_size, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by size descending.
sqlive_compose::read!(
    writer KeptKeyAtSizeBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_size_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_size_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by storage ascending.
sqlive_compose::read!(
    writer KeptKeyAtStorage(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.storage_order AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.storage_order, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by storage descending.
sqlive_compose::read!(
    writer KeptKeyAtStorageBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_storage_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_storage_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by date added, oldest first.
sqlive_compose::read!(
    writer KeptKeyAtAdded(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.created_at AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.created_at, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);

// The key at a position of the Local or Uploaded list sorted by date added, newest first.
sqlive_compose::read!(
    writer KeptKeyAtAddedBack(
        stored: i64,
        position: i64
    ) {
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    },
    r#"
    SELECT entry.sort_added_back AS key_0, entry.album_title_sort_key AS key_1,
           entry.created_at AS key_2, entry.release_id AS key_3
    FROM release_listing AS entry
    WHERE entry.stored = :stored
    ORDER BY entry.sort_added_back, entry.album_title_sort_key,
             entry.created_at, entry.release_id
    LIMIT 1 OFFSET min(:position,
        (SELECT COUNT(*) FROM release_listing AS counted WHERE counted.stored = :stored) - 1)
"#
);
