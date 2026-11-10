use super::*;
use crate::sync_activity::tests::{
    queue_upload, run_on_library_with_storage, write_releases_with_files, FILE_LATE,
};
use bae_records::test_fixtures::storage_listing::{RELEASE_EARLY, RELEASE_LATE};

/// The bytes as one read of `store` finds them.
async fn bytes_now(store: &bae_store::Store) -> ActivityBytes {
    store.read(|sql| activity_bytes_on(&sql)).await.unwrap()
}

fn waiting(bytes_total: u64, bytes_stored: u64) -> Option<UploadRelease> {
    Some(UploadRelease { bytes_total, bytes_stored, sending: BTreeMap::new() })
}

/// The Uploading line's batch reads each release's size and its stored
/// files' bytes, under the batch's number: a release whose file uploaded
/// counts all of its size, one whose original is missing none. Once the
/// batch empties there is nothing to read, and the next batch takes the next
/// number.
#[test]
fn the_upload_batch_reads_each_release_size_and_stored_bytes() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        assert_eq!(bytes_now(store).await.uploads, None, "nothing is queued to upload");

        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        let first = bytes_now(store).await.uploads.expect("two releases are queued");
        assert_eq!(
            first.releases,
            [(RELEASE_EARLY.to_string(), waiting(3, 0)), (RELEASE_LATE.to_string(), waiting(5, 0))]
                .into()
        );

        std::fs::remove_file(originals.join(FILE_LATE)).unwrap();
        store.set_uploads_paused(false).await.unwrap();
        store.retry_uploads_now().await.unwrap();
        store.set_uploads_paused(true).await.unwrap();
        assert_eq!(
            bytes_now(store).await.uploads,
            Some(BatchReleases {
                number: first.number,
                releases: [
                    (RELEASE_EARLY.to_string(), waiting(3, 3)),
                    (RELEASE_LATE.to_string(), waiting(5, 0)),
                ]
                .into(),
            })
        );

        records.cancel_release_uploads(vec![RELEASE_LATE.into()]).await.unwrap();
        assert_eq!(bytes_now(store).await.uploads, None, "no release has files queued");
        queue_upload(store, RELEASE_LATE).await;
        assert_eq!(
            bytes_now(store).await.uploads.map(|uploads| uploads.number),
            Some(first.number + 1),
            "the next batch takes the next number"
        );
    });
}

/// A release that leaves the library while its batch lasts is read as
/// gone, its size with it, while another release's files are queued.
#[test]
fn a_release_removed_from_the_library_is_read_as_gone() {
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;

        bae_records::test_fixtures::delete_release(&records, RELEASE_EARLY).await;
        assert_eq!(
            bytes_now(store).await.uploads.map(|uploads| uploads.releases),
            Some(
                [(RELEASE_EARLY.to_string(), None), (RELEASE_LATE.to_string(), waiting(5, 0))]
                    .into()
            )
        );
    });
}
