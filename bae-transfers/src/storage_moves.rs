//! The writes that move releases' files: to the cloud, each release in a
//! write of its own, and out of the pinned cache, a whole selection in one
//! write.

use bae_store::DbError;
use bae_store::Store;
use bae_store::WriteError;
use coven::CovenWrite;

/// Why [`move_release_to_cloud`] or coven refused the write that moves a
/// release's files to the cloud. The refused write changes nothing.
#[derive(Debug, thiserror::Error)]
pub enum StorageMoveRefusal {
    /// No such release is in the library.
    #[error("the release is not in the library")]
    Missing,
    /// The release holds no files.
    #[error("the release has no files")]
    NoFiles,
    /// coven refused to move a file of the release, and its error says why:
    /// the file is on another device only, it is in the cloud already, it is
    /// in a move already (one still moving, or a failed one that the person
    /// cancels to move the file again), or this device has no record of it.
    #[error("coven refused to move a file of the release: {0}")]
    Coven(Box<coven::DbError>),
}

/// Why a write that moves a release's files to the cloud did not commit.
pub type StorageMoveError = WriteError<StorageMoveRefusal>;

/// Move every file of `release_id` to the cloud, in one write, as one move,
/// and return how many files it moved. coven reads no original now, since
/// each upload checks the bytes it sends against the hashes the import's read
/// recorded, and switches the release's files to the cloud together once all
/// are stored, changing no row. coven refuses the move when any file can't
/// move, and the write then changes nothing. With no storage set up, coven
/// accepts the move and uploads once storage is connected.
pub async fn move_release_to_cloud(
    store: &Store,
    release_id: String,
) -> Result<u32, StorageMoveError> {
    store
        .write(move |sql| {
            let exists: bool = sql.query_row(
                "SELECT EXISTS (SELECT 1 FROM releases WHERE id = ?1)",
                [&release_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(WriteError::Refused(StorageMoveRefusal::Missing));
            }
            let files = release_files_on(&sql, &release_id)?;
            if files.is_empty() {
                return Err(WriteError::Refused(StorageMoveRefusal::NoFiles));
            }
            let count = u32::try_from(files.len()).expect("a release's files fit a u32");
            sql.move_to_cloud(files.into_iter().map(coven::MoveFile::User).collect()).map_err(
                |error| match error {
                    coven::DbError::FileAbsent { .. }
                    | coven::DbError::NotThisDevice { .. }
                    | coven::DbError::NotDeviceOnlyHere { .. }
                    | coven::DbError::FileInMove { .. } => {
                        WriteError::Refused(StorageMoveRefusal::Coven(Box::new(error)))
                    }
                    error => error.into(),
                },
            )?;
            Ok(count)
        })
        .await
}

/// Why a release of a selection did not unpin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnpinRefusal {
    /// No such release is in the library.
    Missing,
    /// The release holds no files.
    NoFiles,
}

/// What one write unpinning a selection did.
#[derive(Debug, PartialEq, Eq)]
pub struct Unpinned {
    /// Each release whose pins were released, in the selection's order, with
    /// how many files it holds.
    pub unpinned: Vec<(String, u32)>,
    /// Each release that could not unpin, with why. It changed nothing.
    pub refused: Vec<(String, UnpinRefusal)>,
}

/// Unpin every file of `release_id`, as [`unpin_releases`] does for a
/// release. coven has nothing to remove for a file on one device only.
pub(crate) fn unpin_files_on(sql: &CovenWrite<'_>, release_id: &str) -> Result<(), DbError> {
    sql.unpin(&release_files_on(sql, release_id)?)?;
    Ok(())
}

/// The coven files of `release_id`'s files, in their ids' order.
fn release_files_on(sql: &CovenWrite<'_>, release_id: &str) -> Result<Vec<coven::FileId>, DbError> {
    Ok(sql.query(
        "SELECT file FROM release_files WHERE release_id = ?1 ORDER BY id",
        [release_id],
        |row| row.get(0),
    )?)
}

/// Unpin the files of every release of `release_ids`, in one write: coven
/// removes each file's copies from this device in that write, the pinned
/// whole copy and any cached chunks alike, and the partial download a pin
/// left. Nothing else is evicted, whatever room the budget has, and coven has
/// nothing to remove for a file on one device only. No transition is shown
/// on the way. A release that is gone or holds no files is named with why and
/// changes nothing; the rest unpin.
pub async fn unpin_releases(store: &Store, release_ids: Vec<String>) -> Result<Unpinned, DbError> {
    store
        .write(move |sql| {
            let mut outcome = Unpinned { unpinned: Vec::new(), refused: Vec::new() };
            for release_id in release_ids {
                let exists: bool = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM releases WHERE id = ?1)",
                    [&release_id],
                    |row| row.get(0),
                )?;
                if !exists {
                    outcome.refused.push((release_id, UnpinRefusal::Missing));
                    continue;
                }
                let files = release_files_on(&sql, &release_id)?;
                if files.is_empty() {
                    outcome.refused.push((release_id, UnpinRefusal::NoFiles));
                    continue;
                }
                let count = u32::try_from(files.len()).expect("a release's files fit a u32");
                sql.unpin(&files)?;
                outcome.unpinned.push((release_id, count));
            }
            Ok(outcome)
        })
        .await
}
