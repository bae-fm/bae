//! `sync_start_failure`: a failed start's class in columns, read back as the
//! category the status line words, and refused outside the classes a start
//! can fail in.

use crate::client::*;
use crate::test_fixtures::temp_db;
use bae_diagnostics::{UiError, UiErrorCategory as C};
use coven::{DropboxAccess, OAuthAccess, OwnBucketReason, StorageFailure as F};

fn failure(category: C) -> UiError {
    UiError::Diagnostic { category, detail: "the start failed".to_string() }
}

/// The failed start the store reads back.
async fn read_back(db: &Database) -> Option<UiError> {
    db.read(|sql| crate::client::sync_start_failure::start_failure_on(&sql)).await.unwrap()
}

/// Every class a start can fail in, and every storage reason with each
/// value it names, comes back as written.
#[tokio::test]
async fn every_class_a_start_fails_in_reads_back_as_written() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let storage = [
        F::ProviderUnavailable,
        F::AccessNotRegistered { required: OAuthAccess::Dropbox(DropboxAccess::AppFolder) },
        F::AccessNotGranted { required: OAuthAccess::Dropbox(DropboxAccess::FullDropbox) },
        F::AccessNotGranted { required: OAuthAccess::OneDrive },
        F::DropboxAppFolderNotShareable,
        F::CatchUpRequired,
        F::Network,
        F::Authentication,
        F::PermissionDenied,
        F::NotFound,
        F::AlreadyExists,
        F::Conflict,
        F::ContainerNotFound,
        F::RegionMismatch,
        F::StoreExists,
        F::ClockSkew,
        F::OwnBucketRequired { reason: OwnBucketReason::Prefix },
        F::OwnBucketRequired { reason: OwnBucketReason::Occupied },
        F::QuotaExceeded,
        F::RateLimited,
        F::InvalidConfiguration,
        F::Refused,
        F::Protocol,
        F::InvalidPath,
        F::InvalidRange,
        F::NotStoreOwner,
        F::InviteeIsStoreOwner,
        F::AccountIdUnavailable,
        F::InvitationMismatch,
        F::SessionMismatch,
        F::SessionExpired,
        F::SessionIdentityUnknown,
        F::InvalidPart,
        F::SingleRequestTooLarge { size: 6 << 30, limit: 5 << 30 },
        F::CopyTooLarge { size: 6 << 30, limit: 5 << 30 },
        F::Encoding,
        F::File,
        F::MemberKeysMissing,
    ];
    let categories = [
        C::Database,
        C::Config,
        C::Internal,
        C::SyncUpdateRequired,
        C::DeviceResetRequired,
        C::DeviceRemoved,
        C::Membership,
        C::Keyring,
        C::KeyringLocked,
        C::KeychainEntitlementMissing,
        C::SavedRestoreCodesFull,
        C::SavedRestoreCodeTooLarge,
    ]
    .into_iter()
    .chain(storage.into_iter().map(C::Storage));
    for category in categories {
        db.record_sync_start_failure(Some(failure(category.clone()))).await.unwrap();
        assert_eq!(read_back(&db).await, Some(failure(category)));
    }
    db.record_sync_start_failure(None).await.unwrap();
    assert_eq!(read_back(&db).await, None);
}

/// A class no start fails in is a caller's bug: refused, and the failure
/// already standing is left as it was.
#[tokio::test]
async fn a_class_no_start_fails_in_is_refused() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    db.record_sync_start_failure(Some(failure(C::KeyringLocked))).await.unwrap();
    let refused = db.record_sync_start_failure(Some(failure(C::ReleaseChangedSinceOpened))).await;
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(read_back(&db).await, Some(failure(C::KeyringLocked)));
}

/// The table's CHECKs keep each column to the reason that uses it.
#[tokio::test]
async fn a_row_whose_columns_disagree_is_refused() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    for columns in [
        // A storage failure with no reason.
        "'storage', NULL, NULL, NULL, NULL, NULL",
        // A reason on a class that isn't storage.
        "'keyring', 'network', NULL, NULL, NULL, NULL",
        // An access failure naming no access.
        "'storage', 'access_not_granted', NULL, NULL, NULL, NULL",
        // An access on a reason that names none.
        "'storage', 'network', NULL, 'one_drive', NULL, NULL",
        // A required bucket with no reason for it.
        "'storage', 'own_bucket_required', NULL, NULL, NULL, NULL",
        // A size without its limit.
        "'storage', 'copy_too_large', NULL, NULL, 10, NULL",
        // A size on a class that isn't storage.
        "'internal', NULL, NULL, NULL, 10, 5",
    ] {
        let sql = format!(
            "INSERT INTO sync_start_failure (singleton, category, storage_failure, \
             own_bucket_reason, required_access, size_bytes, limit_bytes, detail) \
             VALUES (1, {columns}, 'detail')"
        );
        let inserted = db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from)).await;
        assert!(inserted.is_err(), "{columns} was kept");
    }
}
