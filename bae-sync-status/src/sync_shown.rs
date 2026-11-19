//! What the Sync page's top block shows for a sync state: the state in words,
//! the color it reads in, its cause and its fix, and when sync last
//! finished. coven gives the state, and ranks it when several hold; this
//! picks the words and the fix.

use crate::sync_status::{StorageSetupState, SyncStatusSnapshot};
use bae_diagnostics::UiError;
use coven::{
    CloudProvider, LocalFailure, PendingReason, ResetReason, SyncCause, SyncState, SyncWait,
};
use std::time::SystemTime;

/// The color a state reads in: gray for what is ordinary, green for synced,
/// amber for what needs a look soon, red for what stops sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTone {
    Neutral,
    Success,
    Warning,
    Danger,
}

/// The state, named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatusWord {
    /// No storage is set up on this device.
    NotSynced,
    /// A storage setup runs, its call's or one that sync's connection
    /// finishes.
    SettingUpStorage,
    Syncing,
    WaitingToSync,
    Synced,
    OfflineWithUnsentChanges,
    Offline,
    UpdateNeeded,
    /// Signed out of the provider; an S3 library has no sign-in, so its key
    /// was not accepted.
    SignedOut(CloudProvider),
    DeviceNeedsReset,
    /// Another of the person's devices made a new restore code, which
    /// replaced the keys this device holds: sync stays stopped until the
    /// person scans the new code.
    NewRestoreCode,
    NoAccess,
    FolderMissing,
    ClockWrong,
    RemovedFromTheLibrary,
    StorageFull,
    SyncFailed,
}

/// Why a state is what it is, in words.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncCauseLine {
    UploadFailedWillRetry,
    /// The provider asked every request to wait until `until`.
    ProviderWait {
        provider: CloudProvider,
        until: SystemTime,
    },
    UpdateBae,
    AskForAccess,
    CheckTheClock,
    OldCopy,
    FreeStorageSpace,
    /// The changes this device made stay on it: storage refuses the access
    /// it signs in with until the person signs in again.
    ChangesStayOnDevice,
    SignInToICloud,
    /// A file this device uploads is missing or changed since import; it
    /// stays in the way until it is back as it was.
    FileMissingOrChanged,
    /// The disk on this device is full.
    DeviceDiskFull,
    /// The operating system refused bae a file it needs.
    DeviceDiskDenied,
    /// The disk on this device failed for any other reason.
    DeviceDiskFailed,
    DeviceKeysUnreadable,
    LibraryUnreadable,
    /// bae's own failure on this device that sync retries by itself, such as
    /// coven's copy of an image bae made failing its check.
    BaeProblemOnDevice,
    /// Another of the person's devices made a new restore code: entering it
    /// restores the library on this device as a new device, and the restore
    /// discards what this device has not sent, since coven refuses every
    /// write signed with the keys the new code replaced.
    EnterNewRestoreCode,
    /// The provider answered with an error, told as the provider told it.
    ProviderError {
        provider: CloudProvider,
        detail: String,
    },
    ChangeDamaged,
    SomethingWentWrong,
    /// Any other pending condition, in the words of its pending record.
    Pending(PendingReason),
    /// A failure told as its category line: why the last start failed, or
    /// how the last pass failed.
    Failure(UiError),
}

/// What the person can do about the state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFix {
    RetryNow,
    /// Start sync again after a start that failed.
    Reconnect,
    CheckForUpdates,
    /// Dropbox and OneDrive sign in from bae.
    SignIn(CloudProvider),
    EnterAccessKey,
    /// iCloud signs in from the system's settings.
    OpenSystemSettings,
    ResetDevice,
    /// Open the restore screen for the person's new restore code, after
    /// another of their devices made it.
    EnterRestoreCode,
    RemoveFromThisDevice,
    OpenInBrowser,
}

/// The top block of the Sync page.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncShown {
    pub status: SyncStatusWord,
    pub tone: SyncTone,
    pub cause: Option<SyncCauseLine>,
    pub fix: Option<SyncFix>,
    /// When sync last finished, under a state that is a problem (amber or
    /// red) and never under one that isn't.
    pub last_synced: Option<SystemTime>,
}

impl SyncShown {
    fn new(status: SyncStatusWord, tone: SyncTone) -> Self {
        Self { status, tone, cause: None, fix: None, last_synced: None }
    }

    fn because(mut self, cause: SyncCauseLine) -> Self {
        self.cause = Some(cause);
        self
    }

    fn fixed_by(mut self, fix: SyncFix) -> Self {
        self.fix = Some(fix);
        self
    }
}

impl SyncStatusSnapshot {
    /// The state to show: coven's, in words, with its cause and fix. While a
    /// storage setup runs it names the setup in place of any state that is
    /// no problem. A start that failed, with sync not running, is a failure
    /// with that start's error as its cause and Reconnect as its fix.
    pub fn shown(&self) -> SyncShown {
        let mut shown = self.shown_state();
        if shown.tone == SyncTone::Warning || shown.tone == SyncTone::Danger {
            shown.last_synced = self.coven.last_synced;
        }
        shown
    }

    fn shown_state(&self) -> SyncShown {
        use SyncStatusWord as W;
        use SyncTone as T;
        let setting_up = self.storage_setup == Some(StorageSetupState::Running);
        let state = &self.coven.state;
        if setting_up
            && matches!(
                state,
                SyncState::NotSynced
                    | SyncState::SignedOut { cause: None, .. }
                    | SyncState::Syncing(_)
                    | SyncState::Synced
                    | SyncState::WaitingToSync(_)
            )
        {
            return SyncShown::new(W::SettingUpStorage, T::Neutral);
        }
        if let Some(error) = &self.error {
            if !matches!(state, SyncState::NotSynced | SyncState::SignedOut { cause: None, .. }) {
                return self.start_failed(error.clone());
            }
        }
        match state {
            SyncState::NotSynced => SyncShown::new(W::NotSynced, T::Neutral),
            SyncState::Syncing(_) => SyncShown::new(W::Syncing, T::Neutral),
            SyncState::Synced => SyncShown::new(W::Synced, T::Success),
            SyncState::WaitingToSync(wait) => self.waiting(wait),
            SyncState::Offline(_) => SyncShown::new(W::Offline, T::Warning),
            SyncState::OfflineWithUnsentChanges(_) => {
                SyncShown::new(W::OfflineWithUnsentChanges, T::Warning)
            }
            SyncState::SignedOut { provider, .. } => self.signed_out(*provider),
            SyncState::NoAccess(_) => {
                SyncShown::new(W::NoAccess, T::Danger).because(SyncCauseLine::AskForAccess)
            }
            SyncState::StorageFull(_) => {
                let shown = SyncShown::new(W::StorageFull, T::Danger)
                    .because(SyncCauseLine::FreeStorageSpace);
                if self.has_folder_to_open() {
                    shown.fixed_by(SyncFix::OpenInBrowser)
                } else {
                    shown
                }
            }
            SyncState::FolderMissing(_) => SyncShown::new(W::FolderMissing, T::Danger),
            SyncState::ClockWrong(_) => {
                SyncShown::new(W::ClockWrong, T::Danger).because(SyncCauseLine::CheckTheClock)
            }
            SyncState::UpdateNeeded(_) => SyncShown::new(W::UpdateNeeded, T::Warning)
                .because(SyncCauseLine::UpdateBae)
                .fixed_by(SyncFix::CheckForUpdates),
            SyncState::Removed { .. } => SyncShown::new(W::RemovedFromTheLibrary, T::Danger)
                .because(SyncCauseLine::OldCopy)
                .fixed_by(SyncFix::RemoveFromThisDevice),
            SyncState::NeedsReset { reason: ResetReason::MemberKeysReplaced { .. }, .. } => {
                SyncShown::new(W::NewRestoreCode, T::Warning)
                    .because(SyncCauseLine::EnterNewRestoreCode)
                    .fixed_by(SyncFix::EnterRestoreCode)
            }
            SyncState::NeedsReset {
                reason: ResetReason::Replaced | ResetReason::DeviceIdInUse,
                ..
            } => SyncShown::new(W::DeviceNeedsReset, T::Danger).fixed_by(SyncFix::ResetDevice),
            SyncState::Failed(cause) => SyncShown::new(W::SyncFailed, T::Danger)
                .because(self.failure_cause(cause))
                .fixed_by(SyncFix::RetryNow),
        }
    }

    /// A start that failed with storage set up on this device: sync is not
    /// running, and starting it again is what retries it.
    fn start_failed(&self, error: UiError) -> SyncShown {
        let shown = SyncShown::new(SyncStatusWord::SyncFailed, SyncTone::Danger)
            .because(SyncCauseLine::Failure(error));
        if self.reconnect_offered() {
            shown.fixed_by(SyncFix::Reconnect)
        } else {
            shown.fixed_by(SyncFix::RetryNow)
        }
    }

    fn waiting(&self, wait: &SyncWait) -> SyncShown {
        use SyncStatusWord::WaitingToSync;
        let neutral = SyncShown::new(WaitingToSync, SyncTone::Neutral);
        match wait {
            SyncWait::Retrying { .. } => {
                neutral.because(SyncCauseLine::UploadFailedWillRetry).fixed_by(SyncFix::RetryNow)
            }
            SyncWait::CoolingDown { until } => {
                let amber = SyncShown::new(WaitingToSync, SyncTone::Warning);
                match self.provider() {
                    Some(provider) => {
                        amber.because(SyncCauseLine::ProviderWait { provider, until: *until })
                    }
                    None => amber,
                }
            }
            SyncWait::NotSyncing | SyncWait::KeyCopy { .. } | SyncWait::Snapshot { .. } => neutral,
        }
    }

    fn signed_out(&self, provider: CloudProvider) -> SyncShown {
        use SyncStatusWord::SignedOut;
        let amber = SyncShown::new(SignedOut(provider), SyncTone::Warning);
        match provider {
            CloudProvider::S3 => {
                amber.because(SyncCauseLine::ChangesStayOnDevice).fixed_by(SyncFix::EnterAccessKey)
            }
            CloudProvider::Dropbox | CloudProvider::OneDrive => {
                amber.fixed_by(SyncFix::SignIn(provider))
            }
            CloudProvider::CloudKit => {
                amber.because(SyncCauseLine::SignInToICloud).fixed_by(SyncFix::OpenSystemSettings)
            }
        }
    }

    /// The words for a failure no state of its own names: what the last pass
    /// or the first pending record that names it says.
    fn failure_cause(&self, cause: &SyncCause) -> SyncCauseLine {
        use coven::{LastPass, StatusFailure};
        let reason = match cause {
            SyncCause::Pending(record) => {
                return self.reason_cause(&record.reason, record.cause.as_deref())
            }
            SyncCause::Pass(LastPass::Blocked { reason, error, .. }) => {
                return self.reason_cause(reason, error.as_deref())
            }
            SyncCause::Pass(LastPass::Failed { failure, error }) => match failure {
                StatusFailure::Operation(record) => {
                    return self.reason_cause(&record.reason, Some(error))
                }
                StatusFailure::Storage(failure) => {
                    return self.storage_cause(*failure, error);
                }
                StatusFailure::UpdateRequired(_)
                | StatusFailure::Removed(_)
                | StatusFailure::DeviceIdInUse(_)
                | StatusFailure::Other => None,
            },
            SyncCause::Pass(LastPass::Offline { .. } | LastPass::Synced) => None,
        };
        match (reason, self.failure()) {
            (None, Some(error)) => SyncCauseLine::Failure(error),
            (None, None) => SyncCauseLine::SomethingWentWrong,
            (Some(never), _) => never,
        }
    }

    /// A provider failure no state names, as the provider told it.
    fn storage_cause(&self, failure: coven::StorageFailure, error: &str) -> SyncCauseLine {
        match self.provider() {
            Some(provider) => SyncCauseLine::ProviderError { provider, detail: error.to_string() },
            None => SyncCauseLine::Failure(UiError::diagnostic(
                bae_diagnostics::UiErrorCategory::Storage(failure),
                error,
            )),
        }
    }

    /// The words for a pending condition that names a failure.
    fn reason_cause(&self, reason: &PendingReason, detail: Option<&str>) -> SyncCauseLine {
        use coven::FileMissingReason as M;
        match reason {
            PendingReason::Original { .. } => SyncCauseLine::FileMissingOrChanged,
            // coven's own copy of a file bae made failed its check; the person
            // has no file to put back, and coven retries it by itself.
            PendingReason::FileUnavailable(M::Source(_)) => SyncCauseLine::BaeProblemOnDevice,
            PendingReason::Refused(_) => SyncCauseLine::ChangeDamaged,
            PendingReason::Failed(LocalFailure::DiskFull) => SyncCauseLine::DeviceDiskFull,
            PendingReason::Failed(LocalFailure::DiskDenied) => SyncCauseLine::DeviceDiskDenied,
            PendingReason::Failed(LocalFailure::DiskOther) => SyncCauseLine::DeviceDiskFailed,
            PendingReason::Failed(LocalFailure::KeyCustody) => SyncCauseLine::DeviceKeysUnreadable,
            PendingReason::Failed(LocalFailure::Database) => SyncCauseLine::LibraryUnreadable,
            PendingReason::Failed(
                LocalFailure::Crypto
                | LocalFailure::Operation
                | LocalFailure::Bug
                | LocalFailure::Shutdown,
            ) => SyncCauseLine::SomethingWentWrong,
            PendingReason::Storage { provider, failure: _, answered: _ } => {
                SyncCauseLine::ProviderError {
                    provider: *provider,
                    detail: detail.unwrap_or_default().to_string(),
                }
            }
            PendingReason::Cleanup { cleanup, .. } => self.reason_cause(cleanup, detail),
            other => SyncCauseLine::Pending(other.clone()),
        }
    }
}

#[cfg(test)]
#[path = "sync_shown_tests.rs"]
mod tests;
