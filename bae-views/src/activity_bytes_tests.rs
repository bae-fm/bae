use super::*;
use crate::sync_activity::tests::{
    queue_upload, run_on_library_with_storage, write_releases_with_files, FILE_LATE,
};
use bae_records::test_fixtures::storage_listing::{RELEASE_EARLY, RELEASE_LATE};

/// The bytes as one read of `store` finds them.
async fn bytes_now(store: &bae_store::Store) -> ActivityBytes {
    store.read(|sql| activity_bytes_on(&sql)).await.unwrap()
}

/// The Uploading line's batch reads the size of its releases and the bytes of
/// their stored files: a release whose file uploaded counts all of its size,
/// one whose original is missing none. Once the batch empties there is nothing
/// to read.
#[test]
fn the_upload_batch_reads_its_size_and_stored_bytes() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        assert_eq!(bytes_now(store).await.uploads, None, "nothing is queued to upload");

        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        let first = bytes_now(store).await.uploads.expect("two releases are queued");
        assert_eq!((first.bytes_total, first.bytes_done), (8, 0));
        assert!(first.sending.is_empty(), "nothing is being sent");

        std::fs::remove_file(originals.join(FILE_LATE)).unwrap();
        store.set_uploads_paused(false).await.unwrap();
        store.retry_uploads_now().await.unwrap();
        store.set_uploads_paused(true).await.unwrap();
        let second = bytes_now(store).await.uploads.expect("a release still has files queued");
        assert_eq!((second.bytes_total, second.bytes_done), (8, 3));

        records.cancel_release_uploads(vec![RELEASE_LATE.into()]).await.unwrap();
        assert_eq!(bytes_now(store).await.uploads, None, "no release has files queued");
    });
}

/// A release that leaves the library while its batch lasts counts none of its
/// bytes, while another release's files are queued.
#[test]
fn a_release_removed_from_the_library_counts_none_of_its_bytes() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;

        bae_records::test_fixtures::delete_release(&records, RELEASE_EARLY).await;
        let uploads = bytes_now(store).await.uploads.expect("a release has files queued");
        assert_eq!((uploads.bytes_total, uploads.bytes_done), (5, 0));
    });
}
