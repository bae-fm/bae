//! The sync status as the status line and the activity list show it:
//! coven's sync state beside the library's read of its pending records,
//! what a retry would do and why the last start failed, and the actions
//! each pending record offers.

use crate::pending_item::{PendingItem, PendingSubjectName};
use bae_diagnostics::UiError;
use bae_store::{DbError, Store};
use coven::SqlReadContext;
use rusqlite::{params, OptionalExtension, Row};

mod file_row;
mod pending;
mod start_failure;
mod storage_setup;
pub use file_row::FileRow;
pub use pending::PendingRow;
pub use start_failure::StartFailureRecordError;
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

/// Sync activity and the things coven is waiting on that need attention:
/// coven's sync state, followed from coven, beside the rows the library
/// reads in one go ([`SyncStatusRows`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncStatusSnapshot {
    /// coven's sync state and when sync last finished, as coven computes
    /// them from what it knows (`subscribe_sync_status`).
    pub coven: coven::SyncStatus,
    pub pending: Vec<PendingItem>,
    /// Why this device's last attempt to start sync failed, while that
    /// stands.
    pub error: Option<bae_diagnostics::UiError>,
    /// What the person's retry would do now, as coven decides it: run a
    /// pass, or be answered at once without one.
    pub retry_effect: coven::RetryEffect,
    /// A storage setup running, or one that ended undone and that the
    /// person hasn't dismissed.
    pub storage_setup: Option<StorageSetupState>,
    /// The provider of the location recorded on this device, while one is.
    pub provider: Option<coven::CloudProvider>,
}

/// The category a whole-pass sync failure shows under.
fn sync_failure_category(error: &coven::SyncFailure) -> bae_diagnostics::UiErrorCategory {
    use bae_diagnostics::UiErrorCategory as C;
    match error {
        coven::SyncFailure::UpdateRequired(_) => C::SyncUpdateRequired,
        coven::SyncFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
        // A joined device whose admission the library refused shows as a
        // removed one (coven §13.2).
        coven::SyncFailure::Removed | coven::SyncFailure::NotAdmitted => C::DeviceRemoved,
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
        coven::StatusFailure::Removed | coven::StatusFailure::NotAdmitted => C::DeviceRemoved,
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
            blocked_failure(slot, reason, error.as_deref())
        }
    }
}

/// What a blocked slot shows the person: the error that blocked it, or with
/// none, as for a device that opened already blocked, its reason.
fn blocked_failure(
    slot: &coven::Slot,
    reason: &coven::PendingReason,
    error: Option<&str>,
) -> UiError {
    let category = bae_diagnostics::pending_reason_category(reason);
    match error {
        Some(error) => UiError::diagnostic(category, error),
        None => UiError::diagnostic(category, format!("slot {slot:?} cannot apply: {reason:?}")),
    }
}

/// What the sync status reads of the library's tables in one go, beside
/// coven's sync state: coven's pending records (`coven_pending`) each named
/// by its row, why this device's last attempt to start sync failed
/// (`sync_start_failure`), what a retry would do (`coven_retry_effect`), and
/// where this device's storage setup stands (`coven_operations`).
/// Two reads are equal only when every record's error behind it is too:
/// coven's records compare by subject and reason alone.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncStatusRows {
    pub pending: Vec<PendingItem>,
    pub start_failure: Option<UiError>,
    pub retry_effect: coven::RetryEffect,
    pub storage_setup: Option<StorageSetupState>,
    pub provider: Option<coven::CloudProvider>,
}

/// The sync status's rows as one read of the library's tables sees them.
pub fn sync_status_rows_on(
    sql: coven::SqlReadContext<'_>,
) -> Result<SyncStatusRows, bae_store::DbError> {
    let pending = pending::pending_rows_on(&sql)?.into_iter().map(PendingItem::listed).collect();
    let start_failure = start_failure::start_failure_on(&sql)?;
    let storage_setup = storage_setup::storage_setup_on(&sql)?;
    let provider = recorded_provider_on(&sql)?;
    let retry_effect = coven::RetryEffect::read(sql)?;
    Ok(SyncStatusRows { pending, start_failure, retry_effect, storage_setup, provider })
}

/// The provider of the location recorded on this device, as coven's sync
/// status row names it.
fn recorded_provider_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<Option<coven::CloudProvider>, DbError> {
    let name: Option<String> =
        sql.query_row("SELECT provider FROM coven_sync_status", [], |row| row.get(0))?;
    name.map(|name| match name.as_str() {
        "S3" => Ok(coven::CloudProvider::S3),
        "Dropbox" => Ok(coven::CloudProvider::Dropbox),
        "OneDrive" => Ok(coven::CloudProvider::OneDrive),
        "CloudKit" => Ok(coven::CloudProvider::CloudKit),
        _ => Err(DbError::Unreadable { what: "coven_sync_status.provider".into(), stored: name }),
    })
    .transpose()
}

impl SyncStatusSnapshot {
    /// The status of a library that isn't open, from coven's read of its
    /// closed store: sync doesn't run, and what an open library reads from
    /// its tables, the pending records, a failed start and a storage setup,
    /// is not read.
    pub fn closed(coven: coven::SyncStatus, provider: Option<coven::CloudProvider>) -> Self {
        Self {
            coven,
            pending: Vec::new(),
            error: None,
            retry_effect: coven::RetryEffect::NotSyncing,
            storage_setup: None,
            provider,
        }
    }

    /// The sync status from coven's sync state and the library's rows
    /// beside it: the pending records each named by its row, why the last
    /// attempt to start sync failed, while that stands, what a retry would
    /// do, and where this device's storage setup stands.
    pub fn read(coven: coven::SyncStatus, rows: SyncStatusRows) -> Self {
        let SyncStatusRows { pending, start_failure, retry_effect, storage_setup, provider } = rows;
        let mut snapshot =
            Self { coven, pending, error: start_failure, retry_effect, storage_setup, provider };
        snapshot.name_connection_cause();
        snapshot
    }

    /// Storage is set up on this device: a location is recorded and this
    /// device holds its credentials, as coven's sync state says.
    fn storage_set_up(&self) -> bool {
        !matches!(
            self.coven.state,
            coven::SyncState::NotSynced | coven::SyncState::SignedOut { cause: None, .. }
        )
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
            | S::Removed(cause)
            | S::NeedsReset(cause)
            | S::Failed(cause) => cause,
            S::NotSynced
            | S::Syncing(_)
            | S::WaitingToSync(_)
            | S::Synced
            | S::SignedOut { cause: None, .. } => return None,
        };
        match cause {
            coven::SyncCause::Pass(pass) => Some(pass),
            coven::SyncCause::Pending(_) => None,
        }
    }

    /// Give the connection's record the error behind it. coven keeps the
    /// error behind every other record beside it, but a connection's failure
    /// goes to the last pass, which keeps its text until the next pass
    /// replaces it.
    fn name_connection_cause(&mut self) {
        let Some((failure, error)) = self.connection_error() else {
            return;
        };
        let (failure, error) = (failure, error.to_string());
        let connection =
            self.pending.iter_mut().filter(|item| item.subject == PendingSubjectName::Connection);
        for item in connection {
            if matches!(&item.wait.reason, coven::PendingReason::Storage { failure: recorded, .. } if *recorded == failure)
            {
                item.wait.cause = Some(error.clone());
            }
        }
    }

    /// The storage failure a pass failed on, with its text, when it failed
    /// on the connection: storage refused it, or couldn't be reached.
    fn connection_error(&self) -> Option<(coven::StorageFailure, &str)> {
        match self.shown_pass()? {
            coven::LastPass::Offline { failure, error }
            | coven::LastPass::Failed { failure: coven::StatusFailure::Storage(failure), error } => {
                Some((*failure, error))
            }
            coven::LastPass::Synced
            | coven::LastPass::Blocked { .. }
            | coven::LastPass::Failed { .. } => None,
        }
    }

    /// Whether a pending record shows the last pass's failure. coven
    /// records a failed pass's condition beside it: the connection's when
    /// storage refused the pass or couldn't be reached, a blocked slot's, and
    /// that of its own operation whose last attempt failed.
    fn failure_recorded(&self) -> bool {
        use coven::{LastPass as S, PendingReason as R, StatusFailure as F};
        use PendingSubjectName as N;
        let Some(pass) = self.shown_pass() else {
            return false;
        };
        self.pending.iter().any(|item| match (pass, &item.subject, &item.wait.reason) {
            (
                S::Offline { failure, .. } | S::Failed { failure: F::Storage(failure), .. },
                N::Connection,
                R::Storage { failure: recorded, .. },
            ) => recorded == failure,
            (S::Blocked { reason, .. }, N::LibraryChanges, recorded) => recorded == reason,
            (S::Failed { failure: F::Operation(record), .. }, N::Operation(kind), recorded) => {
                let failed = match record.subject {
                    coven::PendingSubject::Operation { kind, .. } => Some(kind),
                    _ => None,
                };
                failed == Some(*kind) && *recorded == record.reason
            }
            _ => false,
        })
    }

    /// The failure the status line and the activity list show beside the
    /// pending records: the last pass's, unless one of them shows it
    /// already.
    /// That record names what failed and what the person can do; the same
    /// failure again on a line of its own reads as a second problem.
    pub fn shown_failure(&self) -> Option<UiError> {
        if self.error.is_none() && self.failure_recorded() {
            return None;
        }
        self.failure()
    }

    pub fn failure(&self) -> Option<UiError> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        use bae_diagnostics::UiErrorCategory as C;
        match self.shown_pass()? {
            coven::LastPass::Offline { error, .. } => Some(UiError::diagnostic(C::Network, error)),
            coven::LastPass::Failed { failure, error } => {
                Some(UiError::diagnostic(status_failure_category(failure), error))
            }
            // A device that opened already blocked has only the slot's
            // reason, which its pending record shows.
            coven::LastPass::Blocked { slot, reason, error: Some(error) } => {
                Some(blocked_failure(slot, reason, Some(error)))
            }
            coven::LastPass::Blocked { error: None, .. } | coven::LastPass::Synced => None,
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
    /// blocked a slot and each failed attempt of its own operations, once per
    /// change, so logging them here would repeat its line.
    fn failure_coven_does_not_log(&self) -> Option<UiError> {
        let logged_by_coven = self.error.is_none()
            && matches!(
                self.shown_pass(),
                Some(
                    coven::LastPass::Blocked { .. }
                        | coven::LastPass::Failed {
                            failure: coven::StatusFailure::Operation(_),
                            ..
                        }
                )
            );
        if logged_by_coven {
            return None;
        }
        self.failure()
    }
}

/// The action a pending record offers. The record itself retains the subject
/// and typed reason, including both failures when an undo cannot finish.
///
/// None of them resets the device: a reset is offered only when opening finds
/// a damaged database, or when sync finds another copy of this device, which
/// coven reports as the last pass's failure rather than as a record. A removed
/// device offers nothing that would register it again; its record is only
/// reviewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    Wait,
    UpdateApp,
    SignIn(coven::CloudProvider),
    Reconnect,
    RestoreFile,
    RetryOperation(coven::OperationId),
    /// Pin the release again through the download queue: what a failed
    /// pin's download waits for.
    RetryDownload {
        release_id: String,
    },
    /// End a release's failed move to the cloud, `id`, so its files stay on
    /// this device only: what a move whose original changed waits for.
    CancelUpload {
        id: coven::MoveId,
    },
    /// Another of the person's devices replaced their keys: this copy can't
    /// send, and restoring it with the person's current restore code
    /// registers this device again under those keys.
    RestoreFromCode,
    Review,
}

/// What the person can do about `record`. `release_id` is the release whose
/// file the record is about, when it is about one: a failed pin's download
/// retries by pinning the release again, and a failed move of a release's
/// files ends by cancelling the move.
pub fn pending_action(record: &coven::PendingRecord, release_id: Option<&str>) -> PendingAction {
    use coven::{PendingReason as R, PendingSubject as S};
    let mut reason = &record.reason;
    while let R::Cleanup { cleanup, .. } = reason {
        reason = cleanup;
    }
    match reason {
        R::NoStorage => PendingAction::Reconnect,
        R::Storage {
            provider,
            failure:
                coven::StorageFailure::Authentication | coven::StorageFailure::AccessNotGranted { .. },
            ..
        } => PendingAction::SignIn(*provider),
        R::FileUnavailable(coven::FileMissingReason::Source(_))
        | R::Original { failure: coven::FileSourceFailure::Missing, .. } => {
            PendingAction::RestoreFile
        }
        R::FileUnavailable(coven::FileMissingReason::Lost) => PendingAction::Review,
        R::UpdateRequired(_) | R::Refused(_) => PendingAction::UpdateApp,
        R::MemberKeysReplaced { .. } => PendingAction::RestoreFromCode,
        R::Removed | R::NotAdmitted => PendingAction::Review,
        _ => match record.retry() {
            coven::Retry::Automatic => PendingAction::Wait,
            coven::Retry::AfterUpdate => PendingAction::UpdateApp,
            coven::Retry::AppAction => match (&record.subject, release_id) {
                (S::Operation { id, .. }, _) => PendingAction::RetryOperation(*id),
                (S::Download { .. }, Some(release_id)) => {
                    PendingAction::RetryDownload { release_id: release_id.to_string() }
                }
                (S::Move { id }, Some(_)) => PendingAction::CancelUpload { id: *id },
                _ => PendingAction::Review,
            },
            coven::Retry::Never => PendingAction::Review,
        },
    }
}

#[cfg(test)]
#[path = "sync_status_tests.rs"]
pub(crate) mod tests;
