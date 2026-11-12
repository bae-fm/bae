use super::*;

/// A busy database passes when asked again; a damaged one, or a refusal,
/// never does.
#[test]
fn only_a_busy_database_is_worth_retrying() {
    assert!(UiErrorCategory::DatabaseBusy.retry_can_help());
    assert!(!UiErrorCategory::DatabaseDamaged.retry_can_help());
    assert!(!UiErrorCategory::Database.retry_can_help());
}

/// A provider failure is worth retrying as coven retries it: the network
/// yes, refused credentials no.
#[test]
fn a_storage_failure_is_worth_retrying_as_coven_retries_it() {
    assert!(UiErrorCategory::Storage(coven::StorageFailure::Network).retry_can_help());
    assert!(!UiErrorCategory::Storage(coven::StorageFailure::Authentication).retry_can_help());
    // A refused sharing waits on the person who fixes their account.
    assert!(!UiErrorCategory::Storage(coven::StorageFailure::SharingRefused(
        coven::SharingRefusal::EmailUnverified
    ))
    .retry_can_help());
    assert!(!UiErrorCategory::StorageCheck {
        check: coven::StorageCheck::Create,
        failure: coven::StorageFailure::PermissionDenied,
    }
    .retry_can_help());
}

/// A grouped release waiting on a download passes once it lands; one whose
/// folder is gone waits on the person.
#[test]
fn a_grouping_block_is_worth_retrying_only_while_a_download_runs() {
    assert!(UiErrorCategory::GroupingBlocked(GroupingBlockReason::FolderFilesDownloading)
        .retry_can_help());
    assert!(!UiErrorCategory::GroupingBlocked(GroupingBlockReason::SourceGone).retry_can_help());
}
