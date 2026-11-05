//! The storage transition this process runs for each release, as
//! `release_storage_action` holds it, what a release's reads show of it, and
//! the writes that end one together with the change to the release's files.
//!
//! Its table is `release_storage_action.sql`.

/// The SQL creating this part's table.
pub(crate) const SQL: &str = include_str!("release_storage_action.sql");

use bae_store::DbError;
use bae_store::Store;
use bae_types::release_storage::ReleaseStorageAction;
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, Row};

/// Ends the storage transition of the release `?1`, whatever it is.
const END_RELEASE_STORAGE_ACTION: &str = "DELETE FROM release_storage_action WHERE release_id = ?1";

/// End `release_id`'s transition `action` in the write that finishes its
/// work, so the release shows the transition until that work has committed
/// and never after. A release not in `action` fails the write: the work
/// would finish a transition nobody admitted.
fn end_transition_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
    action: StoredTransition,
) -> Result<(), DbError> {
    let ended = sql.execute(
        "DELETE FROM release_storage_action WHERE release_id = ?1 AND action = ?2",
        params![release_id, action.column()],
    )?;
    if ended == 0 {
        return Err(DbError::Message(format!(
            "release {release_id} is not in the {} transition its write ends",
            action.column()
        )));
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
    Upload,
    Unpin,
    DetachAudio,
}

impl StoredTransition {
    fn column(self) -> &'static str {
        match self {
            Self::Upload => ReleaseStorageAction::Upload,
            Self::Unpin => ReleaseStorageAction::Unpin,
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

/// Move every file of a release on this device only to the cloud, as one
/// move: coven reads no original now, since each upload checks the bytes it
/// sends against the hashes the import's read recorded, and switches every
/// file to the cloud together once all are stored, changing no row.
/// `keep_here` keeps each file whole on this device too once stored. Until
/// then each file is read from its original here. The write that queues the
/// files ends the release's storage transition, the move's admission.
pub async fn attach_release_files(
    store: &Store,
    release_id: &str,
    keep_here: bool,
) -> Result<(), DbError> {
    let release_id = release_id.to_string();
    let files = store
        .read({
            let release_id = release_id.clone();
            move |sql| bae_records::records::read::get_files_for_release_on(&sql, &release_id)
        })
        .await?;
    if files.is_empty() {
        return Err(DbError::Message("release has no files".into()));
    }
    let moving: Vec<coven::MoveFile> = files
        .into_iter()
        .map(|file| match keep_here {
            true => coven::MoveFile::UserKeptHere(file.file),
            false => coven::MoveFile::User(file.file),
        })
        .collect();
    store
        .call_sql(move |sql| {
            sql.move_to_cloud(moving)?;
            end_transition_on(&sql, &release_id, StoredTransition::Upload)
        })
        .await
}

/// Release the pins on a release's files, `files`, and end its unpin
/// transition, in one write: the release shows Unpinning until its files
/// are evictable and never after. coven evicts each namespace down to its
/// budget in the same write.
pub async fn unpin_release(
    store: &Store,
    release_id: &str,
    files: Vec<coven::FileId>,
) -> Result<(), DbError> {
    let release_id = release_id.to_string();
    store
        .call_sql(move |sql| {
            sql.unpin(&files)?;
            end_transition_on(&sql, &release_id, StoredTransition::Unpin)
        })
        .await
}

/// Switch a release's files to this device only, each to the copy
/// `prepared` placed, which becomes the original coven records for it, and
/// end its make-local transition, in one write: the release shows the
/// transition until its files are here and never after. A write that fails
/// switches nothing, and coven removes the copies.
pub async fn make_release_local(
    store: &Store,
    release_id: &str,
    prepared: coven::PreparedDeviceOnly,
) -> Result<(), DbError> {
    let release_id = release_id.to_string();
    store
        .call_sql(move |sql| {
            sql.make_device_only(prepared)?;
            end_transition_on(&sql, &release_id, StoredTransition::DetachAudio)
        })
        .await
}

#[cfg(test)]
#[path = "release_storage_action_tests.rs"]
mod tests;
