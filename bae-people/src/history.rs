//! Each qualifying play retains the audience it had when it began.

use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryAudience {
    /// Listening before a registered profile exists remains on this device.
    DevicePrivate,
    Person {
        person: coven::MemberId,
        audience: coven::Audience,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: uuid::Uuid,
    pub track_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub played: Duration,
    pub audience: HistoryAudience,
}

/// Why a play could not be recorded.
#[derive(Debug, thiserror::Error)]
pub enum PlayRecordError {
    /// The play began as `0`, a person this device's membership no longer
    /// is: its person changed during the play. Nothing was written.
    #[error("the play began as {0:?}, whom this device no longer is")]
    PersonChanged(coven::MemberId),
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

/// A play in progress; a seek changes its position but never its identity.
#[derive(Clone, Debug)]
pub struct HistoryPlay {
    pub(crate) entry: HistoryEntry,
    pub(crate) duration: Option<Duration>,
}

impl HistoryPlay {
    pub fn entry(&self) -> &HistoryEntry {
        &self.entry
    }

    /// Absolute amount heard, excluding skipped positions, pauses and buffering.
    pub fn set_played(&mut self, played: Duration) {
        self.entry.played = played;
    }

    /// EOF supplies the actual length when imported metadata did not contain it.
    pub fn finish_at(&mut self, duration: Duration) {
        if self.duration.is_none() {
            self.duration = Some(duration);
        }
    }

    pub fn qualifies(&self) -> bool {
        self.entry.played > Duration::ZERO && self.entry.played >= self.threshold()
    }

    /// How much more must be heard before the play qualifies; `None` once it
    /// does.
    pub fn left_to_qualify(&self) -> Option<Duration> {
        if self.qualifies() {
            return None;
        }
        // A play qualifies only once something was heard, even at a zero
        // threshold.
        Some(self.threshold().saturating_sub(self.entry.played).max(Duration::from_nanos(1)))
    }

    fn threshold(&self) -> Duration {
        match self.duration {
            Some(duration) => (duration / 2).min(Duration::from_secs(30)),
            None => Duration::from_secs(30),
        }
    }
}
