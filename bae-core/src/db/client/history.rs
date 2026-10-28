use super::*;
use crate::history::{HistoryAudience, HistoryEntry, HistoryPlay};
use std::time::Duration;

impl Database {
    /// Capture the person's setting once. A later setting change cannot publish this play.
    pub async fn begin_play(
        &self,
        id: uuid::Uuid,
        track_id: &str,
        release_id: &str,
        duration: Option<Duration>,
        started_at: DateTime<Utc>,
    ) -> Result<HistoryPlay, DbError> {
        let audience = match self.own_member().await? {
            Some((member, Some(profile))) => HistoryAudience::Person {
                person: member,
                audience: profile.history_sharing.audience(profile.private_circle),
            },
            Some((_, None)) | None => HistoryAudience::DevicePrivate,
        };
        Ok(HistoryPlay {
            entry: HistoryEntry {
                id,
                track_id: track_id.to_owned(),
                release_id: release_id.to_owned(),
                started_at,
                played: Duration::ZERO,
                audience,
            },
            duration,
        })
    }

    /// Upsert the same play as more audio is heard; unqualified skips write no row.
    pub async fn record_play(&self, play: &HistoryPlay) -> Result<bool, DbError> {
        if !play.qualifies() {
            return Ok(false);
        }
        let _registration = self.inner.history_registration.lock().await;
        let mut entry = play.entry.clone();
        let own = self.own_member().await?;
        let member = own.as_ref().map(|(member, _)| *member);
        match entry.audience {
            HistoryAudience::DevicePrivate => {
                if let Some((member, Some(profile))) = own {
                    entry.audience = HistoryAudience::Person {
                        person: member,
                        audience: coven::Audience::Circle(profile.private_circle),
                    };
                }
            }
            HistoryAudience::Person { person, .. } if Some(person) != member => {
                return Err(DbError::Message(
                    "the listening person's membership changed during this play".into(),
                ));
            }
            HistoryAudience::Person { .. } => {}
        }
        let played_ms = i64::try_from(entry.played.as_millis()).map_err(|error| {
            DbError::Message(format!(
                "played duration exceeds SQLite's integer range: {error}"
            ))
        })?;
        self.call(move |sql| {
            match entry.audience {
                HistoryAudience::DevicePrivate => {
                    sql.execute("INSERT INTO local_history (id, track_id, release_id, started_at, played_ms) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET played_ms = MAX(local_history.played_ms, excluded.played_ms)", params![entry.id.to_string(), entry.track_id, entry.release_id, entry.started_at.to_rfc3339(), played_ms])?;
                }
                HistoryAudience::Person { person, audience } => {
                    sql.execute("INSERT INTO history (id, person_id, track_id, release_id, started_at, played_ms, audience) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO UPDATE SET played_ms = MAX(history.played_ms, excluded.played_ms)", params![entry.id.to_string(), person.to_string(), entry.track_id, entry.release_id, entry.started_at.to_rfc3339(), played_ms, audience])?;
                    sql.execute("DELETE FROM local_history WHERE id = ?", [entry.id.to_string()])?;
                }
            }
            Ok(true)
        }).await
    }

    /// Every play the library holds, newest first: what the tests of
    /// recording a play read back. No surface lists plays.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn history(&self) -> Result<Vec<HistoryEntry>, DbError> {
        self.read(|sql| Ok(history_rows(&sql)?)).await
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn history_rows(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<HistoryEntry>> {
    sql.query("SELECT id, track_id, release_id, started_at, played_ms, person_id, audience FROM history UNION ALL SELECT id, track_id, release_id, started_at, played_ms, NULL, NULL FROM local_history ORDER BY started_at DESC, id", [], |row| {
        let audience = if row.get::<_, Option<String>>(5)?.is_some() {
            HistoryAudience::Person { person: coven::MemberId(uuid_column(row, 5)?), audience: row.get(6)? }
        } else { HistoryAudience::DevicePrivate };
        let started_at: String = row.get(3)?;
        let started_at = started_at.parse::<DateTime<Utc>>().map_err(|error| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error)))?;
        let played_ms: i64 = row.get(4)?;
        #[expect(clippy::map_err_ignore, reason = "the out-of-range error carries the column and the value itself")]
        let played_ms = u64::try_from(played_ms).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, played_ms))?;
        Ok(HistoryEntry { id: uuid_column(row, 0)?, track_id: row.get(1)?, release_id: row.get(2)?, started_at, played: Duration::from_millis(played_ms), audience })
    })
}

pub(super) fn adopt_local_history(
    sql: &SqlContext<'_, '_>,
    profile: &crate::people::PersonProfile,
) -> rusqlite::Result<()> {
    sql.execute("INSERT INTO history (id, person_id, track_id, release_id, started_at, played_ms, audience) SELECT id, ?1, track_id, release_id, started_at, played_ms, ?2 FROM local_history", params![profile.id.to_string(), coven::Audience::Circle(profile.private_circle)])?;
    sql.execute("DELETE FROM local_history", [])?;
    Ok(())
}
