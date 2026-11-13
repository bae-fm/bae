//! What a failed database call met, as far as making the same call again
//! goes: the database was busy, its file is damaged, or it refused the call.
//! The answer is read from the typed failure where it is still whole, never
//! from its text; bae-diagnostics turns it into the class a person is shown.
//! Coven's own variants are read here; SQLite's error codes are read in
//! bae-store, the one owner of SQLite, for the failures its calls return.

use crate::task_failure::ReportedBug;

/// What a failed database call met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseFailure {
    /// The database couldn't take the call when it came: another connection
    /// held it past SQLite's wait, the store is open in another process, the
    /// call waits on coven's own work under way (a claim settling, a request
    /// to a provider, an audience reloading, member keys changing), or what
    /// it read changed under it. Made again, the same call can pass.
    Busy,
    /// The database's file is damaged (SQLite found a page it can't read, or
    /// a file that isn't a database), or a stored value is one nothing here
    /// writes. No call passes until this device's copy is replaced.
    Damaged,
    /// The database refused the call: it breaks a rule of coven's or the
    /// schema's, the handle takes no calls until the store is reopened, the
    /// disk under it is full, read-only or failing, or the cause names none
    /// of these. Made again unchanged, the call fails the same way.
    Refused,
}

/// A failed database call: whether it holds a bug already reported, and
/// otherwise what the database met.
pub trait FailedDatabaseCall: ReportedBug {
    fn database_failure(&self) -> DatabaseFailure;
}

/// Every variant is matched by name, so one coven adds is classed here.
#[deny(clippy::wildcard_enum_match_arm)]
fn coven_database_failure(error: &coven::DbError) -> DatabaseFailure {
    use coven::DbError as E;
    use DatabaseFailure::{Busy, Damaged, Refused};
    match error {
        // Coven turns SQLite's damage into `DamagedDatabase`. Its busy code
        // is SQLite's own, which bae-store reads for the calls it makes
        // (coven's store errors reach it as bae-store's `Sqlite`); one that
        // stays inside coven's error refuses here.
        E::Sqlite(_) => Refused,
        // A synced row with no merge record: the database is damaged.
        E::DamagedDatabase | E::UnrecordedRow { .. } => Damaged,
        // Coven's own work the call waits on, which ends by itself.
        E::ClaimPending
        | E::ProviderRequestRunning
        | E::ClaimUnsettled
        | E::WriteWaiting(_)
        | E::OperationRunning(_)
        | E::MemberKeysChanging
        | E::AudienceReloading(_)
        | E::RaiseWaiting
        | E::CopyPathTaken { .. } => Busy,
        // What the call read changed under it; made again, it reads what
        // stands now.
        E::OperationChanged(_)
        | E::CaptureChanged
        | E::PinChanged { .. }
        | E::NoLongerStored { .. } => Busy,
        E::Lock(error) => lock_failure(error),
        // The handle takes no call until reopened. Damage under it is the
        // damage; anything else stays refused.
        E::StatePersistence(cause) | E::CheckpointFailed(cause) | E::StatisticsFailed(cause) => {
            match coven_database_failure(cause) {
                Damaged => Damaged,
                Busy | Refused => Refused,
            }
        }
        // What failed first is what the call met; the failed rollback or
        // cleanup after it rides in the detail.
        E::Rollback { operation, .. } | E::StagingCleanup { operation, .. } => {
            coven_database_failure(operation)
        }
        E::FileCleanup { write: Err(write), .. } => coven_database_failure(write),
        E::FileCleanup { write: Ok(()), .. } => Refused,
        E::NoClaim
        | E::ClaimNumberExhausted
        | E::WriteNumberExhausted
        | E::InvalidClaimCoverage
        | E::SequenceGap { .. }
        | E::StateEncoding(_)
        | E::RollbackFailed
        | E::WallTimer(_)
        | E::UpkeepThread(_)
        | E::ClaimRead(_)
        | E::StoreClosed
        | E::Shutdown(_)
        | E::Bug(_)
        | E::StoreFailed(_)
        | E::Invariant { .. }
        | E::StoreSettingsMismatch { .. }
        | E::Schema(_)
        | E::PreparedElsewhere
        | E::DeviceOnlyUnreferenced { .. }
        | E::StepFailed
        | E::StatementFailed
        | E::InternalTable { .. }
        | E::TriggerTarget { .. }
        | E::SyncedTableInBulkWrite { .. }
        | E::TransactionEnded
        | E::StatementForbidden { .. }
        | E::WalUnavailable { .. }
        | E::SqliteSettingOutOfRange { .. }
        | E::ReservedCollation { .. }
        | E::DuplicateCollation { .. }
        | E::ReservedFunction { .. }
        | E::DuplicateFunction { .. }
        | E::ConstantFunction { .. }
        | E::VersionChangeRefused { .. }
        | E::VersionNotRebuilt { .. }
        | E::ClockOutOfRange
        | E::InputBudgetExceeded { .. }
        | E::TooLarge { .. }
        | E::TotalTooLarge { .. }
        | E::NotRegistered
        | E::MigrationReadsAcross { .. }
        | E::RebuildHeader { .. }
        | E::ReferenceAudience { .. }
        | E::DeletedCircle(_)
        | E::NotInCircle(_)
        | E::NotOwner { .. }
        | E::InvalidForeignKeyEffect { .. }
        | E::KeyNotUuid { .. }
        | E::InvalidAudience { .. }
        | E::InvalidWrite { .. }
        | E::MissingWriteSources(_)
        | E::WriteFormat(_)
        | E::MigrationKindMismatch { .. }
        | E::MigrationReference { .. }
        | E::MigrationDuplicateColumn { .. }
        | E::MigrationColumnMissing { .. }
        | E::MigrationPrimaryKeyMissing { .. }
        | E::MigrationKeyArity { .. }
        | E::MigrationTableMissing { .. }
        | E::MigrationReferenceCollision { .. }
        | E::DismissedRowChanged { .. }
        | E::FileWriteOnce { .. }
        | E::UserFileMissing { .. }
        | E::UserFileChanged { .. }
        | E::UndeclaredNamespace { .. }
        | E::NamespaceKind { .. }
        | E::FileNameReused { .. }
        | E::FileUnreferenced { .. }
        | E::FileUnknown { .. }
        | E::FileAbsent { .. }
        | E::NotThisDevice { .. }
        | E::NotDeviceOnlyHere { .. }
        | E::FileInMove { .. }
        | E::BoundFileTooLarge { .. }
        | E::DeviceOnly { .. }
        | E::MoveUnknown { .. }
        | E::MoveFinished { .. }
        | E::MoveBound { .. }
        | E::NotInCloud { .. }
        | E::Disk(_)
        | E::FileKey(_)
        | E::OperationUnknown(_)
        | E::Snapshot(_)
        | E::Closing { .. } => Refused,
    }
}

/// The store's lock: held by another process is busy, which ends when that
/// process closes it; any other refusal names a call to run or a fault.
#[deny(clippy::wildcard_enum_match_arm)]
fn lock_failure(error: &coven::StoreLockError) -> DatabaseFailure {
    use coven::StoreLockError as E;
    match error {
        E::AlreadyOpen(_) => DatabaseFailure::Busy,
        E::BootstrapPending(_)
        | E::ReplacementUnfinished { .. }
        | E::DeletionUnfinished(_)
        | E::WrongDirectory(_)
        | E::File(_)
        | E::Bug(_) => DatabaseFailure::Refused,
    }
}

impl FailedDatabaseCall for coven::DbError {
    fn database_failure(&self) -> DatabaseFailure {
        coven_database_failure(self)
    }
}

/// A store call's failure: the database's, the store's lock, or what failed
/// first under a failed rollback or cleanup. A sync step inside the call
/// that failed in the database is that failure; any other refuses.
impl FailedDatabaseCall for coven::CovenError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn database_failure(&self) -> DatabaseFailure {
        use coven::CovenError as E;
        match self {
            E::Database(error) | E::Sync(coven::SyncError::Database(error)) => {
                coven_database_failure(error)
            }
            E::Lock(error) => lock_failure(error),
            E::Rollback { operation: failed, .. } | E::FileCleanup { write: failed, .. } => {
                failed.database_failure()
            }
            E::Key(_)
            | E::Sync(_)
            | E::Migration(_)
            | E::CovenMigration(_)
            | E::Settings(_)
            | E::File(_)
            | E::MissingConfiguration { .. }
            | E::DeviceKeyMissing { .. }
            | E::MemberKeysMissing { .. }
            | E::DeviceLocked { .. }
            | E::Shutdown(_)
            | E::Bug(_)
            | E::NetworkWatch(_) => DatabaseFailure::Refused,
        }
    }
}

#[cfg(test)]
#[path = "database_failure_tests.rs"]
mod tests;
