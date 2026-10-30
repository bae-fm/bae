use super::{BridgeCloudProvider, BridgeError, BridgeMessage, BridgeMessageArg};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePendingRecords {
    Loading,
    Ready { records: Vec<BridgePendingRecord> },
    Failed { error: BridgeError },
}
impl BridgePendingRecords {
    pub(crate) fn from_core(
        value: Option<Result<Vec<bae_core::library::PendingItem>, bae_core::ui::UiError>>,
    ) -> Self {
        match value {
            None => Self::Loading,
            Some(Ok(items)) => Self::Ready {
                records: items
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

/// One thing coven is waiting on: what it is about, why, and what the person
/// can do about it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingRecord {
    /// What the record is about: a file by its name and album, anything else
    /// by what it is.
    pub subject: BridgeMessage,
    pub reason: BridgePendingReason,
    pub action: BridgePendingAction,
    /// The action's catalog key; none for `Wait`, which offers nothing to do.
    pub action_key: Option<String>,
}
impl BridgePendingRecord {
    pub(crate) fn from_core(item: bae_core::library::PendingItem) -> Self {
        let BridgePendingWait { reason, action } = BridgePendingWait::from_core(item.wait);
        Self {
            subject: subject_message(item.subject),
            reason,
            action_key: action.key().map(str::to_string),
            action,
        }
    }
}

/// Why a file in the upload queue waits, and what the person can do about it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingWait {
    pub reason: BridgePendingReason,
    pub action: BridgePendingAction,
}
impl BridgePendingWait {
    pub(crate) fn from_core(wait: bae_core::library::PendingWait) -> Self {
        Self {
            reason: BridgePendingReason::from_core(wait.reason, wait.cause),
            action: BridgePendingAction::from_core(wait.action),
        }
    }
}

/// Why a record waits, as a catalog key, with the error behind it and the
/// reasons an undo that could not finish is made of.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingReason {
    pub key: String,
    /// The text of the error behind the reason, with every cause below it,
    /// while core holds that error: it names what the reason's class leaves
    /// out, already redacted by coven. None when no error is held, as after a
    /// restart; the UI then offers no details.
    pub detail: Option<String>,
    pub causes: Vec<BridgePendingReason>,
}
/// The class of a coven reason, which names its catalog key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingReasonKind {
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
    FileUnavailable,
    FileChanged,
    FileLost,
    Failed,
    Rejected,
    Cleanup,
    Paused,
    Removed,
    MemberKeysReplaced,
}
impl BridgePendingReason {
    pub(crate) fn from_core(
        reason: coven::PendingReason,
        cause: Option<std::sync::Arc<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        let detail = cause.map(|cause| cause.to_string());
        use coven::PendingReason as R;
        use PendingReasonKind as K;
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
            R::FileUnavailable(coven::FileMissingReason::Source(_)) => K::FileUnavailable,
            R::FileUnavailable(coven::FileMissingReason::Lost) => K::FileLost,
            R::Original {
                failure: coven::FileSourceFailure::Missing,
                ..
            } => K::FileUnavailable,
            R::Original {
                failure: coven::FileSourceFailure::Changed | coven::FileSourceFailure::Integrity,
                ..
            } => K::FileChanged,
            R::Failed(_) => K::Failed,
            R::Rejected(_) => K::Rejected,
            R::Cleanup { .. } => K::Cleanup,
            R::Paused => K::Paused,
            R::Removed => K::Removed,
            R::MemberKeysReplaced { .. } => K::MemberKeysReplaced,
        };
        let causes = match reason {
            R::Cleanup { operation, cleanup } => {
                vec![
                    Self::from_core(*operation, None),
                    Self::from_core(*cleanup, None),
                ]
            }
            _ => Vec::new(),
        };
        Self {
            key: kind.key().to_string(),
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
    /// Pin the release again: `queue_pin_releases`, which queues a failed
    /// pin again in its place.
    RetryDownload {
        release_id: String,
    },
    /// End the release's failed move to the cloud: `cancel_failed_release_move`.
    CancelUpload {
        release_id: String,
        id: i64,
    },
    RestoreFromCode,
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
            A::RetryDownload { release_id } => Self::RetryDownload { release_id },
            A::CancelUpload { release_id, id } => Self::CancelUpload {
                release_id,
                id: id.0,
            },
            A::RestoreFromCode => Self::RestoreFromCode,
            A::Review => Self::Review,
        }
    }
}

impl PendingReasonKind {
    /// The UI translates this key and offers the original detail separately.
    pub(crate) fn key(self) -> &'static str {
        use PendingReasonKind as R;
        match self {
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
            R::FileUnavailable => "core.pending.reason.file_unavailable",
            R::FileChanged => "core.pending.reason.file_changed",
            R::FileLost => "core.pending.reason.file_lost",
            R::Failed => "core.pending.reason.failed",
            R::Rejected => "core.pending.reason.rejected",
            R::Cleanup => "core.pending.reason.cleanup",
            R::Paused => "core.pending.reason.paused",
            R::Removed => "core.pending.reason.removed",
            R::MemberKeysReplaced => "core.pending.reason.member_keys_replaced",
        }
    }
}

impl BridgePendingAction {
    /// The action's catalog key; `Wait` offers nothing to do and has none.
    pub(crate) fn key(&self) -> Option<&'static str> {
        use BridgePendingAction as A;
        Some(match self {
            A::Wait => return None,
            A::UpdateApp => "core.pending.action.update_app",
            A::SignIn { .. } => "core.pending.action.sign_in",
            A::Reconnect => "core.pending.action.reconnect",
            A::RestoreFile => "core.pending.action.restore_file",
            A::ResumeUploads => "core.pending.action.resume_uploads",
            A::RetryOperation { .. } | A::RetryDownload { .. } => "core.pending.action.retry",
            A::CancelUpload { .. } => "core.pending.action.cancel_upload",
            A::RestoreFromCode => "core.pending.action.restore_from_code",
            A::Review => "core.pending.action.review",
        })
    }
}

/// The subject as a catalog message. A release file's message takes its
/// `file` name and `album` title and a cover's its `album` title, in the
/// order the source message names them.
pub(crate) fn subject_message(subject: bae_core::library::PendingSubjectName) -> BridgeMessage {
    use bae_core::library::PendingSubjectName as S;
    use coven::OperationKind as O;
    let key = match subject {
        S::ReleaseFile {
            file_name,
            album_title,
        } => {
            return BridgeMessage {
                key: "core.pending.subject.release_file".to_string(),
                args: vec![
                    BridgeMessageArg::new("file", file_name),
                    BridgeMessageArg::new("album", album_title),
                ],
            }
        }
        S::Cover { album_title } => {
            return BridgeMessage {
                key: "core.pending.subject.cover".to_string(),
                args: vec![BridgeMessageArg::new("album", album_title)],
            }
        }
        S::ArtistImage => "core.pending.subject.artist_image",
        S::Avatar => "core.pending.subject.avatar",
        S::DeletedReleaseFile => "core.pending.subject.deleted_release_file",
        S::File => "core.pending.subject.file",
        S::LocalChanges => "core.pending.subject.local_changes",
        S::LibraryChanges => "core.pending.subject.library_changes",
        S::KeyCopy => "core.pending.subject.key_copy",
        S::Snapshot => "core.pending.subject.snapshot",
        S::Retention => "core.pending.subject.retention",
        S::LibraryData => "core.pending.subject.library_data",
        S::CircleData => "core.pending.subject.circle_data",
        S::JoinRequest => "core.pending.subject.join_request",
        S::Connection => "core.pending.subject.connection",
        S::Operation(kind) => match kind {
            O::CreateStore => "core.pending.operation.create_store",
            O::RenameStore => "core.pending.operation.rename_store",
            O::AddDevice => "core.pending.operation.add_device",
            O::SetRole => "core.pending.operation.set_role",
            O::SetAccess => "core.pending.operation.set_access",
            O::RemoveMember => "core.pending.operation.remove_member",
            O::RemoveDevice => "core.pending.operation.remove_device",
            O::RenameCircle => "core.pending.operation.rename_circle",
            O::RemoveCircleMember => "core.pending.operation.remove_circle_member",
            O::RetainSequence => "core.pending.operation.retain_sequence",
            O::RetainFiles => "core.pending.operation.retain_files",
            O::WriteSnapshot => "core.pending.operation.write_snapshot",
            O::SchemaChange => "core.pending.operation.schema_change",
            O::Reset => "core.pending.operation.reset",
            O::ReloadFromSnapshot => "core.pending.operation.reload_from_snapshot",
            O::RequiredReload => "core.pending.operation.required_reload",
            O::RotateKey => "core.pending.operation.rotate_key",
            O::CreateCircle => "core.pending.operation.create_circle",
            O::AddCircleMember => "core.pending.operation.add_circle_member",
            O::DeleteCircle => "core.pending.operation.delete_circle",
            O::Invite => "core.pending.operation.invite",
            O::RevokeAccess => "core.pending.operation.revoke_access",
            O::ReplaceMemberKeys => "core.pending.operation.replace_member_keys",
        },
    };
    BridgeMessage {
        key: key.to_string(),
        args: Vec::new(),
    }
}
