//! The releases the download queue has held of each kind since that kind last
//! had none waiting or downloading, which the sync activity counts and draws
//! a bar for. A read composes them as a fragment, so no other domain reads the
//! batch's table.

// Each release of each kind's batch.
sqlive_compose::fragment!(
    DownloadBatchMember,
    r#"
    SELECT member.release_id, member.kind
    FROM download_batch AS member
"#
);
