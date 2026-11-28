//! The Storage Manager's order as SQL with its sort a parameter, for the
//! reads that sort only what they find or go through the whole list.

// Each release of the list with the two terms the sort `sort` names gives: the
// column that sorts it, as `lead` when the column compares by its bytes and as
// `lead_back`, in the `descending` collation, when it compares by that. The
// other of the two holds one value for every release, never NULL, so a row
// value of the order's terms compares.
coven::fragment!(
    StorageOrder(sort: String) {
        lead: bae_store::list_windows::SortValue,
        lead_back: bae_store::list_windows::SortValue
    },
    r#"
    SELECT sorted.release_id, sorted.album_id, sorted.stored, sorted.album_title_sort_key,
           sorted.created_at,
           CASE :sort
               WHEN 'title' THEN sorted.album_title_sort_key
               WHEN 'artist' THEN sorted.sort_artist
               WHEN 'media' THEN sorted.sort_media
               WHEN 'files' THEN sorted.file_count
               WHEN 'files_back' THEN sorted.sort_files_back
               WHEN 'size' THEN sorted.total_size
               WHEN 'size_back' THEN sorted.sort_size_back
               WHEN 'storage' THEN sorted.storage_order
               WHEN 'storage_back' THEN sorted.sort_storage_back
               WHEN 'added' THEN sorted.created_at
               ELSE 0 END AS lead,
           CASE :sort
               WHEN 'title_back' THEN sorted.sort_title_back
               WHEN 'artist_back' THEN sorted.sort_artist_back
               WHEN 'media_back' THEN sorted.sort_media_back
               WHEN 'added_back' THEN sorted.sort_added_back
               ELSE '' END COLLATE descending AS lead_back
    FROM release_listing AS sorted
"#
);
