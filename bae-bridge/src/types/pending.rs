use super::{BridgeCloudProvider, BridgeError};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePendingRecords {
    Loading,
    Ready { records: Vec<BridgePendingRecord> },
    Failed { error: BridgeError },
}
impl BridgePendingRecords {
    pub(crate) fn from_core(
        value: Option<Result<Vec<coven::PendingRecord>, bae_core::ui::UiError>>,
    ) -> Self {
        match value {
            None => Self::Loading,
            Some(Ok(records)) => Self::Ready {
                records: records
                    .into_iter()
                    .map(BridgePendingRecord::from_core)
                    .collect(),
            },
            Some(Err(error)) => Self::Failed {
                error: BridgeError::from_core(error),
            },
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingRecord {
    pub subject: BridgePendingSubject,
    pub reason: BridgePendingReason,
    pub action: BridgePendingAction,
}
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePendingSubject {
    Write { identity: String },
    Slot { identity: String },
    KeyCopy { identity: String },
    Snapshot { identity: String },
    File { device_id: u64, file_id: String },
    Operation { id: i64, kind: String },
    Retention { path: String },
    Audience { identity: String },
    JoinRequest { invite_id: String },
    Connection,
}
impl BridgePendingRecord {
    pub(crate) fn from_core(record: coven::PendingRecord) -> Self {
        let action = BridgePendingAction::from_core(bae_core::library::pending_action(&record));
        let reason = BridgePendingReason::from_core(record.reason);
        use coven::PendingSubject as S;
        let subject = match record.subject {
            S::Write(id) => BridgePendingSubject::Write {
                identity: format!("{id:?}"),
            },
            S::Slot(id) => BridgePendingSubject::Slot {
                identity: format!("{id:?}"),
            },
            subject @ S::KeyCopy { .. } => BridgePendingSubject::KeyCopy {
                identity: format!("{subject:?}"),
            },
            S::Snapshot(id) => BridgePendingSubject::Snapshot {
                identity: format!("{id:?}"),
            },
            S::File { device, file } => BridgePendingSubject::File {
                device_id: device.0,
                file_id: file.to_string(),
            },
            S::Operation { id, kind } => BridgePendingSubject::Operation {
                id: id.0,
                kind: format!("{kind:?}"),
            },
            S::Retention { path } => BridgePendingSubject::Retention {
                path: path.as_str().to_string(),
            },
            S::Audience(id) => BridgePendingSubject::Audience {
                identity: format!("{id:?}"),
            },
            S::JoinRequest(id) => BridgePendingSubject::JoinRequest {
                invite_id: id.to_string(),
            },
            S::Connection => BridgePendingSubject::Connection,
        };
        Self {
            subject,
            reason,
            action,
        }
    }
}

/// A localized reason kind with the original diagnostics and nested cleanup causes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingReason {
    pub kind: BridgePendingReasonKind,
    pub detail: String,
    pub causes: Vec<BridgePendingReason>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePendingReasonKind {
    Missing,
    Waits,
    KeyUnavailable,
    UpdateRequired,
    NoStorage,
    Storage,
    Refused,
    WritesRefused,
    ProviderPending,
    PendingOwner,
    AccountInUse,
    AccessRemains,
    DeleteAccessKey,
    FileUnavailable,
    Failed,
    Rejected,
    Cleanup,
    Paused,
    AdminAccessRequired,
    Removed,
    MemberKeysReplaced,
}
impl BridgePendingReason {
    fn from_core(reason: coven::PendingReason) -> Self {
        let detail = format!("{reason:?}");
        use coven::PendingReason as R;
        use BridgePendingReasonKind as K;
        let kind = match &reason {
            R::Missing { .. } => K::Missing,
            R::Waits(_) => K::Waits,
            R::KeyUnavailable { .. } => K::KeyUnavailable,
            R::UpdateRequired(_) => K::UpdateRequired,
            R::NoStorage => K::NoStorage,
            R::Storage { .. } => K::Storage,
            R::Refused(_) => K::Refused,
            R::WritesRefused { .. } => K::WritesRefused,
            R::ProviderPending { .. } => K::ProviderPending,
            R::PendingOwner { .. } => K::PendingOwner,
            R::AccountInUse { .. } => K::AccountInUse,
            R::AccessRemains { .. } => K::AccessRemains,
            R::DeleteAccessKey(_) => K::DeleteAccessKey,
            R::FileUnavailable(_) => K::FileUnavailable,
            R::Failed(_) => K::Failed,
            R::Rejected(_) => K::Rejected,
            R::Cleanup { .. } => K::Cleanup,
            R::Paused => K::Paused,
            R::AdminAccessRequired => K::AdminAccessRequired,
            R::Removed => K::Removed,
            R::MemberKeysReplaced { .. } => K::MemberKeysReplaced,
        };
        let causes = match reason {
            R::Cleanup { operation, cleanup } => {
                vec![Self::from_core(*operation), Self::from_core(*cleanup)]
            }
            _ => Vec::new(),
        };
        Self {
            kind,
            detail,
            causes,
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePendingAction {
    Wait,
    UpdateApp,
    SignIn {
        provider: BridgeCloudProvider,
    },
    Reconnect,
    RestoreFile,
    ResumeUploads,
    RetryOperation {
        id: i64,
    },
    ResetDevice,
    ProvideAdminAccess {
        id: i64,
    },
    DeleteAccessKey {
        operation: i64,
        access_key_id: String,
    },
    Review,
}
impl BridgePendingAction {
    fn from_core(value: bae_core::library::PendingAction) -> Self {
        use bae_core::library::PendingAction as A;
        match value {
            A::Wait => Self::Wait,
            A::UpdateApp => Self::UpdateApp,
            A::SignIn(provider) => Self::SignIn {
                provider: BridgeCloudProvider::from_core(&provider),
            },
            A::Reconnect => Self::Reconnect,
            A::RestoreFile => Self::RestoreFile,
            A::ResumeUploads => Self::ResumeUploads,
            A::RetryOperation(id) => Self::RetryOperation { id: id.0 },
            A::ResetDevice => Self::ResetDevice,
            A::ProvideAdminAccess(id) => Self::ProvideAdminAccess { id: id.0 },
            A::DeleteAccessKey {
                operation,
                access_key_id,
            } => Self::DeleteAccessKey {
                operation: operation.0,
                access_key_id,
            },
            A::Review => Self::Review,
        }
    }
}

/// The UI translates this key and offers the original detail separately.
#[uniffi::export]
pub fn bridge_pending_reason_key(reason: BridgePendingReasonKind) -> String {
    use BridgePendingReasonKind as R;
    match reason {
        R::Missing => "core.pending.reason.missing",
        R::Waits => "core.pending.reason.waits",
        R::KeyUnavailable => "core.pending.reason.key_unavailable",
        R::UpdateRequired => "core.error.sync_update_required",
        R::NoStorage => "core.pending.reason.no_storage",
        R::Storage => "core.pending.reason.storage",
        R::Refused => "core.pending.reason.refused",
        R::WritesRefused => "core.pending.reason.writes_refused",
        R::ProviderPending => "core.pending.reason.provider_pending",
        R::PendingOwner => "core.pending.reason.pending_owner",
        R::AccountInUse => "core.pending.reason.account_in_use",
        R::AccessRemains => "core.pending.reason.access_remains",
        R::DeleteAccessKey => "core.pending.reason.delete_access_key",
        R::FileUnavailable => "core.pending.reason.file_unavailable",
        R::Failed => "core.pending.reason.failed",
        R::Rejected => "core.pending.reason.rejected",
        R::Cleanup => "core.pending.reason.cleanup",
        R::Paused => "core.pending.reason.paused",
        R::AdminAccessRequired => "core.pending.reason.admin_access_required",
        R::Removed => "core.pending.reason.removed",
        R::MemberKeysReplaced => "core.pending.reason.member_keys_replaced",
    }
    .to_string()
}

#[uniffi::export]
pub fn bridge_pending_action_key(action: &BridgePendingAction) -> Option<String> {
    use BridgePendingAction as A;
    Some(
        match action {
            A::Wait => return None,
            A::UpdateApp => "core.pending.action.update_app",
            A::SignIn { .. } => "core.pending.action.sign_in",
            A::Reconnect => "core.pending.action.reconnect",
            A::RestoreFile => "core.pending.action.restore_file",
            A::ResumeUploads => "core.pending.action.resume_uploads",
            A::RetryOperation { .. } => "core.pending.action.retry",
            A::ResetDevice => "core.pending.action.reset_device",
            A::ProvideAdminAccess { .. } => "core.pending.action.provide_admin_access",
            A::DeleteAccessKey { .. } => "core.pending.action.delete_access_key",
            A::Review => "core.pending.action.review",
        }
        .to_string(),
    )
}
