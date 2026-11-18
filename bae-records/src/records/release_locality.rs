//! A release's moves to the cloud end before they finish: cancelled by the
//! person, or retired after a failure, every file of the release together.
use super::*;

/// The moves this device is making of the release `?1`'s files, moving or
/// failed, which the person can cancel. A release's files are staged or
/// moved in one write, so they share one move.
const RELEASE_MOVES: &str = release_moves!("?1");

impl Records {
    /// Cancel each move this device is making of these releases' files to
    /// the cloud, in one write: every file stays on this device only, as it
    /// was before, and plays from its original here. Nothing was switched,
    /// so no row changes. A release with no move to cancel is passed over.
    pub async fn cancel_release_uploads(&self, releases: Vec<String>) -> Result<(), DbError> {
        self.store
            .call_sql(move |sql| {
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
                }
                Ok(())
            })
            .await
    }
}
