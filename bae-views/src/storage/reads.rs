//! What the Storage Manager reads beside its windows, counts and positions:
//! its selection, the releases between two of them, every release it lists,
//! and the transfer queues holding work.

// The rows of the releases `selected` (a JSON array of ids) that the list
// under a filter and a search keeps, by release id, each beside what all of
// those rows allow, whether Cancel is the only action one of them offers, and
// whether it is the only one of them. Each is reached by its id.
sqlive_compose::read!(
    writer SelectedStorageRows(selected: String, stored: Option<i64>, search: Option<String>) {
        every_can_upload: bool,
        every_can_pin: bool,
        every_can_unpin: bool,
        every_can_move_to_this_device: bool,
        any_cancel_only: bool,
        sole: bool
    },
    r#"
    SELECT shown.*,
           MIN(shown.can_upload) OVER () AS every_can_upload,
           MIN(shown.can_pin) OVER () AS every_can_pin,
           MIN(shown.can_unpin) OVER () AS every_can_unpin,
           MIN(shown.can_move_to_this_device) OVER () AS every_can_move_to_this_device,
           MAX(shown.cancel_only) OVER () AS any_cancel_only,
           COUNT(*) OVER () = 1 AS sole
    FROM release_listing AS entry
    CROSS JOIN @crate::storage::row::StorageListRow AS shown
            ON shown.row_release_id = entry.release_id
    WHERE entry.release_id IN (SELECT picked.value FROM json_each(:selected) AS picked)
      AND (:stored IS NULL OR entry.stored = :stored)
      AND (:search IS NULL OR entry.album_id IN (
               SELECT found.album_id
               FROM @crate::album_search::AlbumsFound(pattern: :search) AS found))
    ORDER BY entry.release_id
"#
);

// The releases the list under a sort, a filter and a search shows from the
// release `from` to the release `to`, both included, in its order; the one it
// shows when it shows only one of the two, and none when it shows neither.
sqlive_compose::read!(
    writer StorageBetween(
        sort: String,
        stored: Option<i64>,
        search: Option<String>,
        from: String,
        to: String,
    ) { release_id: String },
    r#"
    WITH ranked AS MATERIALIZED (
             SELECT entry.release_id,
                    ROW_NUMBER() OVER (
                        ORDER BY entry.lead, entry.lead_back, entry.album_title_sort_key,
                                 entry.created_at, entry.release_id) AS number
             FROM @crate::storage::order_fragments::StorageOrder(sort: :sort) AS entry
             WHERE (:stored IS NULL OR entry.stored = :stored)
               AND (:search IS NULL OR entry.album_id IN (
                        SELECT found.album_id
                        FROM @crate::album_search::AlbumsFound(pattern: :search) AS found))),
         ends AS (
             SELECT MIN(ranked.number) AS low, MAX(ranked.number) AS high
             FROM ranked WHERE ranked.release_id IN (:from, :to))
    SELECT ranked.release_id
    FROM ranked
    JOIN ends ON ranked.number BETWEEN ends.low AND ends.high
    ORDER BY ranked.number
"#
);

// Every release the list under a filter and a search shows.
sqlive_compose::read!(
    writer StorageIds(stored: Option<i64>, search: Option<String>),
    r#"
    SELECT entry.release_id
    FROM release_listing AS entry
    WHERE (:stored IS NULL OR entry.stored = :stored)
      AND (:search IS NULL OR entry.album_id IN (
               SELECT found.album_id
               FROM @crate::album_search::AlbumsFound(pattern: :search) AS found))
"#
);

// Whether the upload queue holds a file and what holds that queue back, and
// what the download queue and the export and save queue are doing.
sqlive_compose::read!(
    writer StorageQueueWork { uploading: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM coven_upload_queue) AS uploading,
           held.*, downloads.*, outputs.*
    FROM @bae_records::records::queued_uploads::UploadQueueHeld AS held
    CROSS JOIN @bae_transfers::queue_counts::DownloadQueueWork AS downloads
    CROSS JOIN @bae_transfers::queue_counts::OutputQueueWork AS outputs
"#
);
