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
    /// The file, which dismissing forgets.
    pub file: coven::FileId,
    pub reason: coven::AbandonReason,
}

impl From<AbandonedRow> for AbandonedItem {
    fn from(row: AbandonedRow) -> Self {
        Self {
            subject: row.file_row.into(),
            file: row.file,
            reason: row.reason,
        }
    }
}

/// Keeps the sync status's abandoned attachments current, named.
pub(super) struct AbandonedFollower {
    manager: LibraryManager,
    rows: coven::LiveQuery<Vec<AbandonedRow>>,
}

impl AbandonedFollower {
    pub(super) fn new(manager: LibraryManager) -> Self {
        Self {
            rows: manager.database.subscribe_abandoned_rows(),
            manager,
        }
    }

    pub(super) async fn run(mut self) {
        while let Some(rows) = self.rows.next().await {
            let items = rows
                .map(|rows| rows.into_iter().map(AbandonedItem::from).collect())
                .map_err(|error| {
                    warn!(%error, "could not read abandoned attachments");
                    let error = LibraryError::from(error);
                    crate::ui::UiError::diagnostic(error.category(), error)
                });
            self.manager.sync_status.set_abandoned(items);
        }
    }
}

impl LibraryManager {
    /// Forget an abandoned attachment the person has seen.
    pub async fn dismiss_abandoned_attachment(
        &self,
        file: coven::FileId,
    ) -> Result<(), LibraryError> {
        Ok(self.database.dismiss_abandoned_attachments(&[file]).await?)
    }
}
