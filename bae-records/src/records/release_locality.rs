//! A release's moves to the cloud end before they finish: cancelled by the
//! person, or retired after a failure, every file of the release together.
use super::*;

/// The moves this device is making of a release's files that can be
/// cancelled: the move each file's upload belongs to, from coven's upload
/// queue. A release's files are staged or moved in one write, so they share
/// one move.
const RELEASE_MOVES: &str = "SELECT DISTINCT u.move FROM coven_uploads AS u \
     JOIN release_files AS f ON f.file = u.file \
     WHERE f.release_id = ?1 AND u.move IS NOT NULL";

impl Records {
    /// Cancel each move this device is making of these releases' files to
    /// the cloud, in one write: every file stays on this device only, as it
    /// was before, and plays from its original here. Nothing was switched,
    /// so no row changes. A release with no move to cancel is passed over.
    /// Returns the releases whose moves it cancelled.
    pub async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        self.store
            .call_sql(move |sql| {
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
        self.store
            .call_sql(move |sql| {
                sql.cancel_move(id)?;
                Ok(())
            })
            .await
    }
}
