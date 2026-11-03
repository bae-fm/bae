//! A release's files move to the cloud, stop moving, give up their pins or
//! become this device's own, every file of the release together, each in
//! the write that ends the release's transition.
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
    /// move: coven reads no original now, since each upload checks the bytes
    /// it sends against the hashes the import's read recorded, and switches
    /// every file to the cloud together once all are stored, changing no
    /// row. `keep_here` keeps each file whole on this device too once
    /// stored. Until then each file is read from its original here. The
    /// write that queues the files ends the release's storage transition,
    /// the move's admission.
    pub async fn attach_release_files(
        &self,
        release_id: &str,
        keep_here: bool,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let files = self.get_files_for_release(&release_id).await?;
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
        self.call_sql(move |sql| {
            sql.move_to_cloud(moving)?;
            super::release_storage_action::end_transition_on(
                &sql,
                &release_id,
                super::release_storage_action::StoredTransition::Upload,
            )
        })
        .await
    }

    /// Release the pins on a release's files, `files`, and end its unpin
    /// transition, in one write: the release shows Unpinning until its files
    /// are evictable and never after. coven evicts each namespace down to its
    /// budget in the same write.
    pub async fn unpin_release(
        &self,
        release_id: &str,
        files: Vec<coven::FileId>,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call_sql(move |sql| {
            sql.unpin(&files)?;
            super::release_storage_action::end_transition_on(
                &sql,
                &release_id,
                super::release_storage_action::StoredTransition::Unpin,
            )
        })
        .await
    }

    /// Switch a release's files to this device only, each to the copy
    /// `prepared` placed, which becomes the original coven records for it,
    /// and end its make-local transition, in one write: the release shows
    /// the transition until its files are here and never after. A write that
    /// fails switches nothing, and coven removes the copies.
    pub async fn make_release_local(
        &self,
        release_id: &str,
        prepared: coven::PreparedDeviceOnly,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call_sql(move |sql| {
            sql.make_device_only(prepared)?;
            super::release_storage_action::end_transition_on(
                &sql,
                &release_id,
                super::release_storage_action::StoredTransition::DetachAudio,
            )
        })
        .await
    }

    /// Cancel each move this device is making of these releases' files to
    /// the cloud, in one write: every file stays on this device only, as it
    /// was before, and plays from its original here. Nothing was switched,
    /// so no row changes. A release with no move to cancel is passed over.
    /// Returns the releases whose moves it cancelled.
    pub async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        self.call_sql(move |sql| {
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
        })
        .await
    }

    /// End the failed move `id`, whose original changed before its upload,
    /// as its pending record offers: coven retires it, and every file of it
    /// stays on this device only, as it was before it moved.
    pub async fn cancel_failed_release_move(&self, id: coven::MoveId) -> Result<(), DbError> {
        self.call_sql(move |sql| {
            sql.cancel_move(id)?;
            Ok(())
        })
        .await
    }
}
