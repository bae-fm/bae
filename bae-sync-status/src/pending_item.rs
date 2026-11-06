//! What each of coven's pending records is about, named so a person
//! recognizes it, with why it waits and what they can do about it.

use crate::sync_status::{pending_action, PendingAction};
use crate::{FileRow, PendingRow};

/// What a pending record is about. A file is named by the row on this device
/// whose file it is; every other subject by what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingSubjectName {
    /// A release file, by its name and its release's album.
    ReleaseFile { file_name: String, album_title: String },
    /// A release's cover, by its release's album.
    Cover { album_title: String },
    /// An artist's image.
    ArtistImage,
    /// A person's profile picture.
    Avatar,
    /// A release file or cover no row of the library names any more.
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
    /// This device's connection to the library's storage.
    Connection,
    /// One of coven's operations on the library, such as inviting a person.
    Operation(coven::OperationKind),
}

/// Why a record waits, and what the person can do about it. Two waits are
/// the same when they wait for the same reason, offer the same action, and
/// name the same error behind it: a surface shows all three.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingWait {
    /// The record's first unmet condition, kept as coven typed it.
    pub reason: coven::PendingReason,
    /// The text of the error behind `reason`, with every cause below it,
    /// when the run that met it kept it; none after the store reopens.
    /// coven keeps it beside every record but the connection's, whose error
    /// the sync status keeps instead (`SyncStatusSnapshot::read`).
    pub cause: Option<String>,
    pub action: PendingAction,
}

impl PendingWait {
    /// The wait of `record`, about a file of the release `release_id` when
    /// it names one.
    pub fn of(record: &coven::PendingRecord, release_id: Option<&str>) -> Self {
        Self {
            reason: record.reason.clone(),
            cause: record.cause.clone(),
            action: pending_action(record, release_id),
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
        | R::AccountInUse { .. }
        | R::AccessRemains { .. }
        | R::FileUnavailable(_)
        | R::Original { .. }
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
    pub fn listed(row: PendingRow) -> Self {
        Self::named(row.record, row.file_row)
    }

    fn named(record: coven::PendingRecord, file_row: Option<FileRow>) -> Self {
        use coven::PendingSubject as S;
        let release_id = match &file_row {
            Some(FileRow::ReleaseFile { release_id, .. }) => Some(release_id.as_str()),
            _ => None,
        };
        let wait = PendingWait::of(&record, release_id);
        let subject = match record.subject {
            S::Write(_) => PendingSubjectName::LocalChanges,
            S::Slot(_) => PendingSubjectName::LibraryChanges,
            S::KeyCopy { .. } => PendingSubjectName::KeyCopy,
            S::Snapshot(_) => PendingSubjectName::Snapshot,
            S::Upload { .. } | S::Download { .. } | S::Keep { .. } | S::Move { .. } => {
                file_row.map_or(PendingSubjectName::File, PendingSubjectName::from)
            }
            S::Operation { kind, .. } => PendingSubjectName::Operation(kind),
            S::Retention { .. } => PendingSubjectName::Retention,
            S::Audience(coven::Audience::Store) => PendingSubjectName::LibraryData,
            S::Audience(coven::Audience::Circle(_)) => PendingSubjectName::CircleData,
            S::Connection => PendingSubjectName::Connection,
        };
        Self { subject, wait }
    }
}

/// A file named by the row it is the file of.
impl From<FileRow> for PendingSubjectName {
    fn from(row: FileRow) -> Self {
        match row {
            FileRow::ReleaseFile { release_id: _, file_name, album_title } => {
                Self::ReleaseFile { file_name, album_title }
            }
            FileRow::Cover { album_title } => Self::Cover { album_title },
            FileRow::DeletedReleaseFile => Self::DeletedReleaseFile,
            FileRow::ArtistImage => Self::ArtistImage,
            FileRow::Avatar => Self::Avatar,
        }
    }
}

#[cfg(test)]
#[path = "pending_item_tests.rs"]
mod tests;
