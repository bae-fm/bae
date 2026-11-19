use super::*;
use crate::history::{HistoryEntry, HistoryPlay, PlayRecordError};
use bae_util::timestamp::StoredTimestamp as _;
use std::time::Duration;

impl People {
    /// Capture the person's setting once. A later setting change cannot publish this play.
    /// `None` when the setting is Off. A device with no person, or a person
    /// with no profile, is a bug: every registered member has a profile, so
    /// the play is refused, never dropped. A device the library removed still
    /// has its person, and records plays as them.
    pub async fn begin_play(
        &self,
        id: uuid::Uuid,
        track_id: &str,
        duration: Option<Duration>,
        started_at: UtcDateTime,
    ) -> Result<Option<HistoryPlay>, PlayRecordError> {
        let person = self.writing_person().await?.ok_or(PlayRecordError::NoPerson)?;
        let crate::people::Person { profile, membership } = person;
        let profile = profile.ok_or(PlayRecordError::NoProfile(membership.id))?;
        let Some(audience) = profile.history_sharing.audience(profile.private_circle) else {
            return Ok(None);
        };
        Ok(Some(HistoryPlay {
            entry: HistoryEntry {
                id,
                track_id: track_id.to_owned(),
                started_at,
                played: Duration::ZERO,
                person: membership.id,
                audience,
            },
            duration,
        }))
    }

    /// Upsert the same play as more audio is heard; unqualified skips write no row.
    pub async fn record_play(&self, play: &HistoryPlay) -> Result<bool, PlayRecordError> {
        if !play.qualifies() {
            return Ok(false);
        }
        let entry = play.entry.clone();
        if self.writing_person().await?.map(|person| person.membership.id) != Some(entry.person) {
            return Err(PlayRecordError::PersonChanged(entry.person));
        }
        let played_ms = bae_store::query::to_i64(entry.played.as_millis(), "played_ms")?;
        Ok(self.inner.store.call(move |sql| {
            sql.execute("INSERT INTO history (id, person_id, track_id, started_at, played_ms, audience) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET played_ms = MAX(history.played_ms, excluded.played_ms)", params![entry.id.to_string(), entry.person.to_string(), entry.track_id, entry.started_at.to_rfc3339(), played_ms, entry.audience])?;
            Ok(true)
        }).await?)
    }

    /// Every play the library holds, newest first: what the tests of
    /// recording a play read back. No surface lists plays.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn history(&self) -> Result<Vec<HistoryEntry>, DbError> {
        self.inner.store.read(|sql| Ok(history_rows(&sql)?)).await
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn history_rows(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<HistoryEntry>> {
    sql.query("SELECT id, track_id, started_at, played_ms, person_id, audience FROM history ORDER BY started_at DESC, id", [], |row| {
        let started_at = bae_store::read::rfc3339_column(row, "started_at")?;
        let played_ms: i64 = row.get(3)?;
        #[expect(clippy::map_err_ignore, reason = "the out-of-range error carries the column and the value itself")]
        let played_ms = u64::try_from(played_ms).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, played_ms))?;
        Ok(HistoryEntry { id: uuid_column(row, 0)?, track_id: row.get(1)?, started_at, played: Duration::from_millis(played_ms), person: coven::MemberId(uuid_column(row, 4)?), audience: row.get(5)? })
    })
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
