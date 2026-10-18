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
    pub release_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub played: Duration,
    pub audience: HistoryAudience,
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
        let threshold = match self.duration {
            Some(duration) => (duration / 2).min(Duration::from_secs(30)),
            None => Duration::from_secs(30),
        };
        self.entry.played > Duration::ZERO && self.entry.played >= threshold
    }
}
