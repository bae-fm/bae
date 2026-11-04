use super::*;

/// A release whose queued files all wait reads Queued; one with a file being
/// sent reads Uploading.
#[test]
fn a_release_upload_is_queued_until_a_file_is_sent() {
    let waiting =
        UploadProgress::of_release(&crate::DbReleaseUpload { waiting: 3, sending: 0, stored: 2 });
    assert_eq!(waiting, UploadProgress { queued: 3, uploading: 0 });
    assert_eq!(waiting.activity(), UploadActivity::Queued);

    let sending =
        UploadProgress::of_release(&crate::DbReleaseUpload { waiting: 2, sending: 1, stored: 2 });
    assert_eq!(sending, UploadProgress { queued: 2, uploading: 1 });
    assert_eq!(sending.activity(), UploadActivity::Uploading);
}
