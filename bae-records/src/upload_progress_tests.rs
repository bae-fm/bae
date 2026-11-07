use super::*;

/// A release whose queued files all wait reads Queued; one with a file being
/// sent reads Uploading.
#[test]
fn a_release_upload_is_queued_until_a_file_is_sent() {
    let waiting = UploadProgress::of_release(&crate::DbReleaseUpload {
        waiting: 3,
        sending: 0,
        stored: 2,
        wait: None,
    });
    assert_eq!(waiting, UploadProgress { queued: 3, uploading: 0 });
    assert_eq!(waiting.activity(), UploadActivity::Queued);

    let sending = UploadProgress::of_release(&crate::DbReleaseUpload {
        waiting: 2,
        sending: 1,
        stored: 2,
        wait: None,
    });
    assert_eq!(sending, UploadProgress { queued: 2, uploading: 1 });
    assert_eq!(sending.activity(), UploadActivity::Uploading);
}

/// When several reasons hold the files, the network is named first, then the
/// pause, the metered network and the retry time.
#[test]
fn the_reason_named_is_the_first_that_holds() {
    use crate::UploadWait::*;
    let pick = crate::UploadWait::pick;
    assert_eq!(pick(Some(false), true, true, Some(5)), Some(Offline));
    assert_eq!(pick(Some(true), true, true, Some(5)), Some(Paused));
    assert_eq!(pick(None, false, true, Some(5)), Some(Metered));
    assert_eq!(pick(Some(true), false, false, Some(5)), Some(Retrying { at_ms: 5 }));
    assert_eq!(pick(None, false, false, None), None);
}
