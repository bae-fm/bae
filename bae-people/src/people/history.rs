use super::profiles::{writer_on, Writer};
use super::*;
use crate::history::{HistoryEntry, HistoryPlay, PlayRecordError};
use bae_store::WriteStatements;
use bae_util::timestamp::StoredTimestamp as _;
use std::time::Duration;

// A play's row, which a later write of the same play only lengthens.
coven::write!(
    RecordPlay(
        id: &'p str,
        person: coven::MemberId,
        track: &'p str,
        started_at: &'p str,
        played_on: &'p str,
        played_ms: i64,
        audience: coven::Audience,
    ),
    r#"
    INSERT INTO history (id, person_id, track_id, started_at, played_on, played_ms, audience)
    VALUES (:id, :person, :track, :started_at, :played_on, :played_ms, :audience)
    ON CONFLICT (id) DO UPDATE SET played_ms = MAX(history.played_ms, excluded.played_ms)
"#
);

impl People {
    /// Capture the person's setting once. A later setting change cannot publish this play.
    /// `played_on` is the calendar day the playing device showed when the play began.
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
        played_on: time::Date,
    ) -> Result<Option<HistoryPlay>, PlayRecordError> {
        let writer = self.inner.store.read(|sql| writer_on(&sql)).await?;
        let Writer { member, profile } = writer.ok_or(PlayRecordError::NoPerson)?;
        let profile = profile.ok_or(PlayRecordError::NoProfile(member))?;
        let Some(audience) = profile.history_sharing.audience(profile.private_circle) else {
            return Ok(None);
        };
        Ok(Some(HistoryPlay {
            entry: HistoryEntry {
                id,
                track_id: track_id.to_owned(),
                started_at,
                played_on,
                played: Duration::ZERO,
                person: member,
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
        let played_ms = bae_store::query::to_i64(entry.played.as_millis(), "played_ms")?;
        self.inner
            .store
            .call(move |sql| {
                if writer_on(sql)?.map(|writer| writer.member) != Some(entry.person) {
                    return Ok(Err(PlayRecordError::PersonChanged(entry.person)));
                }
                // The first write stores the day, and the play keeps it.
                sql.run(&RecordPlayParams {
                    id: &entry.id.to_string(),
                    person: entry.person,
                    track: &entry.track_id,
                    started_at: &entry.started_at.to_rfc3339(),
                    played_on: &entry.played_on.to_string(),
                    played_ms,
                    audience: entry.audience,
                })?;
                Ok(Ok(true))
            })
            .await?
    }

    /// Every play the library holds, newest first: what the tests of
    /// recording a play read back. No surface lists plays.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn history(&self) -> Result<Vec<HistoryEntry>, DbError> {
        let plays = self.inner.store.read_rows(EveryPlayParams {}).await?;
        plays.into_iter().map(EveryPlayRow::into_entry).collect()
    }
}

// Every play the library holds, newest first.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer EveryPlay {
        played_ms: u64,
        person_id: coven::MemberId,
        audience: coven::Audience,
    },
    r#"
    SELECT play.id, play.track_id, play.started_at, play.played_ms, play.person_id,
           play.audience, play.played_on
    FROM history AS play
    ORDER BY play.started_at DESC, play.id
"#
);

#[cfg(any(test, feature = "test-utils"))]
impl EveryPlayRow {
    fn into_entry(self) -> Result<HistoryEntry, DbError> {
        let started_at = bae_util::timestamp::parse_rfc3339(&self.started_at)
            .map_err(|_error| DbError::unreadable("history.started_at", &self.started_at))?;
        let played_on = time::Date::parse(
            &self.played_on,
            &time::format_description::well_known::Iso8601::DATE,
        )
        .map_err(|_error| DbError::unreadable("history.played_on", &self.played_on))?;
        Ok(HistoryEntry {
            id: stored_uuid("history.id", &self.id)?,
            track_id: self.track_id,
            started_at,
            played_on,
            played: Duration::from_millis(self.played_ms),
            person: self.person_id,
            audience: self.audience,
        })
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
