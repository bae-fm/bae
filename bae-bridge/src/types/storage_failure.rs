#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageFailure {
    ProviderUnavailable,
    AccessNotRegistered {
        required: crate::BridgeOAuthAccess,
    },
    AccessNotGranted {
        required: crate::BridgeOAuthAccess,
    },
    DropboxAppFolderNotShareable,
    Network,
    Corrupted,
    Authentication,
    PermissionDenied,
    NotFound,
    AlreadyExists,
    Conflict,
    ContainerNotFound,
    RegionMismatch,
    OwnLocationRequired,
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
    SessionExpired,
    InvalidPart,
    SingleRequestTooLarge {
        size: u64,
        limit: u64,
    },
    CopyTooLarge {
        size: u64,
        limit: u64,
    },
    Encoding,
    File,
    MemberKeysMissing,
    CatchUpRequired,
    /// The provider refused to share the library's folder for good, for a
    /// reason only a person can fix; it ends the invite whose grant met it.
    SharingRefused {
        refusal: BridgeSharingRefusal,
    },
    Bug,
}

/// Why sharing the library's folder was refused for good (§5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSharingRefusal {
    /// The provider has no account for the invited address.
    InvalidAddress,
    /// The inviting account's email is unverified.
    EmailUnverified,
    /// The inviting account's team doesn't share outside the team.
    TeamSharingOff,
    /// The inviting account's plan has no sharing.
    PlanWithoutSharing,
    /// The folder holds as many members or invitations as the inviting
    /// account's plan allows.
    TooManyMembers,
}

mirror_enum! {
 BridgeSharingRefusal = coven::SharingRefusal,
 from_core: pub(crate) fn,
 into_core: pub(crate) fn,
 variants: {
InvalidAddress,
EmailUnverified,
TeamSharingOff,
PlanWithoutSharing,
TooManyMembers,
 },
}

impl BridgeSharingRefusal {
    /// The core string that says what went wrong and what to fix.
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::InvalidAddress => "core.invite.refused.invalid_address",
            Self::EmailUnverified => "core.invite.refused.email_unverified",
            Self::TeamSharingOff => "core.invite.refused.team_sharing_off",
            Self::PlanWithoutSharing => "core.invite.refused.plan_without_sharing",
            Self::TooManyMembers => "core.invite.refused.too_many_members",
        }
    }
}

mirror_enum! {
 BridgeStorageFailure = coven::StorageFailure,
 from_core: pub(crate) fn,
 into_core: pub(crate) fn,
 variants: {
ProviderUnavailable,
AccessNotRegistered { required: (crate::BridgeOAuthAccess) },
AccessNotGranted { required: (crate::BridgeOAuthAccess) },
DropboxAppFolderNotShareable,
Network,
Corrupted,
Authentication,
PermissionDenied,
NotFound,
AlreadyExists,
Conflict,
ContainerNotFound,
RegionMismatch,
OwnLocationRequired,
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
SharingRefused(refusal: (crate::BridgeSharingRefusal)),
Bug,
 },
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
        BridgeStorageFailure::Corrupted => "core.error.storage.corrupted",
        BridgeStorageFailure::Authentication => "core.error.category.credentials",
        BridgeStorageFailure::PermissionDenied => "core.error.cloud_setup.permission_denied",
        BridgeStorageFailure::NotFound => "core.error.storage.not_found",
        BridgeStorageFailure::AlreadyExists => "core.error.storage.already_exists",
        BridgeStorageFailure::Conflict => "core.error.storage.conflict",
        BridgeStorageFailure::ContainerNotFound => "core.error.cloud_setup.container_not_found",
        BridgeStorageFailure::RegionMismatch => "core.error.cloud_setup.region_mismatch",
        BridgeStorageFailure::OwnLocationRequired => "core.error.storage.own_location_required",
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
        BridgeStorageFailure::SharingRefused { refusal } => refusal.loc_key(),
        // A bug in coven's storage work, which the person can only report.
        BridgeStorageFailure::Bug => "core.error.category.internal",
    }
}
