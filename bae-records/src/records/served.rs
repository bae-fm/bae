//! A release and a track as a server outside bae's screens serves them: a
//! Subsonic album, which is a release, and its songs. [`ServedRelease`] and
//! [`ServedTrack`] are the fragments every such read spreads.

use super::*;
use bae_catalogs::catalog::Catalog;
use time::UtcDateTime;

// Each release as an album outside bae shows it: its album, its place among
// its album's releases, the year it shows (its pressing's, else its album's),
// when it was added, its album's title and credits, how many tracks it has and
// how long they play, its cover's version, and its catalog records.
sqlive_compose::fragment!(
    ServedRelease {
        album_order: f64,
        shown_year: Option<i64>,
        song_count: i64,
        duration_ms: i64,
        cover_version: Option<String>,
    },
    r#"
    SELECT release.id AS served_release_id, release.album_id AS served_album_id,
           release.album_order, COALESCE(release.year, album.year) AS shown_year,
           release.created_at, album.title AS album_title,
           @crate::records::fragments::AlbumCredits(parent: album) AS credits,
           (SELECT COUNT(*) FROM tracks AS counted
            WHERE counted.release_id = release.id) AS song_count,
           (SELECT COALESCE(SUM(timed.duration_ms), 0) FROM tracks AS timed
            WHERE timed.release_id = release.id) AS duration_ms,
           (SELECT cover.version FROM @crate::records::files::CoverVersion AS cover
            WHERE cover.id = release.id) AS cover_version,
           described.*
    FROM releases AS release
    JOIN albums AS album ON album.id = release.album_id
    JOIN @crate::records::fragments::ReleaseRecords AS described
      ON described.records_release_id = release.id
"#
);

// Each audio format as a song outside bae states it: its rate, depth and
// channels, and the one file that plays its track whole when one does: a
// format of one main segment that starts at its file's start and runs to its
// end, with no silence bae generates before it (§8.1).
sqlive_compose::fragment!(
    ServedAudio {
        sample_rate: std::num::NonZeroU32,
        bits_per_sample: Option<std::num::NonZeroU32>,
        channels: std::num::NonZeroU32,
        whole_size: Option<u64>,
    },
    r#"
    SELECT format.track_id AS served_audio_track_id, format.sample_rate,
           format.bits_per_sample, format.channels, whole.whole_name, whole.whole_size,
           whole.whole_container
    FROM audio_formats AS format
    LEFT JOIN (SELECT segment.audio_format_id, file.original_filename AS whole_name,
                      stored.size AS whole_size, file.source_audio_container AS whole_container
               FROM audio_format_segments AS segment
               JOIN release_files AS file ON file.id = segment.file_id
               JOIN coven_files AS stored ON stored.id = file.file
               WHERE segment.role = 'main' AND segment.start_sample = 0
                 AND segment.end_sample IS NULL
                 AND NOT EXISTS (SELECT 1 FROM audio_format_segments AS other
                                 WHERE other.audio_format_id = segment.audio_format_id
                                   AND other.id <> segment.id)) AS whole
           ON whole.audio_format_id = format.id
          AND COALESCE(format.generated_pregap_samples, 0) = 0
"#
);

// Each track as a song outside bae shows it: its release, title, length,
// when it was added, the credits it shows (its own, else its album's), and
// its audio as a song states it, none for a track that stores no audio.
sqlive_compose::fragment!(
    ServedTrack { duration_ms: u64 },
    r#"
    SELECT track.id AS served_track_id, track.release_id, track.title, track.duration_ms,
           track.created_at AS added_at,
           @crate::records::credit_fragments::TrackCredits(parent: track) AS shown_credits,
           audio.*
    FROM tracks AS track
    LEFT JOIN @crate::records::served::ServedAudio AS audio
           ON audio.served_audio_track_id = track.id
"#
);

// One release as an album outside bae serves it, with each of its tracks in
// playing order and how such a surface numbers it: one row with no track for a
// release with none, and no row for a release the library does not hold.
sqlive_compose::read!(
    writer ServedAlbum(release: String),
    r#"
    SELECT served.*, numbered.*, song.*
    FROM @crate::records::served::ServedRelease AS served
    LEFT JOIN @crate::records::export_numbers::ExportNumbers(
             mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                              medium.side_count
                       FROM release_mediums AS medium WHERE medium.release_id = :release),
             places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                             held.track_number, held.stated_length_ms, held.position
                      FROM tracks AS held WHERE held.release_id = :release)) AS numbered
           ON 1
    LEFT JOIN @crate::records::served::ServedTrack AS song
           ON song.served_track_id = numbered.track_id
    WHERE served.served_release_id = :release
    ORDER BY numbered.position, numbered.track_id
"#
);

// One track as a song outside bae serves it, with how such a surface numbers
// it and its release as an album; no row for a track the library does not
// hold.
sqlive_compose::read!(
    writer ServedSong(track: String),
    r#"
    SELECT served.*, numbered.*, song.*
    FROM @crate::records::served::ServedTrack AS song
    JOIN @crate::records::served::ServedRelease AS served
      ON served.served_release_id = song.release_id
    JOIN @crate::records::export_numbers::ExportNumbers(
             mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                              medium.side_count
                       FROM tracks AS asked
                       JOIN release_mediums AS medium ON medium.release_id = asked.release_id
                       WHERE asked.id = :track),
             places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                             held.track_number, held.stated_length_ms, held.position
                      FROM tracks AS asked
                      JOIN tracks AS held ON held.release_id = asked.release_id
                      WHERE asked.id = :track)) AS numbered
      ON numbered.track_id = song.served_track_id
    WHERE song.served_track_id = :track
"#
);

/// A release as a server outside bae serves it as an album.
#[derive(Debug, Clone, PartialEq)]
pub struct DbServedRelease {
    pub id: String,
    pub album_id: String,
    /// Its place among its album's releases.
    pub album_order: f64,
    pub album_title: String,
    /// The album's credits, in list order.
    pub credits: Vec<LibraryCredit>,
    /// The year the release shows: its pressing's, else its album's.
    pub year: Option<i32>,
    pub created_at: UtcDateTime,
    pub cover: Option<ImageRef>,
    pub song_count: u64,
    /// How long its tracks play, together.
    pub duration_ms: u64,
    /// Its release in MusicBrainz, when a MusicBrainz record names its
    /// pressing.
    pub musicbrainz_id: Option<String>,
}

/// A track as a server outside bae serves it as a song.
#[derive(Debug, Clone)]
pub struct DbServedTrack {
    pub id: String,
    pub release_id: String,
    pub title: String,
    pub duration_ms: u64,
    pub created_at: UtcDateTime,
    /// The credits it shows: its own, else its album's.
    pub credits: Vec<LibraryCredit>,
    pub sample_rate: std::num::NonZeroU32,
    /// `None` for a lossy codec, which has no fixed sample depth.
    pub bits_per_sample: Option<std::num::NonZeroU32>,
    pub channels: std::num::NonZeroU32,
    /// The one file that plays the track whole, when one does.
    pub whole_file: Option<DbServedFile>,
}

/// A stored file that plays a track whole.
#[derive(Debug, Clone, PartialEq)]
pub struct DbServedFile {
    /// Its name as the import found it.
    pub name: String,
    pub size: u64,
    pub container: bae_util::audio_container::AudioContainer,
}

/// A track with how a surface outside bae numbers it (§4.6): its number
/// among its medium's tracks, and its medium's position for a release of
/// several mediums.
#[derive(Debug, Clone)]
pub struct DbNumberedServedTrack {
    pub track: DbServedTrack,
    pub number: u32,
    pub disc: Option<u32>,
}

/// A release as an album, with its tracks in playing order.
#[derive(Debug, Clone)]
pub struct DbServedAlbum {
    pub release: DbServedRelease,
    pub tracks: Vec<DbNumberedServedTrack>,
}

/// A track as a song, with its release as an album.
#[derive(Debug, Clone)]
pub struct DbServedSong {
    pub release: DbServedRelease,
    pub track: DbNumberedServedTrack,
}

impl ServedRelease {
    /// The release as an album. Every album has credits, so missing ones
    /// fail.
    pub fn decoded(self) -> Result<DbServedRelease, DbError> {
        let unreadable = |what: &str, value: String| {
            DbError::unreadable(
                format!("the {what} of served release {}", self.served_release_id),
                value,
            )
        };
        let credits = self.credits.as_ref().ok_or_else(|| unreadable("credits", String::new()))?;
        let credits = super::read::credits_from_json(&credits.credits)?;
        let year = self
            .shown_year
            .map(|year| i32::try_from(year).map_err(|_error| unreadable("year", year.to_string())))
            .transpose()?;
        let created_at = bae_util::timestamp::parse_rfc3339(&self.created_at)
            .map_err(|_unparsed| unreadable("created_at", self.created_at.clone()))?;
        let count = |what: &str, value: i64| {
            u64::try_from(value).map_err(|_error| unreadable(what, value.to_string()))
        };
        let musicbrainz_id = super::read::records_of_json(self.described.records_json())?
            .iter()
            .filter_map(bae_catalogs::records::ReleaseRecord::release_ref)
            .find(|release| release.catalog == Catalog::MusicBrainz)
            .map(|release| release.key.clone());
        Ok(DbServedRelease {
            song_count: count("track count", self.song_count)?,
            duration_ms: count("length", self.duration_ms)?,
            cover: crate::cover_ref(&self.served_release_id, self.cover_version),
            year,
            id: self.served_release_id,
            album_id: self.served_album_id,
            album_order: self.album_order,
            album_title: self.album_title,
            credits,
            created_at,
            musicbrainz_id,
        })
    }
}

impl ServedTrack {
    /// The track as a song. Every track shows credits, so missing ones fail,
    /// and so does a track that stores no audio, or whose whole file states
    /// no container.
    pub fn decoded(self) -> Result<DbServedTrack, DbError> {
        let id = self.served_track_id;
        let unreadable = |what: &str| {
            DbError::unreadable(format!("the {what} of served track {id}"), None::<String>)
        };
        let credits = self.shown_credits.ok_or_else(|| unreadable("credits"))?;
        let audio = self.audio.ok_or_else(|| unreadable("audio format"))?;
        let created_at = bae_util::timestamp::parse_rfc3339(&self.added_at)
            .map_err(|_unparsed| DbError::unreadable("a track's created_at", &self.added_at))?;
        let whole_file = match (audio.whole_name, audio.whole_size, audio.whole_container) {
            (Some(name), Some(size), Some(container)) => Some(DbServedFile {
                name,
                size,
                container: bae_util::audio_container::AudioContainer::from_name(&container),
            }),
            (None, None, None) => None,
            _ => return Err(unreadable("whole file")),
        };
        Ok(DbServedTrack {
            credits: super::read::credits_from_json(&credits.credits)?,
            release_id: self.release_id,
            title: self.title,
            duration_ms: self.duration_ms,
            created_at,
            sample_rate: audio.sample_rate,
            bits_per_sample: audio.bits_per_sample,
            channels: audio.channels,
            whole_file,
            id,
        })
    }
}

impl Records {
    /// The release `release_id` as an album outside bae shows it, with its
    /// tracks in playing order; `None` when the library holds no such
    /// release.
    pub async fn served_album(&self, release_id: &str) -> Result<Option<DbServedAlbum>, DbError> {
        let statement = ServedAlbumParams { release: release_id.to_owned() };
        let mut rows = self.store.read_rows(statement).await?.into_iter().peekable();
        let Some(first) = rows.peek() else {
            return Ok(None);
        };
        let release = first.served.clone().decoded()?;
        let tracks = rows
            .filter_map(|row| match (row.song, row.numbered) {
                (Some(song), Some(numbered)) => Some((song, numbered)),
                _ => None,
            })
            .map(|(song, numbered)| {
                Ok(DbNumberedServedTrack {
                    track: song.decoded()?,
                    number: numbered.number,
                    disc: numbered.disc,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(Some(DbServedAlbum { release, tracks }))
    }

    /// The track `track_id` as a song outside bae shows it, with its release
    /// as an album; `None` when the library holds no such track.
    pub async fn served_song(&self, track_id: &str) -> Result<Option<DbServedSong>, DbError> {
        let statement = ServedSongParams { track: track_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        row.map(|row| {
            Ok(DbServedSong {
                release: row.served.decoded()?,
                track: DbNumberedServedTrack {
                    track: row.song.decoded()?,
                    number: row.numbered.number,
                    disc: row.numbered.disc,
                },
            })
        })
        .transpose()
    }
}

#[cfg(test)]
#[path = "served_tests.rs"]
mod tests;
