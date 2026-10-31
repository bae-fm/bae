//! A release's files move to the cloud, or stop moving, every file of the
//! release together.
use super::*;

/// The moves this device is making of a release's files that can be
/// cancelled: the move each file's upload belongs to, from coven's upload
/// queue. A release's files are staged or moved in one write, so they share
/// one move.
const RELEASE_MOVES: &str = "SELECT DISTINCT u.move FROM coven_uploads AS u \
     JOIN release_files AS f ON f.file = u.file \
     WHERE f.release_id = ?1 AND u.move IS NOT NULL";

impl Database {
    /// Move every file of a release on this device only to the cloud, as one
    /// move: each original is read whole now, and coven switches every file
    /// to the cloud together once all are stored, changing no row.
    /// `keep_here` keeps each file whole on this device too once stored.
    /// Until then each file is read from its original here.
    pub(crate) async fn attach_release_files(
        &self,
        release_id: &str,
        keep_here: bool,
    ) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        if files.is_empty() {
            return Err(DbError::Message("release has no files".into()));
        }
        let mut moving = Vec::with_capacity(files.len());
        for file in files {
            let original = self
                .inner
                .handle
                .user_file(file.file)
                .await?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "release file {} has no original on this device",
                        file.id
                    ))
                })?;
            let prepared = coven::prepare_user_file(&original.path, |_| {}).await?;
            moving.push(match keep_here {
                true => coven::MoveFile::UserKeptHere(file.file, prepared),
                false => coven::MoveFile::User(file.file, prepared),
            });
        }
        self.write_with_files(
            |_| Ok(()),
            move |sql, ()| {
                sql.move_to_cloud(moving)?;
                Ok(())
            },
        )
        .await
        .map_err(Self::coven_error)
    }

    /// Cancel each move this device is making of these releases' files to
    /// the cloud, in one write: every file stays on this device only, as it
    /// was before, and plays from its original here. Nothing was switched,
    /// so no row changes. A release with no move to cancel is passed over.
    /// Returns the releases whose moves it cancelled.
    pub(crate) async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        self.write_with_files(
            |_| Ok(()),
            move |sql, ()| {
                let mut cancelled = Vec::with_capacity(releases.len());
                for release in releases {
                    let moves: Vec<coven::MoveId> =
                        sql.query(RELEASE_MOVES, [&release], |row| row.get(0))?;
                    if moves.is_empty() {
                        tracing::debug!(release, "no move to the cloud to cancel");
                        continue;
                    }
                    for id in moves {
                        sql.cancel_move(id)?;
                    }
                    cancelled.push(release);
                }
                Ok(cancelled)
            },
        )
        .await
        .map_err(Self::coven_error)
    }

    /// End the failed move `id`, whose original changed before its upload,
    /// as its pending record offers: coven retires it, and every file of it
    /// stays on this device only, as it was before it moved.
    pub(crate) async fn cancel_failed_release_move(
        &self,
        id: coven::MoveId,
    ) -> Result<(), DbError> {
        self.write_with_files(
            |_| Ok(()),
            move |sql, ()| {
                sql.cancel_move(id)?;
                Ok(())
            },
        )
        .await
        .map_err(Self::coven_error)
    }
}
