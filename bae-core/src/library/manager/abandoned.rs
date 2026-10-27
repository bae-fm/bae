//! The abandoned attachments the sync activity lists: files this device
//! attached whose rows took a newer change before their uploads completed, so
//! no row will ever name them (coven §17.2). Each stays listed, named and with
//! why, until the person dismisses it.

use super::*;
use crate::db::AbandonedRow;

/// One file this device attached that no row will name, as the sync activity
/// lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct AbandonedItem {
    /// What the file was, named by its row when the row is still there.
    pub subject: PendingSubjectName,
    /// coven's record, which says why and which dismissing forgets.
    pub record: coven::AbandonedAttachment,
}

/// Keeps the sync status's abandoned attachments current, named.
pub(super) struct AbandonedFollower {
    manager: LibraryManager,
    records: coven::LiveQuery<Vec<coven::AbandonedAttachment>>,
}

impl AbandonedFollower {
    pub(super) fn new(manager: LibraryManager) -> Self {
        Self {
            records: manager.database.subscribe_abandoned_attachments(),
            manager,
        }
    }

    pub(super) async fn run(mut self) {
        while let Some(records) = self.records.next().await {
            let items = match records {
                Ok(records) => self
                    .manager
                    .abandoned_items(records)
                    .await
                    .map_err(|error| {
                        warn!(%error, "could not name abandoned attachments");
                        crate::ui::UiError::diagnostic(error.category(), error)
                    }),
                Err(error) => {
                    warn!(%error, "could not read abandoned attachments");
                    let error = LibraryError::from(error);
                    Err(crate::ui::UiError::diagnostic(error.category(), error))
                }
            };
            self.manager.sync_status.set_abandoned(items);
        }
    }
}

impl LibraryManager {
    /// Each record, named by its row as the library holds it now.
    async fn abandoned_items(
        &self,
        records: Vec<coven::AbandonedAttachment>,
    ) -> Result<Vec<AbandonedItem>, LibraryError> {
        let rows = self.database.abandoned_rows(&records).await?;
        Ok(records
            .into_iter()
            .zip(rows)
            .map(|(record, row)| AbandonedItem {
                subject: match row {
                    AbandonedRow::ReleaseFile {
                        file_name,
                        album_title,
                    } => PendingSubjectName::ReleaseFile {
                        file_name,
                        album_title,
                    },
                    AbandonedRow::Cover { album_title } => {
                        PendingSubjectName::Cover { album_title }
                    }
                    AbandonedRow::DeletedReleaseFile => PendingSubjectName::DeletedReleaseFile,
                    AbandonedRow::ArtistImage => PendingSubjectName::ArtistImage,
                    AbandonedRow::Avatar => PendingSubjectName::Avatar,
                },
                record,
            })
            .collect())
    }

    /// Forget an abandoned attachment the person has seen.
    pub async fn dismiss_abandoned_attachment(
        &self,
        record: coven::AbandonedAttachment,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .dismiss_abandoned_attachments(std::slice::from_ref(&record))
            .await?)
    }
}
