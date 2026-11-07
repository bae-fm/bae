//! The storage transition this process runs for each release, as
//! `release_storage_action` holds it, what a release's reads show of it, and
//! the writes that end one together with the change to the release's files.
//!
//! Its table is `release_storage_action.sql`.

/// The SQL creating this part's table.
pub(crate) const SQL: &str = include_str!("release_storage_action.sql");

use bae_store::DbError;
use bae_store::Store;
use bae_store::WriteError;
use bae_types::release_storage::ReleaseStorageAction;
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, Row};

/// Ends the storage transition of the release `?1`, whatever it is.
const END_RELEASE_STORAGE_ACTION: &str = "DELETE FROM release_storage_action WHERE release_id = ?1";

/// Why a write that ends a release's storage transition was refused. Nothing
/// of it landed.
#[derive(Debug, thiserror::Error)]
pub enum StorageTransitionRefusal {
    /// The release is not in the transition the write ends: its work would
    /// finish a transition nobody admitted.
    #[error("release {release_id} is not in the {transition:?} transition its write ends")]
    NotInTransition { release_id: String, transition: StoredTransition },
    /// A move to the cloud needs storage set up on this device, and none is.
    #[error("no storage is set up to move releases to")]
    NoStorage,
}

/// Why a write that ends a release's storage transition did not commit.
pub type StorageTransitionError = WriteError<StorageTransitionRefusal>;

/// End `release_id`'s transition `action` in the write that finishes its
/// work, so the release shows the transition until that work has committed
/// and never after. A release not in `action` fails the write: the work
/// would finish a transition nobody admitted.
fn end_transition_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
    action: StoredTransition,
) -> Result<(), StorageTransitionError> {
    let ended = sql.execute(
        "DELETE FROM release_storage_action WHERE release_id = ?1 AND action = ?2",
        params![release_id, action.column()],
    )?;
    if ended == 0 {
        return Err(WriteError::Refused(StorageTransitionRefusal::NotInTransition {
            release_id: release_id.to_string(),
            transition: action,
        }));
    }
    Ok(())
}

/// The transition the release `release` (an SQL expression naming its id)
/// is in, as [`transfer_action_column`] reads it: its row here, else `pin`
/// while the download queue's active entry names it, else NULL.
pub fn transfer_action_sql(release: &str) -> String {
    format!(
        "COALESCE( \
            (SELECT a.action FROM release_storage_action a WHERE a.release_id = {release}), \
            (SELECT 'pin' FROM download_queue q \
             WHERE q.release_id = {release} AND q.state = 'active'))"
    )
}

/// The transition [`transfer_action_sql`] read into the column `name`.
pub fn transfer_action_column(
    row: &Row<'_>,
    name: &str,
) -> rusqlite::Result<Option<ReleaseStorageAction>> {
    let column = row.as_ref().column_index(name)?;
    let Some(action) = row.get::<_, Option<String>>(column)? else {
        return Ok(None);
    };
    match ReleaseStorageAction::from_key(&action) {
        Some(action) => Ok(Some(action)),
        None => Err(bae_store::read::column_conversion_error(
            row,
            column,
            format!("storage action {action} names no transition"),
        )),
    }
}

/// A storage transition `release_storage_action` holds: every one but a
/// pin, which is the download queue's active entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredTransition {
    DetachAudio,
}

impl StoredTransition {
    fn column(self) -> &'static str {
        match self {
            Self::DetachAudio => ReleaseStorageAction::DetachAudio,
        }
        .key()
    }
}

/// Record that `release_id` enters the transition `action`, in one write.
/// `false`, writing nothing, when the release is in one already.
pub async fn begin_release_storage_action(
    store: &Store,
    release_id: &str,
    action: StoredTransition,
) -> Result<bool, DbError> {
    let action = action.column();
    let release_id = release_id.to_string();
    store
        .call(move |sql| {
            let written = sql.execute(
                "INSERT INTO release_storage_action (release_id, action) VALUES (?1, ?2) \
                 ON CONFLICT (release_id) DO NOTHING",
                params![release_id, action],
            )?;
            Ok(written == 1)
        })
        .await
}

/// End `release_id`'s transition, in one write of its own, after its work
/// failed. A release in none is passed over.
pub async fn end_release_storage_action(store: &Store, release_id: &str) -> Result<(), DbError> {
    let release_id = release_id.to_string();
    store
        .call(move |sql| {
            sql.execute(END_RELEASE_STORAGE_ACTION, [release_id])?;
            Ok(())
        })
        .await
}

/// The transition `release_id` is in now, as its reads show it.
pub fn release_transfer_action_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseStorageAction>, DbError> {
    Ok(sql.query_row(
        &format!("SELECT {} AS transfer_action", transfer_action_sql("?1")),
        [release_id],
        |row| transfer_action_column(row, "transfer_action"),
    )?)
}

/// [`release_transfer_action_on`] on its own read.
#[cfg(any(test, feature = "test-utils"))]
pub async fn release_transfer_action(
    store: &Store,
    release_id: &str,
) -> Result<Option<ReleaseStorageAction>, DbError> {
    let release_id = release_id.to_string();
    store.read(move |sql| release_transfer_action_on(&sql, &release_id)).await
}

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
/// `keep_here` keeps each file whole on this device too once stored. A
/// release that cannot move is named with why and changes nothing; the rest
/// move. Without storage set up the write moves nothing and is refused.
pub async fn move_releases_to_cloud(
    store: &Store,
    release_ids: Vec<String>,
    keep_here: bool,
) -> Result<MovedToCloud, StorageTransitionError> {
    store
        .write(move |sql| {
            let storage: bool =
                sql.query_row("SELECT storage = 'connected' FROM coven_sync_status", [], |row| {
                    row.get(0)
                })?;
            if !storage {
                return Err(WriteError::Refused(StorageTransitionRefusal::NoStorage));
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
                let moving = files
                    .into_iter()
                    .map(|(file, _, _)| match keep_here {
                        true => coven::MoveFile::UserKeptHere(file),
                        false => coven::MoveFile::User(file),
                    })
                    .collect();
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

/// Release the pins on the files of every release of `release_ids` stored in
/// the cloud, in one write: coven moves each file from the pinned cache to
/// the evictable cache, still readable and now droppable, and evicts each
/// namespace down to its budget in the same write. No transition is shown on
/// the way. A release that cannot unpin is named with why and changes
/// nothing; the rest unpin.
pub async fn unpin_releases(
    store: &Store,
    release_ids: Vec<String>,
) -> Result<Unpinned, StorageTransitionError> {
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

/// Switch a release's files to this device only, each to the copy
/// `prepared` placed, which becomes the original coven records for it, and
/// end its make-local transition, in one write: the release shows the
/// transition until its files are here and never after, and the release's
/// download queue entry, which a release held here has no use for, ends in
/// the same write. A write that fails switches nothing, and coven removes
/// the copies.
pub async fn make_release_local(
    store: &Store,
    release_id: &str,
    prepared: coven::PreparedDeviceOnly,
) -> Result<(), StorageTransitionError> {
    let release_id = release_id.to_string();
    store
        .write(move |sql| {
            sql.make_device_only(prepared)?;
            crate::queued_releases::tables::end_download_entry_on(&sql, &release_id)?;
            end_transition_on(&sql, &release_id, StoredTransition::DetachAudio)
        })
        .await
}

#[cfg(test)]
#[path = "release_storage_action_tests.rs"]
mod tests;
