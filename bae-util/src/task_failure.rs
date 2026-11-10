//! Work run apart from its caller — as its own task, or as one message of a
//! long-lived loop — whose panic comes back to that caller as a value instead
//! of an unwind: coven's, which bae and coven share. Work on the blocking pool
//! runs under bae-diagnostics' `Diagnostics::blocking`, which reports its
//! panic even once its caller stops waiting.
//!
//! A panic turned into an `Err` at the nearest join lets the caller's own
//! failure path run: the rollback, the cleanup and the "this import failed"
//! record bae writes on `Err`, which an unwind would skip. The panic hook
//! reports the panic as it begins (bae-diagnostics' `report_panics`), and the
//! caller hands the failure to bae-diagnostics' `Diagnostics::bug`, which
//! reports only a bug code found without a panic; this crate has no bug
//! reporter of its own.
//!
//! [`ReportedBug`] says whether a failure holds a bug that has already reached
//! the crash reporter, so whoever meets the failure later doesn't report it
//! again.

pub use coven::task::CarriesBug;
pub use coven::task::{caught, caught_sync, joined, panic_message, Bug, BugKind, TaskFailed};

/// A failure holding a bug that has already reached the crash reporter, so
/// whoever meets the failure later shows it and doesn't report it again.
///
/// Two kinds of bug come so. One a coven call carries back (`CarriesBug`):
/// the owner that awaited the call reported it as it arrived. And an earlier
/// bug coven repeats to later calls: `DbError::StoreFailed` for a store an
/// earlier bug failed, `SyncError::WorkerFailed` for a worker's, which
/// coven's bug sink heard when it happened.
pub trait ReportedBug {
    /// The bug this failure holds, carried or repeated, if any.
    fn reported_bug(&self) -> Option<&Bug>;
}

impl ReportedBug for coven::DbError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_database_bug(self))
    }
}

impl ReportedBug for coven::SyncError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_sync_bug(self))
    }
}

impl ReportedBug for coven::CovenError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_coven_bug(self))
    }
}

impl ReportedBug for coven::FileReadError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_file_read_bug(self))
    }
}

impl ReportedBug for coven::DeviceOnlyError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_device_only_bug(self))
    }
}

impl ReportedBug for coven::StoreCreationError {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug().or_else(|| repeated_creation_bug(self))
    }
}

impl ReportedBug for std::io::Error {
    fn reported_bug(&self) -> Option<&Bug> {
        self.bug()
    }
}

/// The earlier bug a failed store repeats in `error`, itself or through the
/// failure it keeps.
fn repeated_database_bug(error: &coven::DbError) -> Option<&Bug> {
    use coven::DbError as E;
    match error {
        E::StoreFailed(bug) => Some(bug),
        E::Rollback { operation, .. } => repeated_database_bug(operation),
        E::StagingCleanup { operation, cleanup } => {
            repeated_database_bug(operation).or_else(|| repeated_database_bug(cleanup))
        }
        E::FileCleanup { write, failures } => write
            .as_ref()
            .err()
            .and_then(|write| repeated_database_bug(write))
            .or_else(|| failures.iter().find_map(repeated_database_bug)),
        E::Closing { failures } => failures.iter().find_map(repeated_database_bug),
        _ => None,
    }
}

/// The earlier bug a failed worker or store repeats in `error`, itself or
/// through the failure it keeps.
fn repeated_sync_bug(error: &coven::SyncError) -> Option<&Bug> {
    use coven::SyncError as E;
    match error {
        E::WorkerFailed(bug) => Some(bug),
        E::Database(error) => repeated_database_bug(error),
        E::Cleanup { operation, cleanup } => {
            repeated_sync_bug(operation).or_else(|| repeated_sync_bug(cleanup))
        }
        // The cause `Failed` shares is the error itself.
        E::Failed { source, .. } => source.downcast_ref::<E>().and_then(repeated_sync_bug),
        _ => None,
    }
}

fn repeated_coven_bug(error: &coven::CovenError) -> Option<&Bug> {
    use coven::CovenError as E;
    match error {
        E::Database(error) => repeated_database_bug(error),
        E::Sync(error) => repeated_sync_bug(error),
        E::Rollback { operation, .. } => repeated_coven_bug(operation),
        E::FileCleanup { write, failures } => {
            repeated_coven_bug(write).or_else(|| failures.iter().find_map(repeated_database_bug))
        }
        _ => None,
    }
}

fn repeated_file_read_bug(error: &coven::FileReadError) -> Option<&Bug> {
    match error {
        coven::FileReadError::Database(error) => repeated_database_bug(error),
        coven::FileReadError::Sync(error) => repeated_sync_bug(error),
        _ => None,
    }
}

fn repeated_device_only_bug(error: &coven::DeviceOnlyError) -> Option<&Bug> {
    use coven::DeviceOnlyError as E;
    match error {
        E::Database(error) => repeated_database_bug(error),
        E::Download { error, .. } => repeated_file_read_bug(error),
        E::Cleanup { error, .. } => repeated_device_only_bug(error),
        _ => None,
    }
}

fn repeated_creation_bug(error: &coven::StoreCreationError) -> Option<&Bug> {
    use coven::StoreCreationError as E;
    match error {
        E::Store(error) => repeated_coven_bug(error),
        E::Sync(error) => repeated_sync_bug(error),
        E::Cleanup { operation, .. } => repeated_creation_bug(operation),
        E::PublishedButNotSynced { source, .. } => repeated_creation_bug(source),
        _ => None,
    }
}
