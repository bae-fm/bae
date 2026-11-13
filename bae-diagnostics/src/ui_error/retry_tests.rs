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

/// An AirPlay receiver that is busy pairing, asks to wait, or couldn't be
/// reached passes when asked again; one that is locked, full or not
/// accepting pairings, or whose pairing failed, answers the same way, and one
/// that plays only from some people never changes who that is.
#[test]
fn an_airplay_receiver_is_worth_retrying_only_when_the_cause_ends_by_itself() {
    use UiErrorCategory as C;
    for category in [C::AirPlayBusy, C::AirPlayBackOff { seconds: 30 }, C::AirPlayUnreachable] {
        assert!(category.retry_can_help(), "{category:?}");
    }
    for category in [
        C::AirPlayPairingFailed,
        C::AirPlayLockedOut,
        C::AirPlayNotPairing,
        C::AirPlayFull,
        C::AirPlayHomeOnly,
        C::AirPlayAdminsOnly,
        C::AirPlayOwnerOnly,
    ] {
        assert!(!category.retry_can_help(), "{category:?}");
    }
}
