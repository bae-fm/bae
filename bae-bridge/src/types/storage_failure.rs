#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageFailure {
    ProviderUnavailable,
    AccessNotRegistered { required: crate::BridgeOAuthAccess },
    AccessNotGranted { required: crate::BridgeOAuthAccess },
    DropboxAppFolderNotShareable,
    Network,
    Authentication,
    PermissionDenied,
    NotFound,
    AlreadyExists,
    Conflict,
    ContainerNotFound,
    RegionMismatch,
    OwnBucketRequired { reason: BridgeOwnBucketReason },
    StoreExists,
    ClockSkew,
    QuotaExceeded,
    RateLimited,
    InvalidConfiguration,
    Refused,
    Protocol,
    InvalidPath,
    InvalidRange,
    NotStoreOwner,
    InviteeIsStoreOwner,
    AccountIdUnavailable,
    InvitationMismatch,
    SessionMismatch,
    SessionIdentityUnknown,
    SessionExpired,
    InvalidPart,
    SingleRequestTooLarge { size: u64, limit: u64 },
    CopyTooLarge { size: u64, limit: u64 },
    Encoding,
    File,
    MemberKeysMissing,
    CatchUpRequired,
}

mirror_enum! {
 BridgeStorageFailure = coven::StorageFailure,
 from_core: pub(crate) fn,
 variants: {
ProviderUnavailable,
AccessNotRegistered { required: (crate::BridgeOAuthAccess) },
AccessNotGranted { required: (crate::BridgeOAuthAccess) },
DropboxAppFolderNotShareable,
Network,
Authentication,
PermissionDenied,
NotFound,
AlreadyExists,
Conflict,
ContainerNotFound,
RegionMismatch,
OwnBucketRequired { reason: (BridgeOwnBucketReason) },
StoreExists,
ClockSkew,
QuotaExceeded,
RateLimited,
InvalidConfiguration,
Refused,
Protocol,
InvalidPath,
InvalidRange,
NotStoreOwner,
InviteeIsStoreOwner,
AccountIdUnavailable,
InvitationMismatch,
SessionMismatch,
SessionIdentityUnknown,
SessionExpired,
InvalidPart,
SingleRequestTooLarge {
        size,
        limit,
    },
CopyTooLarge {
        size,
        limit,
    },
Encoding,
File,
MemberKeysMissing,
CatchUpRequired,
 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOwnBucketReason {
    Prefix,
    Occupied,
}
mirror_enum! {
 BridgeOwnBucketReason = coven::OwnBucketReason,
 from_core: fn,
 variants: { Prefix, Occupied },
}

pub(crate) fn storage_failure_key(failure: BridgeStorageFailure) -> &'static str {
    match failure {
        BridgeStorageFailure::ProviderUnavailable => "core.error.storage.provider_unavailable",
        BridgeStorageFailure::AccessNotRegistered { .. } => {
            "core.error.storage.access_not_registered"
        }
        BridgeStorageFailure::AccessNotGranted { .. } => "core.error.storage.access_not_granted",
        BridgeStorageFailure::DropboxAppFolderNotShareable => {
            "core.error.storage.dropbox_app_folder_not_shareable"
        }
        BridgeStorageFailure::Network => "core.error.category.network",
        BridgeStorageFailure::Authentication => "core.error.category.credentials",
        BridgeStorageFailure::PermissionDenied => "core.error.cloud_setup.permission_denied",
        BridgeStorageFailure::NotFound => "core.error.storage.not_found",
        BridgeStorageFailure::AlreadyExists => "core.error.storage.already_exists",
        BridgeStorageFailure::Conflict => "core.error.storage.conflict",
        BridgeStorageFailure::ContainerNotFound => "core.error.cloud_setup.container_not_found",
        BridgeStorageFailure::RegionMismatch => "core.error.cloud_setup.region_mismatch",
        BridgeStorageFailure::OwnBucketRequired { .. } => "core.error.storage.own_bucket_required",
        BridgeStorageFailure::StoreExists => "core.error.cloud_setup.location_occupied",
        BridgeStorageFailure::ClockSkew => "core.error.storage.clock_skew",
        BridgeStorageFailure::QuotaExceeded => "core.error.cloud_setup.quota_exceeded",
        BridgeStorageFailure::RateLimited => "core.error.storage.rate_limited",
        BridgeStorageFailure::InvalidConfiguration => {
            "core.error.cloud_setup.invalid_configuration"
        }
        BridgeStorageFailure::Refused => "core.pending.reason.refused",
        BridgeStorageFailure::Protocol => "core.error.storage.invalid_response",
        BridgeStorageFailure::InvalidPath => "core.error.storage.invalid_request",
        BridgeStorageFailure::InvalidRange => "core.error.storage.invalid_request",
        BridgeStorageFailure::NotStoreOwner => "core.error.storage.not_store_owner",
        BridgeStorageFailure::InviteeIsStoreOwner => "core.error.storage.invitee_is_store_owner",
        BridgeStorageFailure::AccountIdUnavailable => "core.error.storage.account_id_unavailable",
        BridgeStorageFailure::InvitationMismatch => "core.error.storage.invitation_mismatch",
        BridgeStorageFailure::SessionMismatch => "core.error.storage.session_mismatch",
        BridgeStorageFailure::SessionIdentityUnknown => {
            "core.error.storage.session_identity_unknown"
        }
        BridgeStorageFailure::SessionExpired => "core.error.storage.session_expired",
        BridgeStorageFailure::InvalidPart => "core.error.storage.invalid_request",
        BridgeStorageFailure::SingleRequestTooLarge { .. } => {
            "core.error.storage.request_too_large"
        }
        BridgeStorageFailure::CopyTooLarge { .. } => "core.error.storage.request_too_large",
        BridgeStorageFailure::Encoding => "core.error.storage.invalid_response",
        BridgeStorageFailure::File => "core.error.storage.file",
        BridgeStorageFailure::MemberKeysMissing => "core.error.storage.member_keys_missing",
        BridgeStorageFailure::CatchUpRequired => "core.pending.reason.waits",
    }
}
