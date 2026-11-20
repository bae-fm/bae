//! The sync status as the status line and the activity list show it:
//! coven's sync state, what a retry would do
//! and why the last start failed, and the action each pending record offers
//! where another surface shows the record.

use bae_diagnostics::UiError;
use bae_store::{DbError, Store};
use coven::SqlReadContext;
use rusqlite::{params, OptionalExtension, Row};

mod copy_reset_notice;
mod start_failure;
mod status_query;
mod storage_setup;
pub use copy_reset_notice::CopyResetNotice;
pub use start_failure::StartFailureRecordError;
pub use status_query::sync_status_on;
pub use storage_setup::{SetupDismissError, StorageSetupState};

/// Sync status's part of the schema: `sync_status.sql`.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("sync_status.sql")]);

/// The library's sync status rows over the store.
#[derive(Clone)]
pub struct SyncStatus {
    /// The store every read and write of the sync status runs through.
    store: Store,
}

impl std::fmt::Debug for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncStatus").finish_non_exhaustive()
    }
}

impl SyncStatus {
    pub fn new(store: Store) -> Self {
        SyncStatus { store }
    }
}

/// Sync activity and what stops it: coven's sync state beside what the
/// library holds, read in one statement ([`sync_status_on`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncStatusSnapshot {
    /// coven's sync state and when sync last finished, as coven computes
    /// them from what it knows (`coven_sync_state`).
    pub coven: coven::SyncStatus,
    /// Why this device's last attempt to start sync failed, while that
    /// stands.
    pub error: Option<bae_diagnostics::UiError>,
    /// What the person's retry would do now, as coven decides it: run a
    /// pass, or be answered at once without one.
    pub retry_effect: coven::RetryEffect,
    /// A storage setup running, or one that ended undone and that the
    /// person hasn't dismissed.
    pub storage_setup: Option<StorageSetupState>,
    /// The location recorded on this device, with its account and folder
    /// labels, while one is.
    pub location: Option<coven::StorageConfig>,
}

/// Why this device isn't in the library, as the failure's detail says it.
fn removal_text(removal: coven::Removal) -> &'static str {
    match removal {
        coven::Removal::Device => "an admin removed this device from the library",
        coven::Removal::Member => {
            "an admin removed this device's person from the library, or the person left it"
        }
        coven::Removal::NeverAdmitted => "the library refused this device's admission",
    }
}

/// Why this installation can't send, as the failure's detail says it.
fn reset_text(reason: coven::ResetReason) -> &'static str {
    match reason {
        coven::ResetReason::Replaced => {
            "a reset or restore registered another installation in this one's place"
        }
        coven::ResetReason::DeviceIdInUse => {
            "another installation uses this device's id, and this copy lacks the other \
             installation's newer changes"
        }
        coven::ResetReason::MemberKeysReplaced { .. } => {
            "another of the person's devices replaced their keys, and this device does not \
             hold the new ones"
        }
    }
}

/// The category a whole-pass sync failure shows under.
fn sync_failure_category(error: &coven::SyncFailure) -> bae_diagnostics::UiErrorCategory {
    use bae_diagnostics::UiErrorCategory as C;
    match error {
        coven::SyncFailure::UpdateRequired(_) => C::SyncUpdateRequired,
        coven::SyncFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
        // An admin removed this device or its person from the library, or no
        // admin admitted the device (coven §13.2).
        coven::SyncFailure::Removed(_) => C::DeviceRemoved,
        coven::SyncFailure::Storage(error) => bae_diagnostics::storage_error_category(error),
        coven::SyncFailure::Operation(_) | coven::SyncFailure::Other(_) => C::Internal,
    }
}

/// The category a whole-pass failure the last pass keeps shows under, as
/// [`sync_failure_category`] classes the failure it was.
fn status_failure_category(failure: &coven::StatusFailure) -> bae_diagnostics::UiErrorCategory {
    use bae_diagnostics::UiErrorCategory as C;
    match failure {
        coven::StatusFailure::UpdateRequired(_) => C::SyncUpdateRequired,
        coven::StatusFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
        coven::StatusFailure::Removed(_) => C::DeviceRemoved,
        coven::StatusFailure::Storage(failure) => bae_diagnostics::storage_category(*failure),
        coven::StatusFailure::Operation(_) | coven::StatusFailure::Other => C::Internal,
    }
}

/// What the person's retry came to: the first sync pass after the request,
/// as coven's `retry_now` answers it.
#[derive(Debug, Clone)]
pub enum SyncRetryOutcome {
    /// The pass reached its read end.
    Synced,
    /// The pass failed, or a failure only the person can clear (new
    /// credentials, an update, a reset) holds every pass.
    Failed(UiError),
    /// The storage provider asked every request to wait until then; the pass
    /// runs once it ends.
    CoolingDown { until: std::time::SystemTime },
    /// Sync isn't running, so no pass followed the request.
    NotSyncing,
}

impl From<coven::RetryOutcome> for SyncRetryOutcome {
    fn from(outcome: coven::RetryOutcome) -> Self {
        match outcome {
            coven::RetryOutcome::Synced { .. } => Self::Synced,
            coven::RetryOutcome::Failed(failure) => Self::Failed(pass_failure(&failure)),
            coven::RetryOutcome::CoolingDown { until } => Self::CoolingDown { until },
            coven::RetryOutcome::NotSyncing => Self::NotSyncing,
        }
    }
}

/// What a failed pass shows the person, under the category the status
/// line would show it. A slot blocked since before this process opened has no
/// error of its own, so its reason is the detail.
fn pass_failure(failure: &coven::PassFailure) -> UiError {
    use bae_diagnostics::UiErrorCategory as C;
    match failure {
        coven::PassFailure::Offline { error } => UiError::diagnostic(C::Network, error),
        coven::PassFailure::Failed { error } => {
            UiError::diagnostic(sync_failure_category(error), error)
        }
        coven::PassFailure::Blocked { slot, reason, error } => {
            waiting_failure(&coven::PendingSubject::Slot(*slot), reason, error.as_deref())
        }
    }
}

/// What a condition coven holds work on shows the person: a blocked slot's,
/// or a pending record's that names the sync state. It shows the error coven
/// keeps for it, or with none, as for a device that opened already blocked or
/// after the library reopens, the subject that waits and its reason.
fn waiting_failure(
    subject: &coven::PendingSubject,
    reason: &coven::PendingReason,
    error: Option<&str>,
) -> UiError {
    let category = bae_diagnostics::pending_reason_category(reason);
    match error {
        Some(error) => UiError::diagnostic(category, error),
        None => UiError::diagnostic(category, format!("{subject:?} waits: {reason:?}")),
    }
}

impl SyncStatusSnapshot {
    /// The status of a library that isn't open, from coven's read of its
    /// closed store: sync doesn't run, and what an open library reads from
    /// its tables, a failed start and a storage setup, is not read.
    pub fn closed(coven: coven::SyncStatus, location: Option<coven::StorageConfig>) -> Self {
        Self {
            coven,
            error: None,
            retry_effect: coven::RetryEffect::NotSyncing,
            storage_setup: None,
            location,
        }
    }

    /// The provider of the location recorded on this device, while one is.
    pub fn provider(&self) -> Option<coven::CloudProvider> {
        self.location.as_ref().map(coven::StorageConfig::provider)
    }

    /// Whether the recorded location has a folder the person can open in a
    /// browser: a Dropbox shared folder or a OneDrive folder whose label the
    /// latest connection read.
    pub fn has_folder_to_open(&self) -> bool {
        matches!(
            self.location,
            Some(
                coven::StorageConfig::Dropbox { folder_label: Some(_), .. }
                    | coven::StorageConfig::OneDrive { folder_label: Some(_), .. }
            )
        )
    }

    /// Storage is set up on this device: a location is recorded and this
    /// device holds its credentials, as coven's sync state says.
    fn storage_set_up(&self) -> bool {
        !matches!(
            self.coven.state,
            coven::SyncState::NotSynced | coven::SyncState::SignedOut { cause: None, .. }
        )
    }

    /// The library has a cloud home: this device has a storage location
    /// recorded, or a setup of one runs.
    pub fn has_cloud_home(&self) -> bool {
        self.location.is_some() || self.storage_setup == Some(StorageSetupState::Running)
    }

    /// A pass is moving, or a file is being sent or downloaded, as coven's
    /// sync state says.
    pub fn syncing(&self) -> bool {
        matches!(self.coven.state, coven::SyncState::Syncing(_))
    }

    /// Whether coven's sync loop runs: storage is set up on this device and
    /// sync isn't stopped, as what a retry would do says.
    pub fn running(&self) -> bool {
        self.storage_set_up() && self.retry_effect != coven::RetryEffect::NotSyncing
    }

    /// How the last pass ended, while it is what names coven's sync state:
    /// offline, or a failure the pass met. A synced or moving state, or one
    /// a pending record names, has no pass behind it.
    pub fn shown_pass(&self) -> Option<&coven::LastPass> {
        match self.shown_cause()? {
            coven::SyncCause::Pass(pass) => Some(pass),
            coven::SyncCause::Pending(_) => None,
        }
    }

    /// What names coven's sync state, while a failure names it: how the last
    /// pass ended, or a pending record. A synced or moving state has none.
    fn shown_cause(&self) -> Option<&coven::SyncCause> {
        use coven::SyncState as S;
        let cause = match &self.coven.state {
            S::Offline(cause)
            | S::OfflineWithUnsentChanges(cause)
            | S::SignedOut { cause: Some(cause), .. }
            | S::NoAccess(cause)
            | S::StorageFull(cause)
            | S::FolderMissing(cause)
            | S::ClockWrong(cause)
            | S::UpdateNeeded(cause)
            | S::Failed(cause) => cause,
            S::NotSynced
            | S::Removed { .. }
            | S::NeedsReset { .. }
            | S::Syncing(_)
            | S::WaitingToSync(_)
            | S::Synced
            | S::SignedOut { cause: None, .. } => return None,
        };
        Some(cause)
    }

    /// The failure the Activity panel lists as sync's: why this device's last
    /// start failed, a removed device or one that needs a reset, or what
    /// names a failed state, the last pass or a pending record.
    pub fn failure(&self) -> Option<UiError> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        use bae_diagnostics::UiErrorCategory as C;
        match &self.coven.state {
            coven::SyncState::Removed { removal, .. } => {
                return Some(UiError::diagnostic(C::DeviceRemoved, removal_text(*removal)))
            }
            coven::SyncState::NeedsReset { reason, .. } => {
                return Some(UiError::diagnostic(C::DeviceResetRequired, reset_text(*reason)))
            }
            _ => {}
        }
        let pass = match self.shown_cause()? {
            coven::SyncCause::Pass(pass) => pass,
            coven::SyncCause::Pending(record) => {
                return Some(waiting_failure(
                    &record.subject,
                    &record.reason,
                    record.cause.as_deref(),
                ))
            }
        };
        match pass {
            coven::LastPass::Offline { error, .. } => Some(UiError::diagnostic(C::Network, error)),
            coven::LastPass::Failed { failure, error } => {
                Some(UiError::diagnostic(status_failure_category(failure), error))
            }
            coven::LastPass::Blocked { slot, reason, error } => {
                Some(waiting_failure(&coven::PendingSubject::Slot(*slot), reason, error.as_deref()))
            }
            coven::LastPass::Synced => None,
        }
    }

    /// Whether the status line offers Reconnect: a start failed and left
    /// sync not running with storage set up on this device, and starting it
    /// again is what retries it.
    /// A loop that runs retries a failed pass by itself, and Retry now ends
    /// its wait; with no storage set up on this device there is no sync to
    /// start, and signing in to the recorded storage again is the way back.
    pub fn reconnect_offered(&self) -> bool {
        self.error.is_some() && self.storage_set_up() && !self.running()
    }

    /// The failure to log as this status follows `last`: one coven doesn't
    /// log itself, as it appears or changes. The same failure read again, or
    /// a change to anything else, logs nothing more.
    pub fn failure_to_log(&self, last: Option<&Self>) -> Option<UiError> {
        let failure = self.failure_coven_does_not_log();
        if last.and_then(Self::failure_coven_does_not_log) == failure {
            return None;
        }
        failure
    }

    /// The failure, unless coven logs it itself: coven logs the error that
    /// blocked a slot, each failed attempt of its own operations and each
    /// pending record's error, once per change, so logging them here would
    /// repeat its line.
    fn failure_coven_does_not_log(&self) -> Option<UiError> {
        let logged_by_coven = self.error.is_none()
            && matches!(
                self.shown_cause(),
                Some(
                    coven::SyncCause::Pending(_)
                        | coven::SyncCause::Pass(
                            coven::LastPass::Blocked { .. }
                                | coven::LastPass::Failed {
                                    failure: coven::StatusFailure::Operation(_),
                                    ..
                                }
                        )
                )
            );
        if logged_by_coven {
            return None;
        }
        self.failure()
    }
}

#[cfg(test)]
#[path = "sync_status_tests.rs"]
pub(crate) mod tests;

/// What this copy will not keep when it resets, when coven's sync `state`
/// says another installation uses this device's id and this copy lacks the
/// other installation's newer changes: core then resets the copy at once,
/// without asking. The work is the edits and files this copy never sent.
pub fn copy_behind(state: &coven::SyncState) -> Option<coven::UnsentWork> {
    match *state {
        coven::SyncState::NeedsReset { reason: coven::ResetReason::DeviceIdInUse, unsent } => {
            Some(unsent)
        }
        _ => None,
    }
}
