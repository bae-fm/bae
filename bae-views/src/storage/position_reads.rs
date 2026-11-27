//! How many releases of the Storage Manager list come before the row at a
//! key, one compiled read for each sort, of the whole list and of the Local or
//! Uploaded one, over the terms its window reads.

// The rows before a key in the whole list sorted by album title ascending.
sqlive_compose::read!(
    writer AllBeforeTitle(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.album_title_sort_key, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by album title descending.
sqlive_compose::read!(
    writer AllBeforeTitleBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_title_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by artist ascending.
sqlive_compose::read!(
    writer AllBeforeArtist(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_artist, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by artist descending.
sqlive_compose::read!(
    writer AllBeforeArtistBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_artist_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by media ascending.
sqlive_compose::read!(
    writer AllBeforeMedia(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_media, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by media descending.
sqlive_compose::read!(
    writer AllBeforeMediaBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_media_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by file count ascending.
sqlive_compose::read!(
    writer AllBeforeFiles(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.file_count, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by file count descending.
sqlive_compose::read!(
    writer AllBeforeFilesBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_files_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by size ascending.
sqlive_compose::read!(
    writer AllBeforeSize(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.total_size, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by size descending.
sqlive_compose::read!(
    writer AllBeforeSizeBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_size_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by storage ascending.
sqlive_compose::read!(
    writer AllBeforeStorage(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.storage_order, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by storage descending.
sqlive_compose::read!(
    writer AllBeforeStorageBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_storage_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by date added, oldest first.
sqlive_compose::read!(
    writer AllBeforeAdded(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.created_at, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the whole list sorted by date added, newest first.
sqlive_compose::read!(
    writer AllBeforeAddedBack(
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE (entry.sort_added_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by album title ascending.
sqlive_compose::read!(
    writer KeptBeforeTitle(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.album_title_sort_key, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by album title descending.
sqlive_compose::read!(
    writer KeptBeforeTitleBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_title_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by artist ascending.
sqlive_compose::read!(
    writer KeptBeforeArtist(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_artist, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by artist descending.
sqlive_compose::read!(
    writer KeptBeforeArtistBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_artist_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by media ascending.
sqlive_compose::read!(
    writer KeptBeforeMedia(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_media, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by media descending.
sqlive_compose::read!(
    writer KeptBeforeMediaBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_media_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by file count ascending.
sqlive_compose::read!(
    writer KeptBeforeFiles(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.file_count, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by file count descending.
sqlive_compose::read!(
    writer KeptBeforeFilesBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_files_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by size ascending.
sqlive_compose::read!(
    writer KeptBeforeSize(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.total_size, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by size descending.
sqlive_compose::read!(
    writer KeptBeforeSizeBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_size_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by storage ascending.
sqlive_compose::read!(
    writer KeptBeforeStorage(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.storage_order, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by storage descending.
sqlive_compose::read!(
    writer KeptBeforeStorageBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_storage_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by date added, oldest first.
sqlive_compose::read!(
    writer KeptBeforeAdded(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.created_at, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);

// The rows before a key in the Local or Uploaded list sorted by date added, newest first.
sqlive_compose::read!(
    writer KeptBeforeAddedBack(
        stored: i64,
        key_0: bae_store::list_windows::SortValue,
        key_1: bae_store::list_windows::SortValue,
        key_2: bae_store::list_windows::SortValue,
        key_3: bae_store::list_windows::SortValue
    ) {
        rows_before: u64
    },
    r#"
    SELECT COUNT(*) AS rows_before
    FROM release_listing AS entry
    WHERE entry.stored = :stored
      AND (entry.sort_added_back, entry.album_title_sort_key,
           entry.created_at, entry.release_id)
          < (:key_0, :key_1, :key_2, :key_3)
"#
);
