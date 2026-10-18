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

    fn apply(&self, change: impl FnOnce(&mut SyncStatusSnapshot)) {
        let mut state = self.state.lock().unwrap();
        change(&mut state);
        self.connected.send_replace(state.connected());
        self.values.send_replace(state.clone());
    }

    pub(super) fn set_status(&self, status: coven::SyncStatus) {
        self.apply(|state| state.status = status);
    }

    pub(super) fn set_pending(&self, pending: coven::CovenResult<Vec<coven::PendingRecord>>) {
        self.apply(|state| {
            state.pending = Some(pending.map_err(|error| {
                tracing::warn!(%error, "could not read pending sync records");
                UiError::internal(error)
            }));
        });
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

impl SyncStatusSnapshot {
    pub fn connected(&self) -> bool {
        !matches!(self.status, coven::SyncStatus::Disconnected)
    }

    pub fn running(&self) -> bool {
        !matches!(
            self.status,
            coven::SyncStatus::Disconnected | coven::SyncStatus::Stopped
        )
    }

    pub fn failure(&self) -> Option<UiError> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        use crate::ui::UiErrorCategory as C;
        match &self.status {
            coven::SyncStatus::Offline { error } => Some(UiError::diagnostic(C::Network, error)),
            coven::SyncStatus::Failed { error } => {
                let category = match error {
                    coven::SyncFailure::UpdateRequired(_) => C::SyncUpdateRequired,
                    coven::SyncFailure::DeviceIdInUse(_) => C::DeviceResetRequired,
                    coven::SyncFailure::Removed => C::Membership,
                    coven::SyncFailure::Storage(error) => {
                        super::error_category::cloud_home_category(error)
                    }
                    coven::SyncFailure::Operation(_) | coven::SyncFailure::Other(_) => C::Internal,
                };
                Some(UiError::diagnostic(category, error))
            }
            _ => None,
        }
    }

    pub fn indicator(&self) -> SyncIndicator {
        if !self.connected() {
            return SyncIndicator::Unconnected;
        }
        match &self.pending {
            None | Some(Err(_)) => SyncIndicator::Loading,
            Some(Ok(pending)) if !pending.is_empty() => SyncIndicator::Waiting {
                items: pending.len(),
            },
            Some(Ok(_))
                if matches!(self.status, coven::SyncStatus::Synced { .. })
                    && self.error.is_none() =>
            {
                SyncIndicator::Synced
            }
            Some(Ok(_)) => SyncIndicator::Loading,
        }
    }
}

/// The action a pending record offers. The record itself retains the subject
/// and typed reason, including both failures when an undo cannot finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    Wait,
    UpdateApp,
    SignIn(coven::CloudProvider),
    Reconnect,
    RestoreFile,
    ResumeUploads,
    RetryOperation(coven::OperationId),
    ResetDevice,
    ProvideAdminAccess(coven::OperationId),
    DeleteAccessKey {
        operation: coven::OperationId,
        access_key_id: String,
    },
    Review,
}

pub fn pending_action(record: &coven::PendingRecord) -> PendingAction {
    use coven::{PendingReason as R, PendingSubject as S};
    let mut reason = &record.reason;
    while let R::Cleanup { cleanup, .. } = reason {
        reason = cleanup;
    }
    match reason {
        R::AdminAccessRequired => match record.subject {
            S::Operation { id, .. } => PendingAction::ProvideAdminAccess(id),
            _ => PendingAction::Review,
        },
        R::DeleteAccessKey(key) => match record.subject {
            S::Operation { id, .. } => PendingAction::DeleteAccessKey {
                operation: id,
                access_key_id: key.access_key_id.clone(),
            },
            _ => PendingAction::Review,
        },
        R::NoStorage => PendingAction::Reconnect,
        R::Storage {
            provider,
            failure:
                coven::StorageFailure::Authentication | coven::StorageFailure::AccessNotGranted { .. },
        } => PendingAction::SignIn(*provider),
        R::FileUnavailable(coven::FileMissingReason::Source(_)) => PendingAction::RestoreFile,
        R::UpdateRequired(_) | R::Refused(_) => PendingAction::UpdateApp,
        R::Removed | R::MemberKeysReplaced { .. } => PendingAction::ResetDevice,
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
            pending: Some(Ok(vec![coven::PendingRecord {
                subject: coven::PendingSubject::Connection,
                reason: coven::PendingReason::NoStorage,
            }])),
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Waiting { items: 1 });
    }

    #[test]
    fn pending_authentication_offers_the_provider_sign_in() {
        let record = coven::PendingRecord {
            subject: coven::PendingSubject::Connection,
            reason: coven::PendingReason::Storage {
                provider: coven::CloudProvider::Dropbox,
                failure: coven::StorageFailure::Authentication,
            },
        };
        assert_eq!(
            pending_action(&record),
            PendingAction::SignIn(coven::CloudProvider::Dropbox)
        );
    }

    #[test]
    fn missing_provider_permission_offers_sign_in() {
        let record = coven::PendingRecord {
            subject: coven::PendingSubject::Connection,
            reason: coven::PendingReason::Storage {
                provider: coven::CloudProvider::Dropbox,
                failure: coven::StorageFailure::AccessNotGranted {
                    required: coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox),
                },
            },
        };
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
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Loading);
        assert!(matches!(
            snapshot.failure(),
            Some(UiError::Diagnostic {
                category: crate::ui::UiErrorCategory::Credentials,
                ..
            })
        ));
    }

    #[test]
    fn an_unread_pending_list_does_not_claim_synced() {
        let snapshot = SyncStatusSnapshot {
            status: coven::SyncStatus::Syncing,
            pending: None,
            error: None,
        };
        assert_eq!(snapshot.indicator(), SyncIndicator::Loading);
    }
}
