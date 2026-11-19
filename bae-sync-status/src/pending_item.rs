//! What each of coven's pending records is about, named so a person
//! recognizes it, with why it waits.

/// What a pending record is about, by what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingSubjectName {
    /// A file.
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

/// Why a record waits. Two waits are the same when they wait for the same
/// reason and name the same error behind it: a surface shows both.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingWait {
    /// The record's first unmet condition, kept as coven typed it.
    pub reason: coven::PendingReason,
    /// The text of the error behind `reason`, with every cause below it,
    /// when the run that met it kept it; none after the store reopens.
    /// coven keeps it beside every record, the connection's included.
    pub cause: Option<String>,
}

impl PendingWait {
    /// The wait of `record`.
    pub fn of(record: &coven::PendingRecord) -> Self {
        Self { reason: record.reason.clone(), cause: record.cause.clone() }
    }
}

/// One thing coven is waiting on while a library is restored or joined,
/// before any library is open.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingItem {
    pub subject: PendingSubjectName,
    /// The record's first unmet condition, kept as coven typed it.
    pub reason: coven::PendingReason,
    /// The text of the error behind `reason`, with every cause below it,
    /// when the run that met it kept it.
    pub cause: Option<String>,
}

impl PendingItem {
    /// A record named before any library is open: every subject but a file
    /// names itself, and a file is a file, since no library holds the row
    /// it is the file of yet.
    pub fn before_open(record: coven::PendingRecord) -> Self {
        use coven::PendingSubject as S;
        let subject = match record.subject {
            S::Write(_) => PendingSubjectName::LocalChanges,
            S::Slot(_) => PendingSubjectName::LibraryChanges,
            S::KeyCopy { .. } => PendingSubjectName::KeyCopy,
            S::Snapshot(_) => PendingSubjectName::Snapshot,
            S::Upload { .. } | S::Download { .. } | S::Keep { .. } | S::Move { .. } => {
                PendingSubjectName::File
            }
            S::Operation { kind, .. } => PendingSubjectName::Operation(kind),
            S::Retention { .. } => PendingSubjectName::Retention,
            S::Audience(coven::Audience::Store) => PendingSubjectName::LibraryData,
            S::Audience(coven::Audience::Circle(_)) => PendingSubjectName::CircleData,
            S::Connection => PendingSubjectName::Connection,
        };
        Self { subject, reason: record.reason, cause: record.cause }
    }
}

#[cfg(test)]
#[path = "pending_item_tests.rs"]
mod tests;
