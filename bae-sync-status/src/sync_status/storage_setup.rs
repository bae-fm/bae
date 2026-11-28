//! This device's storage setups as coven's `coven_operations` lists them
//! (coven §11.6): one running, and one that ended undone, which the sync
//! status shows until the person dismisses it. A setup's own call also hears
//! how it ended, so an undone setup it returned an error for shows the same
//! error; a call that begins acknowledges every
//! setup that ended before it, since the new attempt supersedes what they
//! said. One that finished, or that the person cancelled, with nobody to
//! hear it says nothing the sync state doesn't, and opening the library
//! acknowledges it.
//!
//! The list shows whether a setup runs, not whether it passed its point of
//! no return. Opening undoes every setup cut before it, so a setup running
//! when the library opens is past it, and only sync's connection finishes
//! it.

use super::fragments::StorageSetup;
use super::*;
use bae_diagnostics::UiErrorCategory;

/// Where this device's storage setup stands, as the sync status shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageSetupState {
    /// A setup runs: its call's, or, from when the library opened, one past
    /// its point of no return, which sync's connection finishes with the
    /// credentials the setup kept.
    Running,
    /// It ended undone, `why`: acknowledging `operation` dismisses it.
    Undone { operation: coven::OperationId, why: UiError },
}

/// Why the person's dismissal of an undone storage setup was refused.
#[derive(Debug, thiserror::Error)]
pub enum SetupDismissError {
    /// The operation is no storage setup that ended undone: it runs, it
    /// settled, it was dismissed already, or it is no setup. Nothing was
    /// acknowledged.
    #[error("operation {0:?} is no undone storage setup")]
    NotUndone(coven::OperationId),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A bug the store's write carried back, which the store reported as it
/// returned; a refusal holds none.
impl bae_util::task_failure::ReportedBug for SetupDismissError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::NotUndone(_) => None,
        }
    }
}

/// How a storage setup's row in `coven_operations` reads.
#[derive(Debug, Clone, PartialEq)]
enum SetupRow {
    Running,
    /// Finished; cancelled, which only the person does; or ended by this
    /// device's stop, which the sync state shows.
    Settled,
    Undone(UiError),
}

impl SetupRow {
    /// How `outcome` reads.
    fn of(outcome: Option<coven::StorageSetupOutcome>) -> Self {
        use coven::StorageSetupOutcome as O;
        let undone = |detail: String| {
            Self::Undone(UiError::diagnostic(UiErrorCategory::StorageSetupUndone, detail))
        };
        match outcome {
            None => Self::Running,
            Some(O::Landed(_) | O::Satisfied | O::Stopped(_) | O::Cancelled) => Self::Settled,
            Some(O::Undone(coven::UndoCause::Interrupted)) => {
                undone("bae stopped before the setup could finish".into())
            }
            Some(O::Undone(coven::UndoCause::Failed { error })) => undone(error),
            // The account that holds the storage keeps its member: setting
            // up with another account is refused (§20.9).
            Some(O::Rejected(coven::OperationRejection::StoreOwner)) => {
                Self::Undone(UiError::diagnostic(
                    UiErrorCategory::StoreOwnerAccount,
                    "the storage's owner must set it up with the account that holds it",
                ))
            }
            Some(O::Rejected(rejection)) => {
                undone(format!("the library refused the setup: {rejection:?}"))
            }
        }
    }
}

coven::read!(
    writer LibraryStorageSetup,
    r#"
    SELECT current.*
    FROM @crate::sync_status::fragments::StorageSetup AS current
"#
);

// How the storage setup `operation` ended; no row when it is no setup or it
// left the list.
coven::read!(
    writer SetupOutcome(operation: i64) {
        outcome: Option<coven::StorageSetupOutcome>,
    },
    r#"
    SELECT setup.outcome
    FROM coven_operations AS setup
    WHERE setup.operation = :operation AND setup.kind = 'StorageSetup'
"#
);

// Every storage setup that ended, with how.
coven::read!(
    writer EndedSetups {
        outcome: Option<coven::StorageSetupOutcome>,
    },
    r#"
    SELECT setup.operation, setup.outcome
    FROM coven_operations AS setup
    WHERE setup.kind = 'StorageSetup' AND setup.outcome IS NOT NULL
"#
);

impl StorageSetup {
    /// Where this device's storage setup stands: running while any setup
    /// runs, or else the newest setup that ended undone, or `None`.
    pub(super) fn state(self) -> Result<Option<StorageSetupState>, DbError> {
        if self.setup_running {
            return Ok(Some(StorageSetupState::Running));
        }
        let Some(undone) = self.undone else {
            return Ok(None);
        };
        let operation = coven::OperationId(undone.operation);
        match SetupRow::of(undone.outcome) {
            SetupRow::Undone(why) => Ok(Some(StorageSetupState::Undone { operation, why })),
            row @ (SetupRow::Running | SetupRow::Settled) => Err(DbError::unreadable(
                "coven_operations.outcome of the newest undone storage setup",
                row,
            )),
        }
    }
}

/// Which ended setups a write acknowledges.
#[derive(Debug, Clone, Copy)]
enum Ended {
    /// Every one: a setup call begins, and what they said is superseded.
    All,
    /// Those whose end tells the person nothing.
    Settled,
}

impl SyncStatus {
    /// Whether a storage setup runs on this device. Read as the library
    /// opens, before any setup call, it is one past its point of no return,
    /// which only sync's connection finishes.
    pub async fn storage_setup_running(&self) -> Result<bool, DbError> {
        self.store
            .read(|sql| {
                let row = bae_store::statement_row(&sql, &LibraryStorageSetupParams {})?;
                Ok(row.current.setup_running)
            })
            .await
    }

    /// A storage setup call begins: every ended setup leaves
    /// `coven_operations`, an undone one the person was shown among them,
    /// since the new attempt is the newest word on setup.
    pub async fn acknowledge_storage_setups(&self) -> Result<(), DbError> {
        self.acknowledge_ended(Ended::All).await
    }

    /// The library opened: every setup that finished, that the person
    /// cancelled, or that this device's stop ended, leaves
    /// `coven_operations`. None tells the person anything. An undone one
    /// stays for the status to show.
    pub async fn acknowledge_settled_storage_setups(&self) -> Result<(), DbError> {
        self.acknowledge_ended(Ended::Settled).await
    }

    /// The person dismissed `operation`, a storage setup that ended undone.
    /// Anything else is refused.
    pub async fn acknowledge_undone_storage_setup(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), SetupDismissError> {
        let acknowledged = self
            .store
            .call(move |sql| {
                let outcome = bae_store::statement_optional(
                    sql,
                    &SetupOutcomeParams { operation: operation.0 },
                )?;
                match outcome.map(|row| SetupRow::of(row.outcome)) {
                    Some(SetupRow::Undone(_)) => {
                        sql.acknowledge_operation(operation)?;
                        Ok(true)
                    }
                    Some(SetupRow::Running | SetupRow::Settled) | None => Ok(false),
                }
            })
            .await?;
        match acknowledged {
            true => Ok(()),
            false => Err(SetupDismissError::NotUndone(operation)),
        }
    }

    async fn acknowledge_ended(&self, which: Ended) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                let ended = bae_store::statement_rows(sql, &EndedSetupsParams {})?;
                for setup in ended {
                    let acknowledged = match (which, SetupRow::of(setup.outcome)) {
                        (_, SetupRow::Running) => false,
                        (Ended::All, _) | (Ended::Settled, SetupRow::Settled) => true,
                        (Ended::Settled, SetupRow::Undone(_)) => false,
                    };
                    if acknowledged {
                        sql.acknowledge_operation(coven::OperationId(setup.operation))?;
                    }
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "storage_setup_tests.rs"]
mod tests;
