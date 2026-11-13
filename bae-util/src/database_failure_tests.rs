use super::*;

/// Coven's own work under way passes when asked again; damage and a broken
/// rule never do, and a load whose cleanup failed too is what failed first.
#[test]
fn coven_failures_keep_what_the_database_met() {
    assert_eq!(coven::DbError::ClaimPending.database_failure(), DatabaseFailure::Busy);
    assert_eq!(coven::DbError::DamagedDatabase.database_failure(), DatabaseFailure::Damaged);
    assert_eq!(coven::DbError::StatementFailed.database_failure(), DatabaseFailure::Refused);
    let cleaned = coven::DbError::StagingCleanup {
        operation: Box::new(coven::DbError::MemberKeysChanging),
        cleanup: Box::new(coven::DbError::StatementFailed),
    };
    assert_eq!(cleaned.database_failure(), DatabaseFailure::Busy);
    let store = coven::CovenError::Database(coven::DbError::DamagedDatabase);
    assert_eq!(store.database_failure(), DatabaseFailure::Damaged);
}

/// A write before the device is registered is refused, a sum that passes
/// u64 and a migration reading across audiences break a rule, and a synced row
/// with no merge record is damage.
#[test]
fn the_refusals_coven_added_are_classed() {
    assert_eq!(coven::DbError::NotRegistered.database_failure(), DatabaseFailure::Refused);
    let sum = coven::DbError::TotalTooLarge { field: "sizes", total: u64::MAX, addend: 1 };
    assert_eq!(sum.database_failure(), DatabaseFailure::Refused);
    let unrecorded = coven::DbError::UnrecordedRow { table: "albums".into() };
    assert_eq!(unrecorded.database_failure(), DatabaseFailure::Damaged);
}

/// A handle that takes no calls until reopened refuses, unless damage is
/// what stopped it.
#[test]
fn a_stopped_handle_refuses_unless_damage_stopped_it() {
    let busy = coven::DbError::StatePersistence(std::sync::Arc::new(coven::DbError::ClaimPending));
    assert_eq!(busy.database_failure(), DatabaseFailure::Refused);
    let damaged =
        coven::DbError::CheckpointFailed(std::sync::Arc::new(coven::DbError::DamagedDatabase));
    assert_eq!(damaged.database_failure(), DatabaseFailure::Damaged);
}
