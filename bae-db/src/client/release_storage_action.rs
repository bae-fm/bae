//! The storage transition this process runs for each release, as
//! `release_storage_action` holds it, and what a release's reads show of it.

use super::*;
use bae_types::release_storage::ReleaseStorageAction;

/// Ends the storage transition of the release `?1`, whatever it is.
const END_RELEASE_STORAGE_ACTION: &str = "DELETE FROM release_storage_action WHERE release_id = ?1";

/// End `release_id`'s transition `action` in the write that finishes its
/// work, so the release shows the transition until that work has committed
/// and never after. A release not in `action` fails the write: the work
/// would finish a transition nobody admitted.
pub(super) fn end_transition_on(
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
    match action.as_str() {
        "upload" => Ok(Some(ReleaseStorageAction::Upload)),
        "pin" => Ok(Some(ReleaseStorageAction::Pin)),
        "unpin" => Ok(Some(ReleaseStorageAction::Unpin)),
        "detach_audio" => Ok(Some(ReleaseStorageAction::DetachAudio)),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            format!("storage action {other} names no transition").into(),
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
            Self::Upload => "upload",
            Self::Unpin => "unpin",
            Self::DetachAudio => "detach_audio",
        }
    }
}

impl Database {
    /// Record that `release_id` enters the transition `action`, in one
    /// write. `false`, writing nothing, when the release is in one already.
    pub async fn begin_release_storage_action(
        &self,
        release_id: &str,
        action: StoredTransition,
    ) -> Result<bool, DbError> {
        let action = action.column();
        let release_id = release_id.to_string();
        self.call(move |sql| {
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
    pub async fn end_release_storage_action(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call(move |sql| {
            sql.execute(END_RELEASE_STORAGE_ACTION, [release_id])?;
            Ok(())
        })
        .await
    }

    /// The transition `release_id` is in now, as its reads show it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn release_transfer_action(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseStorageAction>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            Ok(sql.query_row(
                &format!("SELECT {} AS transfer_action", transfer_action_sql("?1")),
                [release_id],
                |row| transfer_action_column(row, "transfer_action"),
            )?)
        })
        .await
    }
}
