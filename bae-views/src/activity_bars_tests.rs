use super::*;
use crate::sync_activity::tests::{
    queue_upload, run_on_library_with_storage, write_releases_with_files, FILE_LATE,
};
use bae_records::test_fixtures::storage_listing::{RELEASE_EARLY, RELEASE_LATE};

/// The bar of a batch with nothing done and nothing transferring.
fn empty_bar<Key>() -> BatchBar<Key> {
    BatchBar { done: 0.0, moving: Vec::new() }
}

/// The bars as one read of `store` finds them.
async fn bars_now(store: &bae_store::Store) -> ActivityBarRows {
    store.read(|sql| activity_bars_on(&sql)).await.unwrap()
}

/// Send what the upload queue can, with `FILE_LATE`'s original gone, so bae
/// stores only the early release's file.
async fn upload_all_but_the_late_file(
    store: &bae_store::Store,
    originals: &bae_util::test_files::TestFolder,
) {
    std::fs::remove_file(originals.join(FILE_LATE)).unwrap();
    store.set_uploads_paused(false).await.unwrap();
    store.retry_uploads_now().await.unwrap();
    store.set_uploads_paused(true).await.unwrap();
}

/// The Uploading line's bar is the share of its releases' bytes that bae has
/// stored: none while every file waits, the early release's 3 bytes of 8
/// once bae stores its file, and an empty bar once the batch empties.
#[test]
fn the_upload_bar_is_the_share_of_its_batch_stored() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        assert_eq!(bars_now(store).await.uploads, empty_bar(), "the upload queue is empty");

        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        assert_eq!(
            bars_now(store).await.uploads,
            BatchBar { done: 0.0, moving: Vec::new() },
            "bae has stored nothing and is sending nothing"
        );

        upload_all_but_the_late_file(store, originals).await;
        assert_eq!(bars_now(store).await.uploads, BatchBar { done: 3.0 / 8.0, moving: Vec::new() });

        records.cancel_release_uploads(vec![RELEASE_LATE.into()]).await.unwrap();
        assert_eq!(bars_now(store).await.uploads, empty_bar(), "the batch is over");
    });
}

/// The upload bar counts none of the bytes of a release that bae deletes
/// from the library during its batch, while another release's files wait in
/// the queue.
#[test]
fn a_release_removed_from_the_library_counts_none_of_its_bytes() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        upload_all_but_the_late_file(store, originals).await;

        bae_records::test_fixtures::delete_release(&records, RELEASE_EARLY).await;
        assert_eq!(bars_now(store).await.uploads, BatchBar { done: 0.0, moving: Vec::new() });
    });
}
