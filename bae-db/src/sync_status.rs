//! The sync status as the status line and the activity list show it:
//! coven's sync status row, its pending records, what a retry would do and
//! why the last start failed, read in one go, and the actions each pending
//! record offers.

use crate::client::sync_status::CovenSyncRow;
use crate::pending_item::{PendingItem, PendingSubjectName, PendingWait};
use crate::PendingRow;
use bae_diagnostics::UiError;

/// Sync activity and the things coven is waiting on that need attention,
/// read from coven's tables and bae's own in one go.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncStatusSnapshot {
    /// coven's sync status row: whether storage is set up on this device,
    /// whether the running pass has moved a slot, and how the last pass
    /// ended.
    pub coven: CovenSyncRow,
    pub pending: Vec<PendingItem>,
    /// Why this device's last attempt to start sync failed, while that
    /// stands.
    pub error: Option<bae_diagnostics::UiError>,
    /// What the person's retry would do now, as coven decides it: run a
    /// pass, or be answered at once without one.
    pub retry_effect: coven::RetryEffect,
}

/// The settings status line. Platforms localize these cases and the item count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncIndicator {
    /// Storage is set up, but the sync loop isn't running.
    Stopped,
    Syncing,
    Synced,
    Offline,
    /// The failure is the snapshot's `shown_failure()`.
    Failed,
    Waiting {
        items: usize,
    },
}

/// The category a whole-pass sync failure shows under.
fn sync_failure_category(error: &coven::SyncFailure) -> bae_diagnostics::UiErrorCategory {
    use bae_diagnostics::UiErrorCategory as C;
    match error {
        coven::SyncFailure::UpdateRequired(_) => C::SyncUpdateRequired,
        coven::SyncFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
        coven::SyncFailure::Removed => C::DeviceRemoved,
        coven::SyncFailure::Storage(error) => bae_diagnostics::cloud_home_category(error),
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
        coven::StatusFailure::Removed => C::DeviceRemoved,
        coven::StatusFailure::Storage(failure) => C::Storage(*failure),
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

/// The sync status as one read of its tables sees it: coven's sync status
/// row (`coven_sync_status`), what a retry would do (`coven_retry_effect`),
/// its pending records (`coven_pending`), and why this device's last
/// attempt to start sync failed (`sync_start_failure`).
pub fn sync_status_on(sql: coven::SqlReadContext<'_>) -> coven::CovenResult<SyncStatusSnapshot> {
    let coven = crate::client::sync_status::coven_row_on(&sql)?;
    let pending = crate::client::pending::pending_rows_on(&sql)?;
    let start_failure = crate::client::sync_start_failure::start_failure_on(&sql)?;
    let retry_effect = coven::RetryEffect::read(sql)?;
    Ok(SyncStatusSnapshot::read(coven, retry_effect, pending, start_failure))
}

impl SyncStatusSnapshot {
    /// The sync status from one read of its tables: coven's sync status row
    /// and what a retry would do, its pending records each named by its
    /// row, and why the last attempt to start sync failed, while that
    /// stands.
    pub fn read(
        coven: CovenSyncRow,
        retry_effect: coven::RetryEffect,
        pending: Vec<PendingRow>,
        error: Option<UiError>,
    ) -> Self {
        let mut snapshot = Self {
            coven,
            pending: pending.into_iter().map(PendingItem::listed).collect(),
            error,
            retry_effect,
        };
        snapshot.name_connection_cause();
        snapshot
    }

    /// A pass is moving: it has read a slot it hadn't, or claimed one.
    pub fn syncing(&self) -> bool {
        self.coven.connected && self.coven.moving
    }

    /// Whether coven's sync loop runs: storage is set up on this device and
    /// sync isn't stopped, as what a retry would do says.
    pub fn running(&self) -> bool {
        self.coven.connected && self.retry_effect != coven::RetryEffect::NotSyncing
    }

    /// How the last pass ended, while that is what the status shows:
    /// storage is set up on this device and no pass is moving. A pass that
    /// moves shows as syncing until it ends, as coven's own sync state
    /// shows it.
    pub fn ended_pass(&self) -> Option<&coven::LastPass> {
        if !self.coven.connected || self.coven.moving {
            return None;
        }
        self.coven.last_pass.as_ref()
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
        match self.ended_pass()? {
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
        let Some(pass) = self.ended_pass() else {
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
        match self.ended_pass()? {
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
    /// A loop that runs retries a failed pass by itself, and Retry Now ends
    /// its wait; with no storage set up on this device there is no sync to
    /// start, and signing in to the recorded storage again is the way back.
    pub fn reconnect_offered(&self) -> bool {
        self.error.is_some() && self.coven.connected && !self.running()
    }

    /// Whether the activity list offers Retry Now: pressing it runs a pass,
    /// and a failure that pass can clear holds work back.
    ///
    /// Whether a pass runs is coven's answer (`retry_effect`), decided as it
    /// answers the retry: no pass runs while sync is stopped, while a
    /// failure only the person can clear holds every pass (new credentials,
    /// an update, a reset), or during a provider's cooldown. Whether there
    /// is anything for the pass to retry is the failure's own:
    /// `failure_a_retry_clears`.
    pub fn retry_offered(&self) -> bool {
        self.retry_effect == coven::RetryEffect::Pass && self.failure_a_retry_clears()
    }

    /// Whether a failure a retry can clear holds work back: the pass's own,
    /// which the next pass retries, or with no pass failing, a pending
    /// record's that coven retries by itself after a wait, such as an upload
    /// a server error refused. A blocked slot and coven's own failed
    /// operation retry as their records do. A failure only an update, a
    /// reset or nothing clears never does, nor a record waiting on other
    /// work, such as another device's upload.
    fn failure_a_retry_clears(&self) -> bool {
        use coven::{LastPass as S, StatusFailure as F};
        let retries =
            |record: &coven::PendingRecord| PendingWait::of(record, None).retries_by_itself();
        match self.ended_pass() {
            Some(S::Offline { .. } | S::Failed { failure: F::Storage(_) | F::Other, .. }) => true,
            Some(S::Blocked { slot, reason, .. }) => retries(&coven::PendingRecord::new(
                coven::PendingSubject::Slot(*slot),
                reason.clone(),
            )),
            Some(S::Failed { failure: F::Operation(record), .. }) => retries(record),
            Some(S::Failed {
                failure: F::UpdateRequired(_) | F::Removed | F::DeviceIdInUse(_),
                ..
            }) => false,
            Some(S::Synced) | None => self.pending.iter().any(|item| item.wait.retries_by_itself()),
        }
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
                self.ended_pass(),
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

    /// What the status line names, or `None` while coven reports no storage
    /// set up on this device: there is no sync to describe. A library with a
    /// recorded location in that state has lost this device's credentials
    /// for it, and signs in to it again.
    ///
    /// Pending records come first, since the list shows what each waits on.
    /// With none, the line names the status itself: a moving pass is
    /// syncing; otherwise how the last pass ended, as coven's sync state
    /// ranks a failure above activity, and a pass that ended synced (or
    /// none yet) is stopped while the loop isn't running. Its error, when it
    /// has one, is `failure()`.
    pub fn indicator(&self) -> Option<SyncIndicator> {
        if !self.coven.connected {
            return None;
        }
        let named = match self.ended_pass() {
            None if self.coven.moving => SyncIndicator::Syncing,
            Some(coven::LastPass::Offline { .. }) => SyncIndicator::Offline,
            Some(coven::LastPass::Failed { .. } | coven::LastPass::Blocked { .. }) => {
                SyncIndicator::Failed
            }
            Some(coven::LastPass::Synced) | None if !self.running() => SyncIndicator::Stopped,
            Some(coven::LastPass::Synced) | None => SyncIndicator::Synced,
        };
        if !self.pending.is_empty() {
            return Some(SyncIndicator::Waiting { items: self.pending.len() });
        }
        if self.error.is_some() {
            return Some(SyncIndicator::Failed);
        }
        Some(named)
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
    ResumeUploads,
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
        R::Removed => PendingAction::Review,
        R::Paused if matches!(record.subject, S::Upload { .. }) => PendingAction::ResumeUploads,
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
mod tests;
