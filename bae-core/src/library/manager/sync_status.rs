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
        let state = SyncStatusSnapshot {
            status,
            pending: None,
            abandoned: None,
            error: None,
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
        let error = match &self.status {
            coven::SyncStatus::Offline { error }
            | coven::SyncStatus::Failed {
                error: coven::SyncFailure::Storage(error),
            } if error.source().is_some() => Some(error),
            _ => None,
        };
        let Some(Ok(items)) = &mut self.pending else {
            return;
        };
        let connection = items
            .iter_mut()
            .filter(|item| item.subject == super::PendingSubjectName::Connection);
        for item in connection {
            item.wait.cause = match (&item.wait.reason, error) {
                (coven::PendingReason::Storage { failure, .. }, Some(error))
                    if error.failure() == *failure =>
                {
                    Some(error.clone() as Arc<dyn std::error::Error + Send + Sync>)
                }
                _ => None,
            };
        }
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
mod tests {
    use super::*;

    #[test]
    fn reading_pending_items_does_not_clear_a_connection_failure() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Disconnected,
            pending: None,
            abandoned: None,
            error: None,
        };
        let state = SyncStatusState {
            state: Arc::new(Mutex::new(snapshot.clone())),
            values: tokio::sync::watch::channel(snapshot).0,
            connected: tokio::sync::watch::channel(false).0,
        };
        state.set_error(Some(UiError::internal("connection failed")));
        state.set_pending(Ok(Vec::new()));
        assert!(state.snapshot().error.is_some());
    }

    #[test]
    fn pending_files_remain_waiting_after_the_sequence_is_synced() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Synced {
                finished_at: std::time::UNIX_EPOCH,
            },
            pending: Some(Ok(vec![super::super::PendingItem::before_open(
                coven::PendingRecord::new(
                    coven::PendingSubject::Connection,
                    coven::PendingReason::NoStorage,
                ),
            )])),
            abandoned: None,
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Waiting { items: 1 });
    }

    #[test]
    fn pending_authentication_offers_the_provider_sign_in() {
        let record = coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            coven::PendingReason::Storage {
                provider: coven::CloudProvider::Dropbox,
                failure: coven::StorageFailure::Authentication,
                answered: true,
            },
        );
        assert_eq!(
            pending_action(&record),
            PendingAction::SignIn(coven::CloudProvider::Dropbox)
        );
    }

    #[test]
    fn missing_provider_permission_offers_sign_in() {
        let record = coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            coven::PendingReason::Storage {
                provider: coven::CloudProvider::Dropbox,
                failure: coven::StorageFailure::AccessNotGranted {
                    required: coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox),
                },
                answered: true,
            },
        );
        assert_eq!(
            pending_action(&record),
            PendingAction::SignIn(coven::CloudProvider::Dropbox)
        );
    }

    #[test]
    fn a_failed_pass_with_no_pending_records_does_not_claim_synced() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Failed {
                error: coven::SyncFailure::Storage(Arc::new(
                    coven::StorageFailure::PermissionDenied.into(),
                )),
            },
            pending: Some(Ok(Vec::new())),
            abandoned: None,
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Failed);
        assert!(matches!(
            snapshot.failure(),
            Some(UiError::Diagnostic {
                category: crate::ui::UiErrorCategory::Storage(
                    coven::StorageFailure::PermissionDenied
                ),
                ..
            })
        ));
    }

    /// Only a pass in progress is syncing: a finished, offline, blocked,
    /// failed, stopped or absent sync is not, however long its loop runs.
    #[test]
    fn only_a_pass_in_progress_is_syncing() {
        let syncing = |status| {
            SyncStatusSnapshot {
                status,
                pending: Some(Ok(Vec::new())),
                abandoned: None,
                error: None,
            }
            .syncing()
        };
        assert!(syncing(coven::SyncStatus::Syncing));
        assert!(!syncing(coven::SyncStatus::Synced {
            finished_at: std::time::UNIX_EPOCH,
        }));
        assert!(!syncing(coven::SyncStatus::Offline {
            error: Arc::new(coven::StorageFailure::PermissionDenied.into()),
        }));
        assert!(!syncing(coven::SyncStatus::Blocked {
            slot: coven::Slot(3),
            reason: coven::PendingReason::NoStorage,
            error: None,
        }));
        assert!(!syncing(coven::SyncStatus::Failed {
            error: coven::SyncFailure::Removed,
        }));
        assert!(!syncing(coven::SyncStatus::Stopped));
        assert!(!syncing(coven::SyncStatus::Disconnected));
    }

    #[test]
    fn an_unread_pending_list_does_not_claim_synced() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Syncing,
            pending: None,
            abandoned: None,
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Loading);
    }

    /// With nothing pending, the line names the status; it says it is still
    /// checking only before the pending list's first read.
    #[test]
    fn with_nothing_pending_the_line_names_the_status() {
        let indicator = |status, error| {
            SyncStatusSnapshot {
                status,
                pending: Some(Ok(Vec::new())),
                abandoned: None,
                error,
            }
            .indicator()
        };
        let offline = || coven::SyncStatus::Offline {
            error: Arc::new(coven::StorageFailure::PermissionDenied.into()),
        };
        assert_eq!(
            indicator(coven::SyncStatus::Syncing, None),
            SyncIndicator::Syncing
        );
        assert_eq!(indicator(offline(), None), SyncIndicator::Offline);
        assert_eq!(
            indicator(
                coven::SyncStatus::Blocked {
                    slot: coven::Slot(3),
                    reason: coven::PendingReason::NoStorage,
                    error: None,
                },
                None
            ),
            SyncIndicator::Failed
        );
        assert_eq!(
            indicator(coven::SyncStatus::Stopped, None),
            SyncIndicator::Stopped
        );
        assert_eq!(
            indicator(
                coven::SyncStatus::Synced {
                    finished_at: std::time::UNIX_EPOCH,
                },
                Some(UiError::diagnostic(
                    crate::ui::UiErrorCategory::Internal,
                    "start failed"
                ))
            ),
            SyncIndicator::Failed
        );
    }

    #[test]
    fn a_pending_list_that_cannot_be_read_says_so() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Synced {
                finished_at: std::time::UNIX_EPOCH,
            },
            pending: Some(Err(UiError::diagnostic(
                crate::ui::UiErrorCategory::Internal,
                "database locked",
            ))),
            abandoned: None,
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Unreadable);
    }

    fn record(reason: coven::PendingReason) -> coven::PendingRecord {
        coven::PendingRecord::new(coven::PendingSubject::Connection, reason)
    }

    fn failed(error: coven::SyncFailure) -> SyncStatusSnapshot {
        SyncStatusSnapshot {
            status: coven::SyncStatus::Failed { error },
            pending: Some(Ok(Vec::new())),
            abandoned: None,
            error: None,
        }
    }

    fn category(snapshot: &SyncStatusSnapshot) -> crate::ui::UiErrorCategory {
        let Some(UiError::Diagnostic { category, .. }) = snapshot.failure() else {
            panic!("the failed pass is the status's failure");
        };
        category
    }

    /// A removed device says it was removed, and nothing it offers would
    /// register it again: its record is only reviewed, never reset.
    #[test]
    fn a_removed_device_says_so_and_offers_nothing_that_registers_it() {
        assert_eq!(
            pending_action(&record(coven::PendingReason::Removed)),
            PendingAction::Review
        );
        assert_eq!(
            category(&failed(coven::SyncFailure::Removed)),
            crate::ui::UiErrorCategory::DeviceRemoved
        );
    }

    /// Keys replaced by another of the person's devices are restored with the
    /// person's current code, not by resetting this device.
    #[test]
    fn replaced_member_keys_offer_restoring_from_the_code() {
        let member = coven::MemberId(uuid::Uuid::from_u128(7));
        assert_eq!(
            pending_action(&record(coven::PendingReason::MemberKeysReplaced { member })),
            PendingAction::RestoreFromCode
        );
    }

    /// Another copy of this device is the one sync failure a reset answers.
    #[test]
    fn another_copy_of_this_device_is_answered_by_a_reset() {
        assert_eq!(
            category(&failed(coven::SyncFailure::DeviceIdInUse(
                coven::DeviceIdEvidence::Replaced
            ))),
            crate::ui::UiErrorCategory::DeviceResetRequired
        );
    }

    fn state(status: coven::SyncStatus) -> SyncStatusState {
        let snapshot = SyncStatusSnapshot {
            status,
            pending: None,
            abandoned: None,
            error: None,
        };
        SyncStatusState {
            state: Arc::new(Mutex::new(snapshot.clone())),
            values: tokio::sync::watch::channel(snapshot).0,
            connected: tokio::sync::watch::channel(true).0,
        }
    }

    /// An S3 request that never reached the provider, as coven reports it:
    /// its class, and a cause whose message names each layer below it.
    fn unreachable_s3() -> Arc<coven::StorageError> {
        Arc::new(coven::StorageError::Provider {
            provider: coven::CloudProvider::S3,
            failure: coven::StorageFailure::Network,
            source: Box::new(std::io::Error::other(
                "S3 request failed: dispatch failure: io error: client error (Connect): \
                 invalid peer certificate: UnknownIssuer",
            )),
        })
    }

    const UNREACHABLE_S3: &str = "S3: Network: S3 request failed: dispatch failure: io error: \
         client error (Connect): invalid peer certificate: UnknownIssuer";

    fn connection_record(failure: coven::StorageFailure) -> super::super::PendingItem {
        super::super::PendingItem::before_open(record(coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            answered: failure != coven::StorageFailure::Network,
            failure,
        }))
    }

    /// A retry that didn't sync answers with what the status shows for the
    /// same failure: an offline pass is the network's, every layer of its
    /// cause kept, and a removed device says so.
    #[test]
    fn a_failed_retry_answers_with_the_failure_the_status_shows() {
        use crate::ui::UiErrorCategory as C;
        let offline =
            SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Offline {
                error: unreachable_s3(),
            }));
        assert!(
            matches!(&offline, SyncRetryOutcome::Failed(UiError::Diagnostic {
                category: C::Network,
                detail,
            }) if detail == UNREACHABLE_S3),
            "{offline:?}"
        );
        let removed =
            SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Failed {
                error: coven::SyncFailure::Removed,
            }));
        assert!(
            matches!(
                removed,
                SyncRetryOutcome::Failed(UiError::Diagnostic {
                    category: C::DeviceRemoved,
                    ..
                })
            ),
            "{removed:?}"
        );
    }

    /// A slot blocked since before this process opened has no error of its
    /// own; the retry's answer still says why, from the slot's reason.
    #[test]
    fn a_retry_held_by_a_slot_blocked_before_opening_names_its_reason() {
        let blocked =
            SyncRetryOutcome::from(coven::RetryOutcome::Failed(coven::PassFailure::Blocked {
                slot: coven::Slot(7),
                reason: coven::PendingReason::Removed,
                error: None,
            }));
        assert!(
            matches!(&blocked, SyncRetryOutcome::Failed(UiError::Diagnostic { detail, .. })
                if detail.contains("Slot(7)") && detail.contains("Removed")),
            "{blocked:?}"
        );
    }

    /// The provider's cooldown keeps its end, and sync not running says so.
    #[test]
    fn a_retry_that_could_not_run_says_why() {
        let until = std::time::UNIX_EPOCH + std::time::Duration::from_secs(600);
        assert!(matches!(
            SyncRetryOutcome::from(coven::RetryOutcome::CoolingDown { until }),
            SyncRetryOutcome::CoolingDown { until: at } if at == until
        ));
        assert!(matches!(
            SyncRetryOutcome::from(coven::RetryOutcome::NotSyncing),
            SyncRetryOutcome::NotSyncing
        ));
        assert!(matches!(
            SyncRetryOutcome::from(coven::RetryOutcome::Synced { finished_at: until }),
            SyncRetryOutcome::Synced
        ));
    }

    #[test]
    fn an_offline_status_names_every_layer_of_its_cause() {
        let state = state(coven::SyncStatus::Offline {
            error: unreachable_s3(),
        });
        assert_eq!(
            state.snapshot().failure(),
            Some(UiError::Diagnostic {
                category: crate::ui::UiErrorCategory::Network,
                detail: UNREACHABLE_S3.to_string(),
            })
        );
    }

    /// The connection's record keeps only the failure's class; while the
    /// status holds the error behind it, the record carries that error,
    /// whichever of the two arrives first.
    #[test]
    fn the_connection_record_carries_the_error_behind_it() {
        let offline = coven::SyncStatus::Offline {
            error: unreachable_s3(),
        };
        let cause = |state: &SyncStatusState| {
            let Some(Ok(items)) = state.snapshot().pending else {
                panic!("the pending records were read");
            };
            items[0].wait.cause.as_ref().map(ToString::to_string)
        };

        let status_first = state(offline.clone());
        status_first.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
        assert_eq!(cause(&status_first).as_deref(), Some(UNREACHABLE_S3));

        let records_first = state(coven::SyncStatus::Syncing);
        records_first.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
        assert_eq!(cause(&records_first), None);
        records_first.set_status(offline);
        assert_eq!(cause(&records_first).as_deref(), Some(UNREACHABLE_S3));
        records_first.set_status(coven::SyncStatus::Syncing);
        assert_eq!(cause(&records_first), None);
    }

    /// A record of another failure, or a status error with no cause left (as
    /// rebuilt from the record after a restart), names nothing more than the
    /// record does.
    #[test]
    fn the_connection_record_names_only_a_matching_error_with_a_cause() {
        let other = state(coven::SyncStatus::Offline {
            error: unreachable_s3(),
        });
        other.set_pending(Ok(vec![connection_record(
            coven::StorageFailure::Authentication,
        )]));
        let restarted = state(coven::SyncStatus::Failed {
            error: coven::SyncFailure::Storage(Arc::new(coven::StorageFailure::Network.into())),
        });
        restarted.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
        for state in [other, restarted] {
            let Some(Ok(items)) = state.snapshot().pending else {
                panic!("the pending records were read");
            };
            assert!(items[0].wait.cause.is_none());
        }
    }

    /// A failure is logged once, with its whole cause, when it appears or
    /// changes; the same failure published again, or a change to anything
    /// else, logs nothing more.
    #[test]
    fn a_new_sync_failure_logs_one_warning_with_its_cause() {
        let state = state(coven::SyncStatus::Syncing);
        let offline = coven::SyncStatus::Offline {
            error: unreachable_s3(),
        };
        let logs = crate::test_logs::capture_warn_logs(|| {
            state.set_status(offline.clone());
            state.set_status(offline.clone());
            state.set_pending(Ok(vec![connection_record(coven::StorageFailure::Network)]));
        });
        assert_eq!(logs.matches(UNREACHABLE_S3).count(), 1, "{logs}");
        assert!(logs.contains("Network"), "{logs}");

        let logs = crate::test_logs::capture_warn_logs(|| {
            state.set_status(coven::SyncStatus::Syncing);
            state.set_status(offline);
        });
        assert_eq!(logs.matches(UNREACHABLE_S3).count(), 1, "{logs}");
    }

    /// A blocked slot surfaces the error that blocked it, in the class of its
    /// reason; a device that opened already blocked has no error to show.
    #[test]
    fn a_blocked_slot_surfaces_the_error_that_blocked_it() {
        let blocked = |error| SyncStatusSnapshot {
            status: coven::SyncStatus::Blocked {
                slot: coven::Slot(3),
                reason: coven::PendingReason::Storage {
                    provider: coven::CloudProvider::S3,
                    failure: coven::StorageFailure::Network,
                    answered: false,
                },
                error,
            },
            pending: Some(Ok(Vec::new())),
            abandoned: None,
            error: None,
        };
        assert_eq!(
            blocked(Some(
                unreachable_s3() as Arc<dyn std::error::Error + Send + Sync>
            ))
            .failure(),
            Some(UiError::Diagnostic {
                category: crate::ui::UiErrorCategory::Storage(coven::StorageFailure::Network),
                detail: UNREACHABLE_S3.to_string(),
            })
        );
        assert_eq!(blocked(None).failure(), None);
    }

    /// coven logs the error that blocked a slot itself, so publishing it
    /// here logs nothing more.
    #[test]
    fn a_blocked_slot_is_not_logged_again() {
        let state = state(coven::SyncStatus::Syncing);
        let logs = crate::test_logs::capture_warn_logs(|| {
            state.set_status(coven::SyncStatus::Blocked {
                slot: coven::Slot(3),
                reason: coven::PendingReason::NoStorage,
                error: Some(unreachable_s3()),
            });
        });
        assert!(!logs.contains(UNREACHABLE_S3), "{logs}");
        assert!(state.snapshot().failure().is_some());
    }

    /// coven holds the error behind every record but the connection's; naming
    /// the connection's from the status leaves the others' as coven gave them.
    #[test]
    fn a_record_keeps_the_error_coven_holds_for_it() {
        let state = state(coven::SyncStatus::Syncing);
        let mut record = coven::PendingRecord::new(
            coven::PendingSubject::Audience(coven::Audience::Store),
            coven::PendingReason::Failed(coven::LocalFailure::Disk),
        );
        record.cause = Some(Arc::new(std::io::Error::other("disk full")));
        state.set_pending(Ok(vec![super::super::PendingItem::before_open(record)]));
        state.set_status(coven::SyncStatus::Offline {
            error: unreachable_s3(),
        });
        let Some(Ok(items)) = state.snapshot().pending else {
            panic!("the pending records were read");
        };
        assert_eq!(
            items[0]
                .wait
                .cause
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("disk full")
        );
    }
}
