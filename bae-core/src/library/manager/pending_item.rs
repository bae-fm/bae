//! What each of coven's pending records is about, named so a person
//! recognizes it, with why it waits and what they can do about it.

use super::*;
use crate::db::{FileRow, PendingRow};

/// What a pending record is about. A file is named by the row on this device
/// whose file it is; every other subject by what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingSubjectName {
    /// A release file, by its name and its release's album.
    ReleaseFile {
        file_name: String,
        album_title: String,
    },
    /// A release's cover, by its release's album.
    Cover { album_title: String },
    /// An artist's image.
    ArtistImage,
    /// A person's profile picture.
    Avatar,
    /// A release file or cover whose row or release is gone.
    DeletedReleaseFile,
    /// A file no row on this device names.
    File,
    /// Edits made on this device, waiting to be sent.
    LocalChanges,
    /// Changes in the library's storage, waiting to apply here.
    LibraryChanges,
    /// A copy of an encryption key for one person.
    KeyCopy,
    /// A saved copy of the library's data in storage.
    Snapshot,
    /// Objects in storage nothing refers to any more, waiting to be deleted.
    Retention,
    /// The data every member of the library shares.
    LibraryData,
    /// Data shared only within a circle of people.
    CircleData,
    /// A request to join the library.
    JoinRequest,
    /// This device's connection to the library's storage.
    Connection,
    /// One of coven's operations on the library, such as inviting a person.
    Operation(coven::OperationKind),
}

/// Why a record waits, and what the person can do about it.
#[derive(Debug, Clone)]
pub struct PendingWait {
    /// The record's first unmet condition, kept as coven typed it.
    pub reason: coven::PendingReason,
    /// The error behind `reason`, with every cause below it, while this
    /// process holds it; a record read after a restart has none. coven holds
    /// it for every record but the connection's, whose error the sync status
    /// carries instead (`SyncStatusSnapshot::name_connection_cause`).
    pub cause: Option<Arc<dyn std::error::Error + Send + Sync>>,
    pub action: PendingAction,
}

/// Two waits are the same when they wait for the same reason, offer the same
/// action, and name the same error behind it: a surface shows all three.
impl PartialEq for PendingWait {
    fn eq(&self, other: &Self) -> bool {
        let cause_text = |wait: &Self| {
            wait.cause.as_ref().map(|cause| {
                let cause: &(dyn std::error::Error + 'static) = cause.as_ref();
                bae_util::error_text::CauseChain(cause).to_string()
            })
        };
        self.reason == other.reason
            && self.action == other.action
            && cause_text(self) == cause_text(other)
    }
}

impl PendingWait {
    pub(crate) fn of(record: &coven::PendingRecord) -> Self {
        Self {
            reason: record.reason.clone(),
            cause: record.cause.clone(),
            action: pending_action(record),
        }
    }

    /// Whether coven retries this record's failure by itself, so a retry
    /// now can clear it: the record offers only waiting, and its reason is a
    /// failure, not a wait on other work such as another device's upload.
    pub(crate) fn retries_by_itself(&self) -> bool {
        self.action == PendingAction::Wait && reports_failure(&self.reason)
    }
}

/// Whether the reason reports a failure; a cleanup reports its own
/// condition, as coven retries it.
fn reports_failure(reason: &coven::PendingReason) -> bool {
    use coven::PendingReason as R;
    match reason {
        R::Cleanup { cleanup, .. } => reports_failure(cleanup),
        R::Storage { .. } | R::Failed(_) => true,
        R::Missing { .. }
        | R::Waits(_)
        | R::KeyUnavailable { .. }
        | R::UpdateRequired(_)
        | R::NoStorage
        | R::Refused(_)
        | R::WritesRefused { .. }
        | R::ProviderPending { .. }
        | R::PendingOwner { .. }
        | R::AccountInUse { .. }
        | R::AccessRemains { .. }
        | R::FileUnavailable(_)
        | R::Rejected(_)
        | R::Paused
        | R::Removed
        | R::MemberKeysReplaced { .. } => false,
    }
}

/// One thing coven is waiting on, as the pending list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingItem {
    pub subject: PendingSubjectName,
    pub wait: PendingWait,
}

impl PendingItem {
    /// A record named before any library is open, as while restoring or
    /// joining: every subject but a file names itself, and a file is named
    /// only by its row, which no library holds yet.
    pub fn before_open(record: coven::PendingRecord) -> Self {
        Self::named(record, None)
    }

    /// A record as the library lists it: a file's named by its row.
    fn listed(row: PendingRow, causes: &coven::PendingCauses) -> Self {
        let mut record = row.record;
        record.cause = causes.cause(&record);
        Self::named(record, row.file_row)
    }

    fn named(record: coven::PendingRecord, file_row: Option<FileRow>) -> Self {
        use coven::PendingSubject as S;
        let wait = PendingWait::of(&record);
        let subject = match record.subject {
            S::Write(_) => PendingSubjectName::LocalChanges,
            S::Slot(_) => PendingSubjectName::LibraryChanges,
            S::KeyCopy { .. } => PendingSubjectName::KeyCopy,
            S::Snapshot(_) => PendingSubjectName::Snapshot,
            S::File { .. } => file_row.map_or(PendingSubjectName::File, PendingSubjectName::from),
            S::Operation { kind, .. } => PendingSubjectName::Operation(kind),
            S::Retention { .. } => PendingSubjectName::Retention,
            S::Audience(coven::Audience::Store) => PendingSubjectName::LibraryData,
            S::Audience(coven::Audience::Circle(_)) => PendingSubjectName::CircleData,
            S::JoinRequest(_) => PendingSubjectName::JoinRequest,
            S::Connection => PendingSubjectName::Connection,
        };
        Self { subject, wait }
    }
}

/// A file named by the row it is the file of.
impl From<FileRow> for PendingSubjectName {
    fn from(row: FileRow) -> Self {
        match row {
            FileRow::ReleaseFile {
                file_name,
                album_title,
            } => Self::ReleaseFile {
                file_name,
                album_title,
            },
            FileRow::Cover { album_title } => Self::Cover { album_title },
            FileRow::DeletedReleaseFile => Self::DeletedReleaseFile,
            FileRow::ArtistImage => Self::ArtistImage,
            FileRow::Avatar => Self::Avatar,
        }
    }
}

/// Keeps the sync status's pending list current: coven's pending records,
/// each named by its row, with the error behind it while this process holds
/// one. The records and the errors come apart: a retry that fails for the
/// same reason holds a new error under an unchanged record.
pub(super) struct PendingFollower {
    manager: LibraryManager,
    rows: coven::LiveQuery<Vec<PendingRow>>,
    causes: tokio::sync::watch::Receiver<coven::PendingCauses>,
}

impl PendingFollower {
    pub(super) fn new(manager: LibraryManager) -> Self {
        Self {
            rows: manager.database.subscribe_pending_rows(),
            causes: manager.database.subscribe_pending_causes(),
            manager,
        }
    }

    pub(super) async fn run(mut self) {
        let mut read: Option<Result<Vec<PendingRow>, crate::ui::UiError>> = None;
        loop {
            tokio::select! {
                Some(rows) = self.rows.next() => {
                    read = Some(rows.map_err(|error| {
                        warn!(%error, "could not read pending sync records");
                        let error = LibraryError::from(error);
                        crate::ui::UiError::diagnostic(error.category(), error)
                    }));
                }
                Ok(()) = self.causes.changed() => {}
                else => return,
            }
            // Errors that change before the first records are read wait for
            // them.
            let Some(read) = &read else { continue };
            let causes = self.causes.borrow_and_update();
            let items = read.clone().map(|rows| {
                rows.into_iter()
                    .map(|row| PendingItem::listed(row, &causes))
                    .collect()
            });
            drop(causes);
            self.manager.sync_status.set_pending(items);
        }
    }
}

#[cfg(test)]
#[path = "pending_item_tests.rs"]
mod tests;
