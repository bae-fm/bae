//! Why this device's last attempt to start sync failed, as
//! `sync_start_failure`'s columns hold it: the failure's class, and for a
//! storage failure coven's reason with what that reason names.

use super::*;
use bae_diagnostics::{UiError, UiErrorCategory, UiErrorClass};
use coven::{DropboxAccess, OAuthAccess, OwnBucketReason, StorageFailure};

/// A failed start's class in `sync_start_failure`'s columns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FailureColumns {
    category: &'static str,
    storage_failure: Option<&'static str>,
    own_bucket_reason: Option<&'static str>,
    required_access: Option<&'static str>,
    size_bytes: Option<i64>,
    limit_bytes: Option<i64>,
}

impl FailureColumns {
    fn plain(category: &'static str) -> Self {
        Self {
            category,
            storage_failure: None,
            own_bucket_reason: None,
            required_access: None,
            size_bytes: None,
            limit_bytes: None,
        }
    }

    /// The columns of `category`. A start fails only in the classes coven's
    /// `SyncError` maps to, or as a panic (`Internal`); any other is a bug
    /// in the caller, refused rather than kept.
    fn of(category: &UiErrorCategory) -> Result<Self, DbError> {
        use UiErrorClass as K;
        if let UiErrorCategory::Storage(failure) = category {
            return Self::storage(*failure);
        }
        let class = category.class();
        match class {
            K::Database
            | K::Config
            | K::Internal
            | K::SyncUpdateRequired
            | K::DeviceResetRequired
            | K::DeviceRemoved
            | K::Membership
            | K::Keyring
            | K::KeyringLocked
            | K::KeychainEntitlementMissing
            | K::SavedRestoreCodesFull
            | K::SavedRestoreCodeTooLarge => Ok(Self::plain(class.name())),
            K::Storage
            | K::Import
            | K::ImportData
            | K::CandidateImportInProgress
            | K::CandidateBeingIdentified
            | K::FilesAlreadyImported
            | K::MetadataTrackCount
            | K::TiedMediums
            | K::ReleaseChangedSinceOpened
            | K::GroupingBlocked
            | K::Export
            | K::Save
            | K::StorageCheck
            | K::S3VersioningUnknown
            | K::S3VersioningUnsupported
            | K::StorageLocationChangeUnsupported
            | K::S3Prefix
            | K::DeviceIdentityMissing
            | K::Credentials
            | K::Network
            | K::InviteJoining
            | K::StoreOwnerAccount
            | K::Renderer => {
                Err(DbError::Message(format!("a sync start can't fail as {category:?}")))
            }
        }
    }

    fn storage(failure: StorageFailure) -> Result<Self, DbError> {
        use StorageFailure as F;
        let mut columns = Self::plain(UiErrorClass::Storage.name());
        let name = match failure {
            F::ProviderUnavailable => "provider_unavailable",
            F::AccessNotRegistered { required } => {
                columns.required_access = Some(access_name(required));
                "access_not_registered"
            }
            F::AccessNotGranted { required } => {
                columns.required_access = Some(access_name(required));
                "access_not_granted"
            }
            F::DropboxAppFolderNotShareable => "dropbox_app_folder_not_shareable",
            F::CatchUpRequired => "catch_up_required",
            F::Network => "network",
            F::Authentication => "authentication",
            F::PermissionDenied => "permission_denied",
            F::NotFound => "not_found",
            F::AlreadyExists => "already_exists",
            F::Conflict => "conflict",
            F::ContainerNotFound => "container_not_found",
            F::RegionMismatch => "region_mismatch",
            F::StoreExists => "store_exists",
            F::ClockSkew => "clock_skew",
            F::OwnBucketRequired { reason } => {
                columns.own_bucket_reason = Some(match reason {
                    OwnBucketReason::Prefix => "prefix",
                    OwnBucketReason::Occupied => "occupied",
                });
                "own_bucket_required"
            }
            F::QuotaExceeded => "quota_exceeded",
            F::RateLimited => "rate_limited",
            F::InvalidConfiguration => "invalid_configuration",
            F::Refused => "refused",
            F::Protocol => "protocol",
            F::InvalidPath => "invalid_path",
            F::InvalidRange => "invalid_range",
            F::NotStoreOwner => "not_store_owner",
            F::InviteeIsStoreOwner => "invitee_is_store_owner",
            F::AccountIdUnavailable => "account_id_unavailable",
            F::InvitationMismatch => "invitation_mismatch",
            F::SessionMismatch => "session_mismatch",
            F::SessionExpired => "session_expired",
            F::SessionIdentityUnknown => "session_identity_unknown",
            F::InvalidPart => "invalid_part",
            F::SingleRequestTooLarge { size, limit } => {
                columns.sizes(size, limit)?;
                "single_request_too_large"
            }
            F::CopyTooLarge { size, limit } => {
                columns.sizes(size, limit)?;
                "copy_too_large"
            }
            F::Encoding => "encoding",
            F::File => "file",
            F::MemberKeysMissing => "member_keys_missing",
        };
        columns.storage_failure = Some(name);
        Ok(columns)
    }

    fn sizes(&mut self, size: u64, limit: u64) -> Result<(), DbError> {
        self.size_bytes = Some(bae_store::query::to_i64(size, "size_bytes")?);
        self.limit_bytes = Some(bae_store::query::to_i64(limit, "limit_bytes")?);
        Ok(())
    }
}

fn access_name(access: OAuthAccess) -> &'static str {
    match access {
        OAuthAccess::Dropbox(DropboxAccess::AppFolder) => "dropbox_app_folder",
        OAuthAccess::Dropbox(DropboxAccess::FullDropbox) => "dropbox_full",
        OAuthAccess::OneDrive => "one_drive",
    }
}

/// A stored value that names no class: the table's CHECKs keep this from
/// happening, so it is a damaged row.
fn unnamed(column: &str, value: &str) -> DbError {
    DbError::unreadable(format!("sync_start_failure.{column}"), value)
}

/// The row's columns as read, before they are named back into a category.
struct StoredColumns {
    category: String,
    storage_failure: Option<String>,
    own_bucket_reason: Option<String>,
    required_access: Option<String>,
    size_bytes: Option<i64>,
    limit_bytes: Option<i64>,
}

impl StoredColumns {
    fn category(&self) -> Result<UiErrorCategory, DbError> {
        let unnamed_class = || unnamed("category", &self.category);
        let class = UiErrorClass::named(&self.category).ok_or_else(unnamed_class)?;
        if class == UiErrorClass::Storage {
            return Ok(UiErrorCategory::Storage(self.storage_failure()?));
        }
        UiErrorCategory::plain(class).ok_or_else(unnamed_class)
    }

    fn storage_failure(&self) -> Result<StorageFailure, DbError> {
        use StorageFailure as F;
        let name =
            self.storage_failure.as_deref().ok_or_else(|| unnamed("storage_failure", "NULL"))?;
        Ok(match name {
            "provider_unavailable" => F::ProviderUnavailable,
            "access_not_registered" => F::AccessNotRegistered { required: self.required_access()? },
            "access_not_granted" => F::AccessNotGranted { required: self.required_access()? },
            "dropbox_app_folder_not_shareable" => F::DropboxAppFolderNotShareable,
            "catch_up_required" => F::CatchUpRequired,
            "network" => F::Network,
            "authentication" => F::Authentication,
            "permission_denied" => F::PermissionDenied,
            "not_found" => F::NotFound,
            "already_exists" => F::AlreadyExists,
            "conflict" => F::Conflict,
            "container_not_found" => F::ContainerNotFound,
            "region_mismatch" => F::RegionMismatch,
            "store_exists" => F::StoreExists,
            "clock_skew" => F::ClockSkew,
            "own_bucket_required" => F::OwnBucketRequired {
                reason: match self.own_bucket_reason.as_deref() {
                    Some("prefix") => OwnBucketReason::Prefix,
                    Some("occupied") => OwnBucketReason::Occupied,
                    other => return Err(unnamed("own_bucket_reason", &format!("{other:?}"))),
                },
            },
            "quota_exceeded" => F::QuotaExceeded,
            "rate_limited" => F::RateLimited,
            "invalid_configuration" => F::InvalidConfiguration,
            "refused" => F::Refused,
            "protocol" => F::Protocol,
            "invalid_path" => F::InvalidPath,
            "invalid_range" => F::InvalidRange,
            "not_store_owner" => F::NotStoreOwner,
            "invitee_is_store_owner" => F::InviteeIsStoreOwner,
            "account_id_unavailable" => F::AccountIdUnavailable,
            "invitation_mismatch" => F::InvitationMismatch,
            "session_mismatch" => F::SessionMismatch,
            "session_expired" => F::SessionExpired,
            "session_identity_unknown" => F::SessionIdentityUnknown,
            "invalid_part" => F::InvalidPart,
            "single_request_too_large" => {
                let (size, limit) = self.sizes()?;
                F::SingleRequestTooLarge { size, limit }
            }
            "copy_too_large" => {
                let (size, limit) = self.sizes()?;
                F::CopyTooLarge { size, limit }
            }
            "encoding" => F::Encoding,
            "file" => F::File,
            "member_keys_missing" => F::MemberKeysMissing,
            other => return Err(unnamed("storage_failure", other)),
        })
    }

    fn required_access(&self) -> Result<OAuthAccess, DbError> {
        Ok(match self.required_access.as_deref() {
            Some("dropbox_app_folder") => OAuthAccess::Dropbox(DropboxAccess::AppFolder),
            Some("dropbox_full") => OAuthAccess::Dropbox(DropboxAccess::FullDropbox),
            Some("one_drive") => OAuthAccess::OneDrive,
            other => return Err(unnamed("required_access", &format!("{other:?}"))),
        })
    }

    fn sizes(&self) -> Result<(u64, u64), DbError> {
        let bytes = |column: &str, value: Option<i64>| {
            value
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| unnamed(column, &format!("{value:?}")))
        };
        Ok((bytes("size_bytes", self.size_bytes)?, bytes("limit_bytes", self.limit_bytes)?))
    }
}

/// Why the last attempt to start sync failed, while that stands.
pub(super) fn start_failure_on(sql: &SqlReadContext<'_>) -> Result<Option<UiError>, DbError> {
    let row = sql
        .query_row(
            "SELECT category, storage_failure, own_bucket_reason, required_access, \
             size_bytes, limit_bytes, detail FROM sync_start_failure",
            [],
            |row| {
                Ok((
                    StoredColumns {
                        category: row.get(0)?,
                        storage_failure: row.get(1)?,
                        own_bucket_reason: row.get(2)?,
                        required_access: row.get(3)?,
                        size_bytes: row.get(4)?,
                        limit_bytes: row.get(5)?,
                    },
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    row.map(|(columns, detail)| Ok(UiError::Diagnostic { category: columns.category()?, detail }))
        .transpose()
}

impl SyncStatus {
    /// Record why starting sync failed, or with `None` that no failed start
    /// stands any more.
    pub async fn record_sync_start_failure(&self, failure: Option<UiError>) -> Result<(), DbError> {
        let row = failure
            .map(|UiError::Diagnostic { category, detail }| {
                FailureColumns::of(&category).map(|columns| (columns, detail))
            })
            .transpose()?;
        self.store
            .call(move |sql| {
                match &row {
                    Some((columns, detail)) => sql.execute(
                        "INSERT INTO sync_start_failure (singleton, category, storage_failure, \
                     own_bucket_reason, required_access, size_bytes, limit_bytes, detail) \
                     VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                     ON CONFLICT (singleton) DO UPDATE SET \
                     category = excluded.category, storage_failure = excluded.storage_failure, \
                     own_bucket_reason = excluded.own_bucket_reason, \
                     required_access = excluded.required_access, \
                     size_bytes = excluded.size_bytes, limit_bytes = excluded.limit_bytes, \
                     detail = excluded.detail",
                        params![
                            columns.category,
                            columns.storage_failure,
                            columns.own_bucket_reason,
                            columns.required_access,
                            columns.size_bytes,
                            columns.limit_bytes,
                            detail
                        ],
                    )?,
                    None => sql.execute("DELETE FROM sync_start_failure", [])?,
                };
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "start_failure_tests.rs"]
mod tests;
