//! The releases of the download lines' batches, which the sync activity counts
//! and the Activity panel's bars measure.

// Each release of a download line's batch, by the line's kind: its size's
// bytes, whether the library still holds it, and whether the line's work on it
// is done. It is done once no entry of the kind for it is in the queue and its
// storage row says so: a pin once every file is pinned, a move to this device
// once the release is no longer stored in the cloud. A release the library no
// longer holds has no storage row, so it counts no bytes and is never done.
coven::fragment!(
    DownloadBatchRelease { bytes: i64, held: bool, done: bool },
    r#"
    SELECT member.*,
           COALESCE(storage.total_size, 0) AS bytes,
           storage.release_id IS NOT NULL AS held,
           COALESCE(CASE member.kind WHEN 'pin' THEN storage.pinned
                                     ELSE storage.stored = 0 END, 0)
               AND NOT EXISTS (
                   SELECT 1
                   FROM @bae_transfers::download_entry::QueuedDownloadEntry AS queued
                   WHERE queued.release_id = member.release_id
                       AND queued.kind = member.kind) AS done
    FROM @bae_transfers::download_batch::DownloadBatchMember AS member
    LEFT JOIN @bae_records::release_storage::ReleaseStorage AS storage
        ON storage.release_id = member.release_id
"#
);
