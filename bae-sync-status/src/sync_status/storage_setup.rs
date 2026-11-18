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
    /// How `outcome` reads; an outcome no storage setup ends with, a refused
    /// sharing, is an unreadable row.
    fn of(outcome: Option<coven::OperationOutcome>) -> Result<Self, DbError> {
        use coven::OperationOutcome as O;
        let undone = |detail: String| {
            Self::Undone(UiError::diagnostic(UiErrorCategory::StorageSetupUndone, detail))
        };
        Ok(match outcome {
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
            Some(outcome @ O::SharingRefused { .. }) => {
                return Err(DbError::unreadable(
                    "coven_operations.outcome of a storage setup",
                    outcome,
                ))
            }
        })
    }
}

/// The columns the status query reads of this device's storage setups: whether
/// one runs, then the newest one that ended undone, which it reads only for
/// when none runs.
pub(super) const STORAGE_SETUP_COLUMNS: &str = "EXISTS (SELECT 1 FROM coven_operations AS setup \
     WHERE setup.kind = 'StorageSetup' AND setup.outcome IS NULL) AS setup_running, \
     undone.operation, undone.outcome";

/// The newest storage setup that ended undone, which [`STORAGE_SETUP_COLUMNS`]
/// reads from: coven names how an operation ended in `outcome_kind`.
pub(super) const UNDONE_SETUP_JOIN: &str = "LEFT JOIN (SELECT operation, outcome \
     FROM coven_operations \
     WHERE kind = 'StorageSetup' AND outcome_kind IN ('undone', 'rejected') \
     ORDER BY operation DESC LIMIT 1) AS undone";

/// [`STORAGE_SETUP_COLUMNS`] as read, from column `first` on.
pub(super) struct StorageSetupColumns {
    running: bool,
    undone: Option<(coven::OperationId, Option<coven::OperationOutcome>)>,
}

impl StorageSetupColumns {
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        let undone = row
            .get::<_, Option<i64>>(first + 1)?
            .map(|operation| {
                Ok::<_, rusqlite::Error>((coven::OperationId(operation), row.get(first + 2)?))
            })
            .transpose()?;
        Ok(Self { running: row.get(first)?, undone })
    }

    /// Where this device's storage setup stands: running while any setup
    /// runs, or else the newest setup that ended undone, or `None`.
    pub(super) fn state(self) -> Result<Option<StorageSetupState>, DbError> {
        if self.running {
            return Ok(Some(StorageSetupState::Running));
        }
        let Some((operation, outcome)) = self.undone else {
            return Ok(None);
        };
        match SetupRow::of(outcome)? {
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
                Ok(sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM coven_operations \
                     WHERE kind = 'StorageSetup' AND outcome IS NULL)",
                    [],
                    |row| row.get(0),
                )?)
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
                let outcome = sql
                    .query_row(
                        "SELECT outcome FROM coven_operations \
                         WHERE operation = ?1 AND kind = 'StorageSetup'",
                        [operation.0],
                        |row| row.get::<_, Option<coven::OperationOutcome>>(0),
                    )
                    .optional()?;
                match outcome.map(SetupRow::of).transpose()? {
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
                let ended = sql.query(
                    "SELECT operation, outcome FROM coven_operations \
                     WHERE kind = 'StorageSetup' AND outcome IS NOT NULL",
                    [],
                    |row| {
                        Ok((
                            coven::OperationId(row.get(0)?),
                            row.get::<_, Option<coven::OperationOutcome>>(1)?,
                        ))
                    },
                )?;
                for (operation, outcome) in ended {
                    let acknowledged = match (which, SetupRow::of(outcome)?) {
                        (_, SetupRow::Running) => false,
                        (Ended::All, _) | (Ended::Settled, SetupRow::Settled) => true,
                        (Ended::Settled, SetupRow::Undone(_)) => false,
                    };
                    if acknowledged {
                        sql.acknowledge_operation(operation)?;
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
