//! The device-local `playback_state` row: where playback stood, so a restart
//! resumes it.

use super::*;

/// The single device-local `playback_state` row: where playback stood, so a
/// restart resumes it. The queue it plays from is in the queue tables, and
/// the volume it resumes at in `playback_now`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackResumeRow {
    /// The queue entry playing, `None` when its track was deleted.
    pub current: Option<crate::QueueEntry>,
    /// How far into the current track's stream it had played.
    pub position_ms: Option<u64>,
}

// The `playback_state` row.
sqlive_compose::fragment!(
    SavedResume { position_ms: Option<u64> },
    r#"
    SELECT saved.id, saved.current_entry, saved.current_track_id, saved.position_ms
    FROM playback_state AS saved
    WHERE saved.id = 'current'
"#
);

impl SavedResume {
    /// The row, or why the entry it names can't be read.
    fn into_row(self) -> Result<PlaybackResumeRow, DbError> {
        let current = match self.current_entry.zip(self.current_track_id) {
            Some((entry, track_id)) => Some(crate::QueueEntry {
                id: entry.parse().map_err(|problem: String| {
                    DbError::unreadable("playback_state.current_entry", problem)
                })?,
                track_id,
            }),
            None => None,
        };
        Ok(PlaybackResumeRow { current, position_ms: self.position_ms })
    }
}

sqlive_compose::read!(
    writer StoredResume,
    r#"
    SELECT saved.*
    FROM @crate::tables::resume::SavedResume AS saved
"#
);

// What a playback service takes up as it starts: the kept volume, how the
// queue repeats, the resume row while one is stored, and how many tracks the
// context lane holds, played or not.
sqlive_compose::read!(
    writer PlaybackStartRead { context_tracks: u64 },
    r#"
    SELECT level.*, queue.*, saved.*,
           (SELECT COUNT(*) FROM playback_context) AS context_tracks
    FROM @crate::tables::now::KeptLevel AS level
    JOIN @crate::tables::queue_reads::QueueRepeat AS queue ON 1
    LEFT JOIN @crate::tables::resume::SavedResume AS saved ON 1
"#
);

/// What a playback service takes up as it starts. The volume and the resume
/// row's entry each say for themselves why they can't be read, so one that
/// can't leaves the rest to take up.
#[derive(Debug)]
pub struct PlaybackStart {
    /// The kept volume and whether it is muted.
    pub level: Result<(crate::Volume, bool), DbError>,
    pub repeat: crate::RepeatMode,
    /// The resume row, `None` when none is stored.
    pub resume: Result<Option<PlaybackResumeRow>, DbError>,
    /// How many tracks the context lane holds, played or not.
    pub context_tracks: u64,
}

/// Read what a playback service takes up as it starts.
pub async fn playback_start(store: &Store) -> Result<PlaybackStart, DbError> {
    let row = store.read_row(PlaybackStartReadParams {}).await?;
    Ok(PlaybackStart {
        level: row.level.into_level(),
        repeat: row.queue.into_mode(),
        resume: row.saved.map(SavedResume::into_row).transpose(),
        context_tracks: row.context_tracks,
    })
}

/// Whether a resume row is stored.
pub(super) fn resume_stored_on(sql: &impl bae_store::WriterRead) -> Result<bool, DbError> {
    Ok(statement_optional(sql, &StoredResumeParams {})?.is_some())
}

/// Read the device-local `playback_state` row, `None` when none is stored.
#[cfg(any(test, feature = "test-utils"))]
pub async fn load_playback_state(store: &Store) -> Result<Option<PlaybackResumeRow>, DbError> {
    store.read(|sql| playback_state_on(&sql)).await
}

/// Follow the device-local `playback_state` row as `load_playback_state`
/// reads it.
#[cfg(any(test, feature = "test-utils"))]
pub fn subscribe_playback_state(
    store: &Store,
) -> coven::LiveQuery<Option<PlaybackResumeRow>, bae_store::DbError> {
    store.subscribe(|sql| playback_state_on(&sql))
}

/// The `playback_state` row `sql` reads, `None` when none is stored.
#[cfg(any(test, feature = "test-utils"))]
fn playback_state_on(sql: &SqlReadContext<'_>) -> Result<Option<PlaybackResumeRow>, DbError> {
    statement_optional(sql, &StoredResumeParams {})?.map(|row| row.saved.into_row()).transpose()
}

/// Plant damage for a test: the resume row names an entry no queue entry id
/// parses from, so reading it fails.
#[cfg(any(test, feature = "test-utils"))]
pub fn plant_unreadable_resume_row_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_state",
        "PRAGMA foreign_keys = OFF; \
         INSERT OR REPLACE INTO playback_state (id, current_entry, current_track_id, position_ms) \
         VALUES ('current', 'not an entry id', 'a-track', 1000);",
    );
}
