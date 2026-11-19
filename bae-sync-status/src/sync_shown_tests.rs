use crate::sync_shown::{SyncCauseLine, SyncFix, SyncShown, SyncStatusWord, SyncTone};
use crate::sync_status::tests::*;
use crate::sync_status::{StorageSetupState, SyncStatusSnapshot};
use bae_diagnostics::UiError;

/// A moment sync last finished, for the tests of when it shows.
fn finished() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000)
}

/// A folder label with a page to open.
fn folder_label() -> coven::FolderLabel {
    coven::FolderLabel {
        name: "Music".into(),
        web_url: url::Url::parse("https://example.com/folder").unwrap(),
    }
}

/// A location on `provider`, a Dropbox or OneDrive one with a folder label.
pub(crate) fn location_on(provider: coven::CloudProvider) -> coven::StorageConfig {
    match provider {
        coven::CloudProvider::S3 => coven::StorageConfig::S3 {
            bucket: "bucket".into(),
            region: "region".into(),
            endpoint: None,
            profile: None,
        },
        coven::CloudProvider::Dropbox => coven::StorageConfig::Dropbox {
            location: coven::DropboxLocation::SharedFolder { namespace_id: "1".into() },
            account_label: None,
            folder_label: Some(folder_label()),
        },
        coven::CloudProvider::OneDrive => coven::StorageConfig::OneDrive {
            drive_id: "drive".into(),
            folder_id: "folder".into(),
            account_label: None,
            folder_label: Some(folder_label()),
        },
        coven::CloudProvider::CloudKit => coven::StorageConfig::CloudKit {
            container: "container".into(),
            owner: "owner".into(),
            zone: "zone".into(),
        },
    }
}

/// The top block for `state` in a library on `provider`, sync having last
/// finished at [`finished`], with no failed start.
fn shown_on(provider: coven::CloudProvider, state: coven::SyncState) -> SyncShown {
    shown_on_location(location_on(provider), state)
}

/// The top block for `state` in a library at `location`, as [`shown_on`].
fn shown_on_location(location: coven::StorageConfig, state: coven::SyncState) -> SyncShown {
    SyncStatusSnapshot {
        coven: coven::SyncStatus { state, last_synced: Some(finished()) },
        error: None,
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: None,
        location: Some(location),
    }
    .shown()
}

/// A state coven names by the pending record `record`.
fn named_by(record: coven::PendingRecord, named: Names) -> coven::SyncState {
    named(coven::SyncCause::Pending(record))
}

/// Each state shows its words, its color, its cause and its fix, as the
/// Sync page's boards draw them.
#[test]
fn every_state_shows_its_words_color_cause_and_fix() {
    use coven::CloudProvider as P;
    use SyncCauseLine as C;
    use SyncFix as F;
    use SyncStatusWord as W;
    use SyncTone as T;
    let dropbox = |state| shown_on(P::Dropbox, state);
    let check = |shown: SyncShown, status, tone, cause: Option<SyncCauseLine>, fix| {
        assert_eq!((shown.status, shown.tone, shown.cause, shown.fix), (status, tone, cause, fix));
    };

    check(dropbox(disconnected()), W::NotSynced, T::Neutral, None, None);
    check(dropbox(moving()), W::Syncing, T::Neutral, None, None);
    check(dropbox(synced()), W::Synced, T::Success, None, None);
    let waits = |wait| dropbox(coven::SyncState::WaitingToSync(wait));
    check(waits(coven::SyncWait::NotSyncing), W::WaitingToSync, T::Neutral, None, None);
    let snapshot = coven::SyncWait::Snapshot { audience: coven::Audience::Store };
    check(waits(snapshot), W::WaitingToSync, T::Neutral, None, None);
    check(
        waits(coven::SyncWait::Retrying { at: finished() }),
        W::WaitingToSync,
        T::Neutral,
        Some(C::UploadFailedWillRetry),
        Some(F::RetryNow),
    );
    check(
        waits(coven::SyncWait::CoolingDown { until: finished() }),
        W::WaitingToSync,
        T::Warning,
        Some(C::ProviderWait { provider: P::Dropbox, until: finished() }),
        None,
    );
    check(dropbox(offline()), W::Offline, T::Warning, None, None);
    let unsent = ended(
        coven::LastPass::Offline { failure: coven::StorageFailure::Network, error: "".into() },
        coven::SyncState::OfflineWithUnsentChanges,
    );
    check(dropbox(unsent), W::OfflineWithUnsentChanges, T::Warning, None, None);

    let update = failed_status(
        coven::StatusFailure::UpdateRequired(coven::RequiredUpdate::AppSchema { version: 9 }),
        coven::SyncState::UpdateNeeded,
    );
    check(
        dropbox(update),
        W::UpdateNeeded,
        T::Warning,
        Some(C::UpdateBae),
        Some(F::CheckForUpdates),
    );
    let signed_out =
        |provider| shown_on(provider, coven::SyncState::SignedOut { provider, cause: None });
    check(
        signed_out(P::Dropbox),
        W::SignedOut(P::Dropbox),
        T::Warning,
        None,
        Some(F::SignIn(P::Dropbox)),
    );
    check(
        signed_out(P::OneDrive),
        W::SignedOut(P::OneDrive),
        T::Warning,
        None,
        Some(F::SignIn(P::OneDrive)),
    );
    check(
        signed_out(P::S3),
        W::SignedOut(P::S3),
        T::Warning,
        Some(C::ChangesStayOnDevice),
        Some(F::EnterAccessKey),
    );
    check(
        signed_out(P::CloudKit),
        W::SignedOut(P::CloudKit),
        T::Warning,
        Some(C::SignInToICloud),
        Some(F::OpenSystemSettings),
    );

    let by_pass = |failure, named: Names| dropbox(failed_status(failure, named));
    check(dropbox(needs_reset_state()), W::DeviceNeedsReset, T::Danger, None, Some(F::ResetDevice));
    let keys_replaced = coven::SyncState::NeedsReset {
        reason: coven::ResetReason::MemberKeysReplaced {
            member: coven::MemberId(uuid::Uuid::from_u128(7)),
        },
        unsent: coven::UnsentWork { writes: 1, files: 0 },
    };
    check(
        dropbox(keys_replaced),
        W::NewRestoreCode,
        T::Warning,
        Some(C::EnterNewRestoreCode),
        Some(F::EnterRestoreCode),
    );
    check(
        by_pass(
            coven::StatusFailure::Storage(coven::StorageFailure::PermissionDenied),
            coven::SyncState::NoAccess,
        ),
        W::NoAccess,
        T::Danger,
        Some(C::AskForAccess),
        None,
    );
    check(
        by_pass(
            coven::StatusFailure::Storage(coven::StorageFailure::ContainerNotFound),
            coven::SyncState::FolderMissing,
        ),
        W::FolderMissing,
        T::Danger,
        None,
        None,
    );
    check(
        by_pass(
            coven::StatusFailure::Storage(coven::StorageFailure::ClockSkew),
            coven::SyncState::ClockWrong,
        ),
        W::ClockWrong,
        T::Danger,
        Some(C::CheckTheClock),
        None,
    );
    check(
        dropbox(removed_state()),
        W::RemovedFromTheLibrary,
        T::Danger,
        Some(C::OldCopy),
        Some(F::RemoveFromThisDevice),
    );
    let full = failed_status(
        coven::StatusFailure::Storage(coven::StorageFailure::QuotaExceeded),
        coven::SyncState::StorageFull,
    );
    check(
        dropbox(full.clone()),
        W::StorageFull,
        T::Danger,
        Some(C::FreeStorageSpace),
        Some(F::OpenInBrowser),
    );
    check(
        shown_on(P::S3, full.clone()),
        W::StorageFull,
        T::Danger,
        Some(C::FreeStorageSpace),
        None,
    );
    // Dropbox's App folder has no folder label, so no page to open.
    let app_folder = |state| {
        shown_on_location(
            coven::StorageConfig::Dropbox {
                location: coven::DropboxLocation::AppFolder,
                account_label: None,
                folder_label: None,
            },
            state,
        )
    };
    check(app_folder(full), W::StorageFull, T::Danger, Some(C::FreeStorageSpace), None);
}

/// Sync failed always offers Retry now, and says why in the words of its
/// cause: a file that can't be uploaded is named, and a provider's error is
/// told as the provider told it.
#[test]
fn sync_failed_says_why_and_offers_retry_now() {
    use coven::CloudProvider as P;
    let failed_by = |provider, reason| {
        shown_on(
            provider,
            named_by(
                coven::PendingRecord::new(
                    coven::PendingSubject::Upload {
                        device: coven::DeviceId(1),
                        object: coven::ObjectId(uuid::Uuid::from_u128(5)),
                    },
                    reason,
                ),
                coven::SyncState::Failed,
            ),
        )
    };
    let cause = |reason| {
        let shown = failed_by(P::Dropbox, reason);
        assert_eq!(
            (shown.status, shown.tone, shown.fix),
            (SyncStatusWord::SyncFailed, SyncTone::Danger, Some(SyncFix::RetryNow))
        );
        shown.cause.expect("a failure says why")
    };
    assert_eq!(
        cause(coven::PendingReason::FileUnavailable(coven::FileMissingReason::Source(
            coven::FileSourceFailure::Missing
        ))),
        SyncCauseLine::BaeProblemOnDevice
    );
    assert_eq!(
        cause(coven::PendingReason::Original {
            path: "/music/a.flac".into(),
            failure: coven::FileSourceFailure::Missing,
        }),
        SyncCauseLine::FileMissingOrChanged
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::DiskFull)),
        SyncCauseLine::DeviceDiskFull
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::DiskDenied)),
        SyncCauseLine::DeviceDiskDenied
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::DiskOther)),
        SyncCauseLine::DeviceDiskFailed
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::KeyCustody)),
        SyncCauseLine::DeviceKeysUnreadable
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::Database)),
        SyncCauseLine::LibraryUnreadable
    );
    assert_eq!(
        cause(coven::PendingReason::Failed(coven::LocalFailure::Bug)),
        SyncCauseLine::SomethingWentWrong
    );
    assert_eq!(
        cause(coven::PendingReason::Refused(coven::Refusal::ContentHash)),
        SyncCauseLine::ChangeDamaged
    );
    assert_eq!(
        cause(coven::PendingReason::Storage {
            provider: P::Dropbox,
            failure: coven::StorageFailure::Protocol,
            answered: true,
        }),
        SyncCauseLine::ProviderError { provider: P::Dropbox, detail: String::new() }
    );
}

/// When sync last finished shows under a problem state, amber or red, and
/// under no other: never under Synced.
#[test]
fn last_synced_shows_under_a_problem_state_only() {
    let at = |state| shown_on(coven::CloudProvider::Dropbox, state).last_synced;
    assert_eq!(at(offline()), Some(finished()));
    assert_eq!(
        at(coven::SyncState::SignedOut { provider: coven::CloudProvider::Dropbox, cause: None }),
        Some(finished())
    );
    assert_eq!(at(removed_state()), Some(finished()));
    assert_eq!(
        at(coven::SyncState::WaitingToSync(coven::SyncWait::CoolingDown { until: finished() })),
        Some(finished())
    );
    assert_eq!(at(synced()), None);
    assert_eq!(at(moving()), None);
    assert_eq!(at(disconnected()), None);
    assert_eq!(at(coven::SyncState::WaitingToSync(coven::SyncWait::NotSyncing)), None);
}

/// A start that failed with storage set up shows as a failure with the
/// start's error as its cause, and Reconnect while sync isn't running.
#[test]
fn a_failed_start_shows_why_and_offers_reconnect() {
    let error = UiError::internal("the store key is missing");
    let status = |effect| read(synced(), effect, Some(error.clone())).shown();
    let stopped = status(coven::RetryEffect::NotSyncing);
    assert_eq!(stopped.status, SyncStatusWord::SyncFailed);
    assert_eq!(stopped.tone, SyncTone::Danger);
    assert_eq!(stopped.cause, Some(SyncCauseLine::Failure(error.clone())));
    assert_eq!(stopped.fix, Some(SyncFix::Reconnect));
    assert_eq!(status(coven::RetryEffect::Pass).fix, Some(SyncFix::RetryNow));
}

/// While a setup runs, the state names it in place of every state that is
/// no problem, and a problem still shows.
#[test]
fn a_running_setup_names_the_state_until_a_problem_does() {
    let setting_up = |state| {
        let mut status = alone(state);
        status.storage_setup = Some(StorageSetupState::Running);
        status.shown().status
    };
    let setup = SyncStatusWord::SettingUpStorage;
    assert_eq!(setting_up(disconnected()), setup);
    assert_eq!(setting_up(moving()), setup);
    assert_eq!(setting_up(synced()), setup);
    assert_eq!(setting_up(offline()), SyncStatusWord::Offline);
    assert_eq!(setting_up(removed_state()), SyncStatusWord::RemovedFromTheLibrary);
}
