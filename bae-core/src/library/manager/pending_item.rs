//! What each of coven's pending records is about, named so a person
//! recognizes it, with why it waits and what they can do about it.

use super::*;
use crate::library::UploadFileLabel;

/// What a pending record is about. A file this device is uploading is named
/// by its place in the library; every other subject by what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingSubjectName {
    /// A file of a release this device is uploading.
    ReleaseFile {
        file_name: String,
        album_title: String,
    },
    /// A release's cover this device is uploading.
    Cover { album_title: String },
    /// An artist's image this device is uploading.
    ArtistImage,
    /// A person's profile picture this device is uploading.
    Avatar,
    /// A file this device is uploading whose release was deleted.
    DeletedReleaseFile,
    /// A file this device can't name: one another device uploads, which only
    /// that device's queue knows.
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
    pub action: PendingAction,
}

impl PendingWait {
    pub(crate) fn of(record: &coven::PendingRecord) -> Self {
        Self {
            reason: record.reason.clone(),
            action: pending_action(record),
        }
    }
}

/// One thing coven is waiting on, as the pending list shows it.
#[derive(Debug, Clone)]
pub struct PendingItem {
    pub subject: PendingSubjectName,
    pub wait: PendingWait,
}

impl PendingItem {
    /// A record named before any library is open, as while restoring or
    /// joining: every subject but a file names itself, and a file is named
    /// only by this device's upload queue, which doesn't exist yet.
    pub fn before_open(record: coven::PendingRecord) -> Self {
        Self::named(record, |_, _| None)
    }

    fn named(
        record: coven::PendingRecord,
        upload: impl Fn(coven::DeviceId, coven::FileId) -> Option<PendingSubjectName>,
    ) -> Self {
        use coven::PendingSubject as S;
        let wait = PendingWait::of(&record);
        let subject = match record.subject {
            S::Write(_) => PendingSubjectName::LocalChanges,
            S::Slot(_) => PendingSubjectName::LibraryChanges,
            S::KeyCopy { .. } => PendingSubjectName::KeyCopy,
            S::Snapshot(_) => PendingSubjectName::Snapshot,
            S::File { device, file } => upload(device, file).unwrap_or(PendingSubjectName::File),
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

/// The name of a file in this device's upload queue, from the queue's label
/// and the album title of the release it belongs to, when that release is
/// still in the library.
fn upload_name(label: UploadFileLabel, album_title: Option<String>) -> PendingSubjectName {
    match (label, album_title) {
        (UploadFileLabel::Filename(file_name), Some(album_title)) => {
            PendingSubjectName::ReleaseFile {
                file_name,
                album_title,
            }
        }
        (UploadFileLabel::Cover, Some(album_title)) => PendingSubjectName::Cover { album_title },
        (UploadFileLabel::Filename(_) | UploadFileLabel::Cover, None)
        | (UploadFileLabel::DeletedReleaseFile, _) => PendingSubjectName::DeletedReleaseFile,
        (UploadFileLabel::ArtistImage, _) => PendingSubjectName::ArtistImage,
        (UploadFileLabel::Avatar, _) => PendingSubjectName::Avatar,
    }
}

impl LibraryManager {
    /// Each pending record, named. A file this device uploads is named from
    /// its upload queue and its release's album; the queue is read only when
    /// a record is about a file.
    pub(crate) async fn pending_items(
        &self,
        records: Vec<coven::PendingRecord>,
    ) -> Result<Vec<PendingItem>, LibraryError> {
        let names_files = records
            .iter()
            .any(|record| matches!(record.subject, coven::PendingSubject::File { .. }));
        let uploads = if names_files {
            Some(self.upload_names().await?)
        } else {
            None
        };
        Ok(records
            .into_iter()
            .map(|record| {
                PendingItem::named(record, |device, file| {
                    uploads
                        .as_ref()
                        .expect("a file record reads the upload queue")
                        .get(&(device, file))
                        .cloned()
                })
            })
            .collect())
    }

    /// Every file in this device's upload queue, by its uploader and id, with
    /// its name.
    async fn upload_names(
        &self,
    ) -> Result<HashMap<(coven::DeviceId, coven::FileId), PendingSubjectName>, LibraryError> {
        let queue = self.database.outbox_queue().await?;
        let releases = self
            .database
            .queued_releases(
                queue
                    .uploads
                    .iter()
                    .filter_map(|upload| upload.release_id.clone())
                    .collect(),
            )
            .await?;
        Ok(queue
            .uploads
            .into_iter()
            .map(|upload| {
                let album_title = upload
                    .release_id
                    .as_ref()
                    .and_then(|release| releases.get(release))
                    .map(|release| release.title.clone());
                (
                    (upload.upload.file.uploader(), upload.upload.file.id()),
                    upload_name(upload.label, album_title),
                )
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "pending_item_tests.rs"]
mod tests;
