//! A Storage Manager row: the release's half and its album's, and what its
//! storage state shows.

use super::*;
use crate::album_detail::{StoragePlace, StorageRowStatus};
use bae_records::records::query::AlbumSummaryRow;

/// One Storage Manager row as read, its album's half not yet decoded.
#[derive(Clone, PartialEq)]
pub(crate) struct StorageRowParts {
    release: DbReleaseSummary,
    release_cover_version: Option<String>,
    album: AlbumSummaryRow,
    status: StorageRowStatus,
    date_added: UtcDateTime,
}

impl StorageRowParts {
    pub(crate) fn process(self) -> Result<DbStorageRow, DbError> {
        Ok(DbStorageRow {
            release: self.release,
            date_added: self.date_added,
            release_cover_version: self.release_cover_version,
            album: self.album.process()?,
            status: self.status,
        })
    }
}

// Each release of the Storage Manager's list as its row shows it: where it
// sorts by storage and when it was added, from its listing row; its storage
// row, the storage actions it offers, its entry in the download queue and its
// media; its own cover's version; and its album's summary.
coven::fragment!(
    StorageListRow { storage_order: i64, release_cover_version: Option<String> },
    r#"
    SELECT kept.release_id AS row_release_id, kept.storage_order,
           kept.created_at AS date_added,
           storage.*, actions.*, download.*, media.*,
           (SELECT cover.version
            FROM @bae_records::records::files::CoverVersion AS cover
            WHERE cover.id = kept.release_id) AS release_cover_version,
           album.*
    FROM release_listing AS kept
    CROSS JOIN @bae_records::release_storage::ReleaseStorage AS storage
            ON storage.release_id = kept.release_id
    CROSS JOIN @crate::storage_actions::ReleaseStorageActions AS actions
            ON actions.actions_release_id = kept.release_id
    LEFT JOIN @bae_transfers::download_entry::QueuedDownloadEntry AS download
           ON download.release_id = kept.release_id
    CROSS JOIN @bae_records::records::fragments::MediaOfRelease AS media
            ON media.media_release_id = kept.release_id
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS album
            ON album.summary_album_id = kept.album_id
"#
);

impl StorageListRow {
    /// The row as read.
    pub(crate) fn into_parts(self) -> Result<StorageRowParts, DbError> {
        let entry = self.download.as_ref().map(|queued| queued.entry()).transpose()?;
        let status = status_of(
            self.storage_order,
            entry.clone(),
            self.storage.failed_move().map(|(file, _cause)| file.to_string()),
        )?;
        let date_added =
            bae_util::timestamp::parse_rfc3339(&self.date_added).map_err(|_unparsed| {
                DbError::unreadable("when a release was added", &self.date_added)
            })?;
        let total_size = self.storage.total_size()?;
        let album_id = self.album.album_id().to_string();
        let album = self.album.into_row()?;
        Ok(StorageRowParts {
            release: DbReleaseSummary {
                id: self.row_release_id,
                album_id,
                media: self.media.counts()?,
                remote: self.storage.stored()?,
                pinned: self.storage.pinned(),
                upload: self.storage.upload()?,
                download: entry,
                file_count: self.storage.file_count(),
                total_size,
                actions: self.actions.actions(),
            },
            release_cover_version: self.release_cover_version,
            album,
            status,
            date_added,
        })
    }
}

/// What the row's storage state shows, from the place the listing's
/// `storage_order` names, the release's download queue entry and the file
/// whose changed original made its move fail.
fn status_of(
    storage_order: i64,
    download: Option<bae_transfers::DownloadEntry>,
    failed_move_file: Option<String>,
) -> Result<StorageRowStatus, DbError> {
    let unreadable = |reason: String| DbError::unreadable("a release's storage state", reason);
    let place = match storage_order {
        0 => StoragePlace::Failed,
        1 => StoragePlace::Uploading,
        2 => StoragePlace::Pinning,
        3 => StoragePlace::OnOneDevice,
        4 => StoragePlace::Pinned,
        5 => StoragePlace::InStorage,
        order => return Err(unreadable(format!("storage order {order} names no place"))),
    };
    StorageRowStatus::of(place, download, failed_move_file).map_err(unreadable)
}
