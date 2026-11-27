//! A track's stored audio and its tags, each read as one statement: for one
//! track, for a track beside the one after it, and for every track of a
//! release.

use super::*;
use crate::resolved_track_audio::{ResolvedTrackAudio, ResolvedTrackAudioSegment, TrackAudioError};

// The segments of the audio format, in segment order, as a JSON array: each
// with the size and container of the release file it names and its scrub
// points' blob, empty for a segment that stores none. A release file's `file`
// always names a record in `coven_files` (coven 17.2), so the size is NULL
// only when no release file has the segment's id.
sqlive_compose::fragment!(
    AudioFormatSegments on audio_formats {
        struct StoredSegment {
            role: String,
            file_id: String,
            file_size: Option<u64>,
            container: Option<String>,
            start_sample: u64,
            end_sample: Option<u64>,
            start_byte: Option<u64>,
            end_byte: Option<u64>,
            scrub_points: hex,
        }
        segments: json<Vec<StoredSegment>>
    },
    r#"
    SELECT json_group_array(json_object(
               'role', segment.role, 'file_id', segment.file_id, 'file_size', stored.size,
               'container', file.source_audio_container,
               'start_sample', segment.start_sample, 'end_sample', segment.end_sample,
               'start_byte', segment.start_byte, 'end_byte', segment.end_byte,
               'scrub_points', hex(scrubbed.points))
               ORDER BY segment.segment_index) AS segments
    FROM audio_format_segments AS segment
    LEFT JOIN release_files AS file ON file.id = segment.file_id
    LEFT JOIN coven_files AS stored ON stored.id = file.file
    LEFT JOIN audio_segment_scrub_points AS scrubbed
           ON scrubbed.audio_format_segment_id = segment.id
    WHERE segment.audio_format_id = @parent.id
"#
);

// A track's audio format, with its segments.
sqlive_compose::fragment!(
    StoredAudioFormat {
        pregap_samples: Option<u64>,
        generated_pregap_samples: Option<u64>,
    },
    r#"
    SELECT format.track_id AS format_track_id, format.content_type, format.pregap_samples,
           format.generated_pregap_samples, format.sample_rate, format.bits_per_sample,
           format.channels, format.track_loudness_lufs, format.track_peak_linear,
           @crate::records::track_audio::AudioFormatSegments(parent: format) AS segments
    FROM audio_formats AS format
"#
);

// Each track's audio: its length, the loudness its release was measured at,
// and its audio format with its segments, no format for a track that stores
// none. `first_of_release` says whether no track of its release plays before
// it.
sqlive_compose::fragment!(
    TrackAudio { duration_ms: u64, first_of_release: bool },
    r#"
    SELECT track.id AS audio_track_id, track.release_id, track.position, track.duration_ms,
           NOT EXISTS (SELECT 1 FROM tracks AS earlier
                       WHERE earlier.release_id = track.release_id
                         AND earlier.position < track.position) AS first_of_release,
           release.album_loudness_lufs, release.album_peak_linear,
           format.*
    FROM tracks AS track
    JOIN releases AS release ON release.id = track.release_id
    LEFT JOIN @crate::records::track_audio::StoredAudioFormat AS format
           ON format.format_track_id = track.id
"#
);

// What a saved copy of a track is tagged with, for each track of one release,
// whose mediums and tracks' places the caller gives: its title and shown
// credits, its album's title, year and credits, its release's year, its
// number and disc as a surface outside bae states them, and how many tracks
// the release has.
sqlive_compose::fragment!(
    TrackTags
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ), places(
        id TEXT, release_id TEXT, medium_id TEXT, side INTEGER, track_number INTEGER,
        stated_length_ms INTEGER, position INTEGER
    ) {
        total_tracks: i64
    },
    r#"
    SELECT track.id AS tags_track_id, track.title,
           album.title AS album_title, album.year AS album_year,
           release.year AS release_year,
           @crate::records::credit_fragments::TrackCredits(parent: track) AS shown_credits,
           @crate::records::fragments::AlbumCredits(parent: album) AS album_credits,
           numbered.*,
           (SELECT COUNT(*) FROM tracks AS counted
            WHERE counted.release_id = track.release_id) AS total_tracks
    FROM @crate::records::export_numbers::ExportNumbers(
             mediums: (SELECT * FROM mediums),
             places: (SELECT * FROM places)) AS numbered
    JOIN tracks AS track ON track.id = numbered.track_id
    JOIN releases AS release ON release.id = track.release_id
    JOIN albums AS album ON album.id = release.album_id
"#
);

// One track's audio; no row for a track the library does not hold.
sqlive_compose::read!(
    OneTrackAudio(track: String),
    r#"
    SELECT audio.*
    FROM @crate::records::track_audio::TrackAudio AS audio
    WHERE audio.audio_track_id = :track
"#
);

// One track's tags; no row for a track the library does not hold.
sqlive_compose::read!(
    OneTrackTags(track: String),
    r#"
    SELECT tags.*
    FROM @crate::records::track_audio::TrackTags(
        mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                         medium.side_count
                  FROM tracks AS asked
                  JOIN release_mediums AS medium ON medium.release_id = asked.release_id
                  WHERE asked.id = :track),
        places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                        held.track_number, held.stated_length_ms, held.position
                 FROM tracks AS asked
                 JOIN tracks AS held ON held.release_id = asked.release_id
                 WHERE asked.id = :track)) AS tags
    WHERE tags.tags_track_id = :track
"#
);

// One track's audio and tags, then the audio and tags of the track after it
// on its release; no row for a track the library does not hold.
sqlive_compose::read!(
    TrackAndNextForSave(track: String),
    r#"
    SELECT audio.*, tags.*
    FROM @crate::records::track_audio::TrackAudio AS audio
    JOIN @crate::records::track_audio::TrackTags(
        mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                         medium.side_count
                  FROM tracks AS asked
                  JOIN release_mediums AS medium ON medium.release_id = asked.release_id
                  WHERE asked.id = :track),
        places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                        held.track_number, held.stated_length_ms, held.position
                 FROM tracks AS asked
                 JOIN tracks AS held ON held.release_id = asked.release_id
                 WHERE asked.id = :track)) AS tags
      ON tags.tags_track_id = audio.audio_track_id
    WHERE audio.audio_track_id IN (
        :track,
        (SELECT later.id
         FROM tracks AS asked
         JOIN tracks AS later
           ON later.release_id = asked.release_id AND later.position > asked.position
         WHERE asked.id = :track
         ORDER BY later.position
         LIMIT 1))
    ORDER BY audio.position
"#
);

// The audio and tags of every track of one release, in playing order.
sqlive_compose::read!(
    ReleaseForSave(release: String),
    r#"
    SELECT audio.*, tags.*
    FROM @crate::records::track_audio::TrackAudio AS audio
    JOIN @crate::records::track_audio::TrackTags(
        mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                         medium.side_count
                  FROM release_mediums AS medium WHERE medium.release_id = :release),
        places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                        held.track_number, held.stated_length_ms, held.position
                 FROM tracks AS held WHERE held.release_id = :release)) AS tags
      ON tags.tags_track_id = audio.audio_track_id
    WHERE audio.release_id = :release
    ORDER BY audio.position, audio.audio_track_id
"#
);

/// What a saved copy of a track is tagged with, as the library holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackTags {
    pub title: String,
    /// The credits the track shows: its own, else its album's.
    pub artists: Vec<LibraryCredit>,
    pub album_artists: Vec<LibraryCredit>,
    pub album_title: String,
    /// The year the album first came out.
    pub album_year: Option<i32>,
    /// The year the release's pressing states.
    pub release_year: Option<i32>,
    /// The track's number among its medium's tracks, from one.
    pub number: u32,
    /// Its medium's position, for a release of several mediums.
    pub disc: Option<u32>,
    /// How many tracks its release has.
    pub total_tracks: usize,
}

/// A track as a save reads it: its audio and its tags.
#[derive(Debug, Clone)]
pub struct TrackForSave {
    pub audio: ResolvedTrackAudio,
    pub tags: DbTrackTags,
}

/// One track as a save of it alone reads it: the track, whether its release
/// plays it first, and the audio of the track after it, `None` for the last.
#[derive(Debug, Clone)]
pub struct TrackSaveRead {
    pub track: TrackForSave,
    pub is_first: bool,
    pub next: Option<ResolvedTrackAudio>,
}

impl Records {
    /// `track_id`'s audio as the library holds it now.
    pub(crate) async fn track_audio(
        &self,
        track_id: &str,
    ) -> Result<ResolvedTrackAudio, TrackAudioError> {
        let statement = OneTrackAudioParams { track: track_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        row.ok_or_else(|| TrackAudioError::TrackNotFound(track_id.to_owned()))?.audio.decoded()
    }

    /// What a saved copy of `track_id` is tagged with.
    pub async fn track_tags(&self, track_id: &str) -> Result<DbTrackTags, TrackAudioError> {
        let statement = OneTrackTagsParams { track: track_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        Ok(row
            .ok_or_else(|| TrackAudioError::TrackNotFound(track_id.to_owned()))?
            .tags
            .decoded()?)
    }

    /// `track_id` as a save of it alone reads it.
    pub async fn track_for_save(&self, track_id: &str) -> Result<TrackSaveRead, TrackAudioError> {
        let statement = TrackAndNextForSaveParams { track: track_id.to_owned() };
        let mut rows = self.store.read_rows(statement).await?.into_iter();
        let first =
            rows.next().ok_or_else(|| TrackAudioError::TrackNotFound(track_id.to_owned()))?;
        let is_first = first.audio.first_of_release;
        let track = TrackForSave { audio: first.audio.decoded()?, tags: first.tags.decoded()? };
        let next = rows.next().map(|row| row.audio.decoded()).transpose()?;
        Ok(TrackSaveRead { track, is_first, next })
    }

    /// Every track of `release_id` as a save of the release reads it, in
    /// playing order.
    pub async fn release_tracks_for_save(
        &self,
        release_id: &str,
    ) -> Result<Vec<TrackForSave>, TrackAudioError> {
        let statement = ReleaseForSaveParams { release: release_id.to_owned() };
        self.store
            .read_rows(statement)
            .await?
            .into_iter()
            .map(|row| Ok(TrackForSave { audio: row.audio.decoded()?, tags: row.tags.decoded()? }))
            .collect()
    }
}

impl TrackAudio {
    /// The track's audio. A track with no format or no segments, and a
    /// segment whose file the library lacks or holds as other than audio,
    /// each fail with their own error.
    fn decoded(self) -> Result<ResolvedTrackAudio, TrackAudioError> {
        let track_id = self.audio_track_id;
        let format = self.format.ok_or_else(|| TrackAudioError::NoAudioFormat(track_id.clone()))?;
        let unreadable = |reason: String| {
            TrackAudioError::Database(DbError::unreadable("a track's audio", reason))
        };
        let sample_rate =
            track_audio_count(format.sample_rate, "sample_rate", &track_id).map_err(unreadable)?;
        let channels =
            track_audio_count(format.channels, "channels", &track_id).map_err(unreadable)?;
        let bits_per_sample =
            track_bit_depth(format.bits_per_sample, &track_id).map_err(unreadable)?;
        let segments = format
            .segments
            .segments
            .into_iter()
            .map(|segment| {
                let file_size = segment
                    .file_size
                    .ok_or_else(|| TrackAudioError::FileNotFound(segment.file_id.clone()))?;
                let container = segment
                    .container
                    .as_deref()
                    .map(bae_util::audio_container::AudioContainer::from_name)
                    .ok_or_else(|| TrackAudioError::NotAudio(segment.file_id.clone()))?;
                let role = DbAudioSegmentRole::from_db_value(&segment.role).ok_or_else(|| {
                    unreadable(format!("unknown audio segment role: {}", segment.role))
                })?;
                // A blob holds at least one point, so no bytes means the
                // segment stores none.
                let scrub_points = if segment.scrub_points.is_empty() {
                    Vec::new()
                } else {
                    super::scrub_points_column::decode(&segment.scrub_points)?
                };
                Ok(ResolvedTrackAudioSegment {
                    role,
                    file_id: segment.file_id,
                    file_size,
                    container,
                    span: crate::SegmentSpan {
                        start_sample: segment.start_sample,
                        end_sample: segment.end_sample,
                        start_byte: segment.start_byte,
                        end_byte: segment.end_byte,
                    },
                    scrub_points: crate::ScrubPoints::new(scrub_points),
                })
            })
            .collect::<Result<Vec<_>, TrackAudioError>>()?;
        if segments.is_empty() {
            return Err(TrackAudioError::NoSegments(track_id));
        }
        Ok(ResolvedTrackAudio {
            track_id,
            release_id: self.release_id,
            segments,
            duration_ms: self.duration_ms,
            pregap_samples: format.pregap_samples,
            generated_pregap_samples: format.generated_pregap_samples,
            sample_rate,
            channels,
            bits_per_sample,
            content_type: ContentType::from_mime(&format.content_type),
            track_loudness_lufs: format.track_loudness_lufs,
            track_peak_linear: format.track_peak_linear,
            album_loudness_lufs: self.album_loudness_lufs,
            album_peak_linear: self.album_peak_linear,
        })
    }
}

impl TrackTags {
    fn decoded(self) -> Result<DbTrackTags, DbError> {
        let year = |year: Option<i64>, what: &'static str| {
            year.map(|year| i32::try_from(year).map_err(|_error| DbError::unreadable(what, year)))
                .transpose()
        };
        // Each credits fragment yields a value for every row it is spread
        // over, so a missing one is a broken composition.
        let missing = |whose: &str| {
            DbError::unreadable(
                format!("the {whose} credits of track {}", self.tags_track_id),
                None::<String>,
            )
        };
        let shown = self.shown_credits.ok_or_else(|| missing("shown"))?;
        let album = self.album_credits.ok_or_else(|| missing("album"))?;
        Ok(DbTrackTags {
            title: self.title,
            artists: super::read::credits_from_json(&shown.credits)?,
            album_artists: super::read::credits_from_json(&album.credits)?,
            album_title: self.album_title,
            album_year: year(self.album_year, "an album's year")?,
            release_year: year(self.release_year, "a release's year")?,
            number: self.numbered.number,
            disc: self.numbered.disc,
            total_tracks: bae_store::query::to_usize(self.total_tracks, "a release's track count")?,
        })
    }
}

#[cfg(test)]
#[path = "track_audio_tests.rs"]
mod tests;
