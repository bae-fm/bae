use super::*;
use crate::history::HistoryPlay;

impl LibraryManager {
    pub(crate) async fn begin_play(
        &self,
        track_id: &str,
        release_id: &str,
        duration: Option<std::time::Duration>,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<HistoryPlay, crate::db::DbError> {
        self.database
            .begin_play(
                self.ids.new_id(),
                track_id,
                release_id,
                duration,
                started_at,
            )
            .await
    }

    pub(crate) async fn record_play(&self, play: &HistoryPlay) -> Result<bool, crate::db::DbError> {
        self.database.record_play(play).await
    }

    /// Every play the library holds, newest first: what the tests of
    /// recording a play read back. No surface lists plays.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn history(&self) -> Result<Vec<crate::history::HistoryEntry>, crate::db::DbError> {
        self.database.history().await
    }
}
