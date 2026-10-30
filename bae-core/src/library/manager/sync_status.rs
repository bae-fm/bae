//! Sync status and the actions for coven's current pending records.

use super::{SyncIndicator, SyncStatusSnapshot};
use crate::db::Database;
use crate::ui::UiError;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct SyncStatusState {
    state: Arc<Mutex<SyncStatusSnapshot>>,
    values: tokio::sync::watch::Sender<SyncStatusSnapshot>,
    connected: tokio::sync::watch::Sender<bool>,
}

impl SyncStatusState {
    pub(super) fn new(database: &Database) -> Self {
        let status = database.subscribe_sync_status().borrow().clone();
        let retry_effect = *database.subscribe_retry_effect().borrow();
        let state = SyncStatusSnapshot {
            status,
            pending: None,
            abandoned: None,
            error: None,
            retry_effect,
        };
        let (values, _) = tokio::sync::watch::channel(state.clone());
        let (connected, _) = tokio::sync::watch::channel(state.connected());
        Self {
            state: Arc::new(Mutex::new(state)),
            values,
            connected,
        }
    }

    /// Every change goes through here, so the connection's record always
    /// carries the error the status holds, and a failure coven doesn't log
    /// itself is logged once, as it appears or changes, rather than on every
    /// value published.
    fn apply(&self, change: impl FnOnce(&mut SyncStatusSnapshot)) {
        let mut state = self.state.lock().unwrap();
        let failure = state.failure_coven_does_not_log();
        change(&mut state);
        state.name_connection_cause();
        let changed = state.failure_coven_does_not_log();
        if changed != failure {
            if let Some(UiError::Diagnostic { category, detail }) = &changed {
                tracing::warn!(?category, "sync failure: {detail}");
            }
        }
        self.connected.send_replace(state.connected());
        self.values.send_replace(state.clone());
    }

    /// The sync status and what a retry would do beside it, applied
    /// together so no value shows one without the other.
    pub(super) fn set_sync(&self, status: coven::SyncStatus, retry_effect: coven::RetryEffect) {
        self.apply(|state| {
            state.status = status;
            state.retry_effect = retry_effect;
        });
    }

    #[cfg(test)]
    pub(super) fn set_status(&self, status: coven::SyncStatus) {
        self.apply(|state| state.status = status);
    }

    /// The pending work that needs attention.
    pub(super) fn set_pending(&self, pending: Result<Vec<super::PendingItem>, UiError>) {
        self.apply(|state| state.pending = Some(pending));
    }

    /// This device's abandoned attachments, named.
    pub(super) fn set_abandoned(&self, abandoned: Result<Vec<super::AbandonedItem>, UiError>) {
        self.apply(|state| state.abandoned = Some(abandoned));
    }

    pub(super) fn set_error(&self, error: Option<UiError>) {
        self.apply(|state| state.error = error);
    }

    pub(super) fn subscribe_cloud_home(&self) -> tokio::sync::watch::Receiver<bool> {
        self.connected.subscribe()
    }

    pub(super) fn snapshot(&self) -> SyncStatusSnapshot {
        self.state.lock().unwrap().clone()
    }
    pub(super) fn subscribe(&self) -> tokio::sync::watch::Receiver<SyncStatusSnapshot> {
        self.values.subscribe()
    }
}

/// The category a whole-pass sync failure shows under.
fn sync_failure_category(error: &coven::SyncFailure) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    match error {
        coven::SyncFailure::UpdateRequired(_) => C::SyncUpdateRequired,
        coven::SyncFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
        coven::SyncFailure::Removed => C::DeviceRemoved,
        coven::SyncFailure::Storage(error) => super::error_category::cloud_home_category(error),
        coven::SyncFailure::Operation(_) | coven::SyncFailure::Other(_) => C::Internal,
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
/// would show it. A slot blocked since before this process opened has no
/// error of its own, so its reason is the detail.
fn pass_failure(failure: &coven::PassFailure) -> UiError {
    use crate::ui::UiErrorCategory as C;
    match failure {
        coven::PassFailure::Offline { error } => UiError::diagnostic(C::Network, error),
        coven::PassFailure::Failed { error } => {
            UiError::diagnostic(sync_failure_category(error), error)
        }
        coven::PassFailure::Blocked {
            slot,
            reason,
            error,
        } => {
            let category = super::error_category::pending_reason_category(reason);
            match error {
                Some(error) => UiError::diagnostic(category, error),
                None => {
                    UiError::diagnostic(category, format!("slot {slot:?} cannot apply: {reason:?}"))
                }
            }
        }
    }
}

impl SyncStatusSnapshot {
    pub fn connected(&self) -> bool {
        !matches!(self.status, coven::SyncStatus::Disconnected)
    }

    /// The sync loop is running: storage is set up and syncing isn't
    /// stopped, whether a pass is in progress, finished, or failed and backing
    /// off.
    pub(super) fn loop_running(&self) -> bool {
        !matches!(
            self.status,
            coven::SyncStatus::Disconnected | coven::SyncStatus::Stopped
        )
    }

    /// A sync pass is in progress (or the first one is queued).
    pub fn syncing(&self) -> bool {
        matches!(self.status, coven::SyncStatus::Syncing)
    }

    /// Give the connection's record the error behind it. coven holds the
    /// error behind every other record, but a connection's failure goes to
    /// the status this process published, which holds it, with its cause,
    /// until the next pass replaces it. An error rebuilt from the record after
    /// a restart has no cause, and names nothing the record doesn't.
    fn name_connection_cause(&mut self) {
        use std::error::Error as _;
        let error = self
            .connection_error()
            .filter(|error| error.source().is_some())
            .cloned();
        let Some(Ok(items)) = &mut self.pending else {
            return;
        };
        let connection = items
            .iter_mut()
            .filter(|item| item.subject == super::PendingSubjectName::Connection);
        for item in connection {
            item.wait.cause = match (&item.wait.reason, &error) {
                (coven::PendingReason::Storage { failure, .. }, Some(error))
                    if error.failure() == *failure =>
                {
                    Some(error.clone() as Arc<dyn std::error::Error + Send + Sync>)
                }
                _ => None,
            };
        }
    }

    /// The storage error a pass failed on when it failed on the connection:
    /// storage refused it, or couldn't be reached.
    fn connection_error(&self) -> Option<&Arc<coven::StorageError>> {
        match &self.status {
            coven::SyncStatus::Offline { error }
            | coven::SyncStatus::Failed {
                error: coven::SyncFailure::Storage(error),
            } => Some(error),
            _ => None,
        }
    }

    /// Whether a pending record shows the status's failure. coven records
    /// a failed pass's condition beside the status: the connection's when
    /// storage refused the pass or couldn't be reached, a blocked slot's, and
    /// that of its own operation whose last attempt failed.
    fn failure_recorded(&self) -> bool {
        use super::PendingSubjectName as N;
        use coven::{PendingReason as R, SyncFailure as F, SyncStatus as S};
        let Some(Ok(items)) = &self.pending else {
            return false;
        };
        items.iter().any(
            |item| match (&self.status, &item.subject, &item.wait.reason) {
                (
                    S::Offline { error }
                    | S::Failed {
                        error: F::Storage(error),
                    },
                    N::Connection,
                    R::Storage { failure, .. },
                ) => error.failure() == *failure,
                (S::Blocked { reason, .. }, N::LibraryChanges, recorded) => recorded == reason,
                (
                    S::Failed {
                        error: F::Operation(record),
                    },
                    N::Operation(kind),
                    recorded,
                ) => {
                    let failed = match record.subject {
                        coven::PendingSubject::Operation { kind, .. } => Some(kind),
                        _ => None,
                    };
                    failed == Some(*kind) && *recorded == record.reason
                }
                _ => false,
            },
        )
    }

    /// The failure the status line and the activity list show beside the
    /// pending records: the status's, unless one of them shows it already.
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
        use crate::ui::UiErrorCategory as C;
        match &self.status {
            coven::SyncStatus::Offline { error } => Some(UiError::diagnostic(C::Network, error)),
            coven::SyncStatus::Failed { error } => {
                Some(UiError::diagnostic(sync_failure_category(error), error))
            }
            // A device that opened already blocked has only the slot's
            // reason, which its pending record shows.
            coven::SyncStatus::Blocked {
                reason,
                error: Some(error),
                ..
            } => Some(UiError::diagnostic(
                super::error_category::pending_reason_category(reason),
                error,
            )),
            coven::SyncStatus::Blocked { error: None, .. }
            | coven::SyncStatus::Disconnected
            | coven::SyncStatus::Stopped
            | coven::SyncStatus::Syncing
            | coven::SyncStatus::Synced { .. } => None,
        }
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
        use coven::{SyncFailure as F, SyncStatus as S};
        let retries =
            |record: &coven::PendingRecord| super::PendingWait::of(record).retries_by_itself();
        match &self.status {
            S::Offline { .. }
            | S::Failed {
                error: F::Storage(_) | F::Other(_),
            } => true,
            S::Blocked { slot, reason, .. } => retries(&coven::PendingRecord::new(
                coven::PendingSubject::Slot(*slot),
                reason.clone(),
            )),
            S::Failed {
                error: F::Operation(record),
            } => retries(record),
            S::Failed {
                error: F::UpdateRequired(_) | F::Removed | F::DeviceIdInUse(_),
            } => false,
            S::Disconnected | S::Stopped | S::Syncing | S::Synced { .. } => matches!(
                &self.pending,
                Some(Ok(items)) if items.iter().any(|item| item.wait.retries_by_itself())
            ),
        }
    }

    /// The failure, unless coven logs it itself: coven logs the error that
    /// blocked a slot and each failed attempt of its own operations, once per
    /// change, so logging them here would repeat its line.
    fn failure_coven_does_not_log(&self) -> Option<UiError> {
        let logged_by_coven = self.error.is_none()
            && matches!(
                self.status,
                coven::SyncStatus::Blocked { .. }
                    | coven::SyncStatus::Failed {
                        error: coven::SyncFailure::Operation(_)
                    }
            );
        if logged_by_coven {
            return None;
        }
        self.failure()
    }

    /// Pending records come first, since the list shows what each waits on.
    /// With none, the line names the status itself; its error, when it has
    /// one, is `failure()`.
    pub fn indicator(&self) -> SyncIndicator {
        match &self.pending {
            _ if !self.connected() => SyncIndicator::Unconnected,
            None => SyncIndicator::Loading,
            Some(Err(_)) => SyncIndicator::Unreadable,
            Some(Ok(pending)) if !pending.is_empty() => SyncIndicator::Waiting {
                items: pending.len(),
            },
            Some(Ok(_)) if self.error.is_some() => SyncIndicator::Failed,
            Some(Ok(_)) => match self.status {
                coven::SyncStatus::Disconnected => SyncIndicator::Unconnected,
                coven::SyncStatus::Stopped => SyncIndicator::Stopped,
                coven::SyncStatus::Syncing => SyncIndicator::Syncing,
                coven::SyncStatus::Synced { .. } => SyncIndicator::Synced,
                coven::SyncStatus::Offline { .. } => SyncIndicator::Offline,
                coven::SyncStatus::Failed { .. } | coven::SyncStatus::Blocked { .. } => {
                    SyncIndicator::Failed
                }
            },
        }
    }
}

/// The action a pending record offers. The record itself retains the subject
/// and typed reason, including both failures when an undo cannot finish.
///
/// None of them resets the device: a reset is offered only when opening finds
/// a damaged database, or when sync finds another copy of this device, which
/// coven reports as the status's failure rather than as a record. A removed
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
    /// Another of the person's devices replaced their keys: this copy can't
    /// send, and restoring it with the person's current restore code
    /// registers this device again under those keys.
    RestoreFromCode,
    Review,
}

pub fn pending_action(record: &coven::PendingRecord) -> PendingAction {
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
        R::FileUnavailable(coven::FileMissingReason::Source(_)) => PendingAction::RestoreFile,
        R::UpdateRequired(_) | R::Refused(_) => PendingAction::UpdateApp,
        R::MemberKeysReplaced { .. } => PendingAction::RestoreFromCode,
        R::Removed => PendingAction::Review,
        R::Paused if matches!(record.subject, S::File { .. }) => PendingAction::ResumeUploads,
        _ => match record.retry() {
            coven::Retry::Automatic => PendingAction::Wait,
            coven::Retry::AfterUpdate => PendingAction::UpdateApp,
            coven::Retry::AppAction => match record.subject {
                S::Operation { id, .. } => PendingAction::RetryOperation(id),
                _ => PendingAction::Review,
            },
            coven::Retry::Never => PendingAction::Review,
        },
    }
}

#[cfg(test)]
#[path = "sync_status_tests.rs"]
mod tests;
