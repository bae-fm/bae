//! The writes that move a selection of releases' files: to the cloud, and
//! out of the pinned cache. Each is one write over the whole selection.

use bae_store::Store;
use bae_store::WriteError;
use coven::CovenWrite;

/// Why a write that moves a selection's files was refused. Nothing of it
/// landed.
#[derive(Debug, thiserror::Error)]
pub enum StorageMoveRefusal {
    /// A move to the cloud needs storage set up on this device, and none is.
    #[error("no storage is set up to move releases to")]
    NoStorage,
}

/// Why a write that moves a selection's files did not commit.
pub type StorageMoveError = WriteError<StorageMoveRefusal>;

/// Why a release of a selection did not move to the cloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveRefusal {
    /// No such release is in the library.
    Missing,
    /// The release holds no files.
    NoFiles,
    /// A file of the release is not on this device only, or is moving
    /// already.
    NotMovable,
}

/// What one write moving a selection to the cloud did.
#[derive(Debug, PartialEq, Eq)]
pub struct MovedToCloud {
    /// Each release whose files now wait in the upload queue, in the
    /// selection's order, with how many files it moved.
    pub moved: Vec<(String, u32)>,
    /// Each release that could not move, with why. It changed nothing.
    pub refused: Vec<(String, MoveRefusal)>,
}

/// One release's files, each with whether this device alone holds it and
/// whether its upload is queued.
const RELEASE_MOVE_FILES: &str = "SELECT f.file, \
            COALESCE(d.is_self, 0), \
            EXISTS (SELECT 1 FROM coven_uploads AS u WHERE u.file = f.file) \
     FROM release_files AS f \
     JOIN coven_files AS c ON c.id = f.file \
     LEFT JOIN coven_devices AS d ON d.device = coven_file_device(c.coordinates) \
     WHERE f.release_id = ?1 ORDER BY f.id";

/// Move the files of every release of `release_ids` that is on this device
/// only to the cloud, in one write, each release as one move of its own:
/// coven reads no original now, since each upload checks the bytes it sends
/// against the hashes the import's read recorded, and switches a release's
/// files to the cloud together once all are stored, changing no row.
/// A release that cannot move is named with why and changes nothing; the rest
/// move. Without storage set up the write moves nothing and is refused.
pub async fn move_releases_to_cloud(
    store: &Store,
    release_ids: Vec<String>,
) -> Result<MovedToCloud, StorageMoveError> {
    store
        .write(move |sql| {
            let storage: bool =
                sql.query_row("SELECT storage = 'connected' FROM coven_sync_status", [], |row| {
                    row.get(0)
                })?;
            if !storage {
                return Err(WriteError::Refused(StorageMoveRefusal::NoStorage));
            }
            let mut outcome = MovedToCloud { moved: Vec::new(), refused: Vec::new() };
            for release_id in release_ids {
                let exists: bool = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM releases WHERE id = ?1)",
                    [&release_id],
                    |row| row.get(0),
                )?;
                if !exists {
                    outcome.refused.push((release_id, MoveRefusal::Missing));
                    continue;
                }
                let files: Vec<(coven::FileId, bool, bool)> =
                    sql.query(RELEASE_MOVE_FILES, [&release_id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })?;
                if files.is_empty() {
                    outcome.refused.push((release_id, MoveRefusal::NoFiles));
                    continue;
                }
                if files.iter().any(|(_, here, queued)| !here || *queued) {
                    outcome.refused.push((release_id, MoveRefusal::NotMovable));
                    continue;
                }
                let count = u32::try_from(files.len()).expect("a release's files fit a u32");
                let moving =
                    files.into_iter().map(|(file, _, _)| coven::MoveFile::User(file)).collect();
                sql.move_to_cloud(moving)?;
                outcome.moved.push((release_id, count));
            }
            Ok(outcome)
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
    /// A file of the release is not stored in the cloud.
    NotInStorage,
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

/// One release's files, each with whether it is stored in the cloud: no
/// device holds it alone.
const RELEASE_UNPIN_FILES: &str = "SELECT f.file, coven_file_device(c.coordinates) IS NULL \
     FROM release_files AS f \
     JOIN coven_files AS c ON c.id = f.file \
     WHERE f.release_id = ?1 ORDER BY f.id";

/// Unpin `release_id`'s files that are stored in the cloud, as
/// [`unpin_releases`] does for a release.
pub(crate) fn unpin_stored_files_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<(), bae_store::DbError> {
    let files: Vec<coven::FileId> = sql
        .query(RELEASE_UNPIN_FILES, [release_id], |row| {
            Ok((row.get::<_, coven::FileId>(0)?, row.get::<_, bool>(1)?))
        })?
        .into_iter()
        .filter_map(|(file, in_storage)| in_storage.then_some(file))
        .collect();
    if !files.is_empty() {
        sql.unpin(&files)?;
    }
    Ok(())
}

/// Unpin the files of every release of `release_ids` stored in the cloud, in
/// one write: coven removes each file's copies from this device in that write,
/// the pinned whole copy and any cached chunks alike, and the partial download
/// a pin left. Nothing else is evicted, whatever room the budget has. No
/// transition is shown on the way. A release that cannot unpin is named with
/// why and changes nothing; the rest unpin.
pub async fn unpin_releases(
    store: &Store,
    release_ids: Vec<String>,
) -> Result<Unpinned, StorageMoveError> {
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
                let files: Vec<(coven::FileId, bool)> =
                    sql.query(RELEASE_UNPIN_FILES, [&release_id], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?;
                if files.is_empty() {
                    outcome.refused.push((release_id, UnpinRefusal::NoFiles));
                    continue;
                }
                if files.iter().any(|(_, in_storage)| !in_storage) {
                    outcome.refused.push((release_id, UnpinRefusal::NotInStorage));
                    continue;
                }
                let count = u32::try_from(files.len()).expect("a release's files fit a u32");
                let files: Vec<coven::FileId> = files.into_iter().map(|(file, _)| file).collect();
                sql.unpin(&files)?;
                outcome.unpinned.push((release_id, count));
            }
            Ok(outcome)
        })
        .await
}
