//! What an edit of a release starts from, as fragments each such read
//! spreads: the release as its editor opens on it ([`EditorRelease`]), which
//! the metadata edit reads too, and what a save of an editor's form reads
//! beside it ([`ReleaseSaveRows`]). Each edit reads its rows as one
//! statement, and its write runs that statement again and refuses when the
//! rows changed.

use super::editor_tracks::StoredEditorTrack;
use super::read::{file_of, rows_of, FileColumns};
use super::release_records::RecordsTargetAlbum;
use super::sheet_files::{DbCarvedTrack, DbReleaseAudio};
use super::*;

// A release as its editor opens on it: its detail, with no person's stars,
// beside its album's title, year, credits and creation time, and the JSON
// array `editor_tracks` of its tracks' audio in release order
// ([`StoredEditorTrack`]), which no sheet edit changes: `unedited` is
// [`UNEDITED`]. No row for a release the library does not hold.
sqlive_compose::fragment!(
    EditorRelease(release: String, unedited: String) { editor_tracks: String },
    r#"
    SELECT detail.*, facts.*, album.*,
           (SELECT json_group_array(json_object(
                       'row_id', shown.row_id, 'side_kind', shown.side_kind,
                       'side_ordinal', shown.side_ordinal, 'place_number', shown.place_number,
                       'listed_length_ms', shown.listed_length_ms, 'track', shown.track,
                       'audio_format_count', shown.audio_format_count,
                       'main_segment_count', shown.main_segment_count,
                       'main_segment', shown.main_segment, 'sheet', shown.sheet)
                       ORDER BY shown.placed_medium, shown.placed_position)
            FROM @crate::records::editor_tracks::EditorTrackRows(
                release: :release, edits: :unedited,
                mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                                 medium.side_count
                          FROM release_mediums AS medium WHERE medium.release_id = :release),
                places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                                held.track_number, held.stated_length_ms, held.position
                         FROM tracks AS held WHERE held.release_id = :release)) AS shown
            WHERE NOT shown.empty_side) AS editor_tracks
    FROM @crate::records::release_detail::ReleaseDetail(
        stars: @crate::records::track_group_fragments::NoTrackStars) AS detail
    JOIN @crate::records::release_detail::ReleaseAlbumFacts AS facts
      ON facts.album_id = detail.album_id
    JOIN @crate::records::fragments::LibraryAlbum AS album ON album.album_id = detail.album_id
    WHERE detail.detail_release_id = :release
"#
);

// One release as its editor opens on it.
sqlive_compose::read!(
    writer OneEditorRelease(release: String, unedited: String),
    r#"
    SELECT shown.*
    FROM @crate::records::release_edit_rows::EditorRelease(
        release: :release, unedited: :unedited) AS shown
"#
);

/// The sheet edits that change no track.
pub const UNEDITED: &str = "{}";

// What the edits `edits` do to the tracks of the release `release`: the JSON
// array `dropped` of the ids of the tracks a save deletes, in release order,
// the JSON array `added` of the files a save adds a track for, each a
// `{file_id, name}` in the order their first deleted track sat, and the
// release's first medium.
sqlive_compose::fragment!(
    SheetChanges(release: String, edits: String) {
        dropped: String,
        added: String,
        first_medium: Option<String>
    },
    r#"
    SELECT (SELECT json_group_array(gone.track_id ORDER BY gone.position)
            FROM @crate::records::sheet_fragments::DroppedSheetTracks(
                release: :release, edits: :edits) AS gone) AS dropped,
           (SELECT json_group_array(json_object(
                       'file_id', fresh.file_id, 'name', f.original_filename)
                       ORDER BY fresh.first_position)
            FROM @crate::records::sheet_fragments::SheetAddedFiles(
                release: :release, edits: :edits) AS fresh
            JOIN release_files AS f ON f.id = fresh.file_id) AS added,
           (SELECT held.id FROM release_mediums AS held WHERE held.release_id = :release
            ORDER BY held.position LIMIT 1) AS first_medium
"#
);

// The audio of the release `release`'s tracks: the JSON arrays of their audio
// formats and of those formats' segments, in release order, and of the tracks
// a CUE sheet carves.
sqlive_compose::fragment!(
    ReleaseAudio(release: String) {
        audio_formats: String,
        audio_segments: String,
        carved: String
    },
    r#"
    SELECT (SELECT json_group_array(json_object(
                       'id', format.id, 'track_id', format.track_id,
                       'content_type', format.content_type,
                       'pregap_samples', format.pregap_samples,
                       'generated_pregap_samples', format.generated_pregap_samples,
                       'sample_rate', format.sample_rate,
                       'bits_per_sample', format.bits_per_sample, 'channels', format.channels,
                       'track_loudness_lufs', format.track_loudness_lufs,
                       'track_peak_linear', format.track_peak_linear,
                       'created_at', format.created_at)
                       ORDER BY held.position)
            FROM tracks AS held
            JOIN audio_formats AS format ON format.track_id = held.id
            WHERE held.release_id = :release) AS audio_formats,
           (SELECT json_group_array(json_object(
                       'id', segment.id, 'audio_format_id', segment.audio_format_id,
                       'segment_index', segment.segment_index, 'role', segment.role,
                       'file_id', segment.file_id, 'start_sample', segment.start_sample,
                       'end_sample', segment.end_sample, 'start_byte', segment.start_byte,
                       'end_byte', segment.end_byte, 'created_at', segment.created_at)
                       ORDER BY held.position, segment.segment_index)
            FROM tracks AS held
            JOIN audio_formats AS format ON format.track_id = held.id
            JOIN audio_format_segments AS segment ON segment.audio_format_id = format.id
            WHERE held.release_id = :release) AS audio_segments,
           (SELECT json_group_array(json_object(
                       'track_id', cut.track_id, 'sheet_file_id', cut.sheet_file_id,
                       'file_reference', cut.file_reference)
                       ORDER BY held.position)
            FROM tracks AS held
            JOIN carved_tracks AS cut ON cut.track_id = held.id
            WHERE held.release_id = :release) AS carved
"#
);

// What a save of a release editor's form reads beside the release as the
// editor opens on it ([`EditorRelease`]): the album the release is in and the
// one its records file it under once they are the form's `records` (a JSON
// array, `records_json`) where it replaced them (`relinked`), and else the
// release's own ([`FiledByRecords`](super::fragments::FiledByRecords)); what
// the form's edits do to its CUE sheets' tracks; its tracks' audio; and its
// files, as `FileDocuments` writes them.
sqlive_compose::fragment!(
    ReleaseSaveRows(release: String, relinked: bool, records: String, edits: String) {
        files: String
    },
    r#"
    SELECT filed.*, changes.*, audio.*,
           (SELECT json_group_array(json(listed.file_json)
                       ORDER BY listed.original_filename COLLATE natural_order,
                                listed.document_file_id)
            FROM @crate::records::file_fragments::FileDocuments(
                     files: (SELECT file.*
                             FROM @crate::records::file_fragments::ReleaseFileColumns AS file
                             WHERE file.release_id = :release)) AS listed) AS files
    FROM @crate::records::fragments::FiledByRecords(
             release: :release,
             records: (SELECT record.key, record.value ->> 0, record.value ->> 1,
                              record.value ->> 2, record.value ->> 3
                       FROM json_each(CASE WHEN :relinked THEN :records ELSE (
                           SELECT json_group_array(
                                      json_array(held.catalog, held.kind, held.key,
                                                 held.album_key)
                                      ORDER BY bae_catalog_rank(held.catalog), held.catalog)
                           FROM release_records AS held
                           WHERE held.release_id = :release) END) AS record)) AS filed
    JOIN @crate::records::release_edit_rows::SheetChanges(
             release: :release, edits: :edits) AS changes
    JOIN @crate::records::release_edit_rows::ReleaseAudio(release: :release) AS audio
"#
);

/// A release as the editor opens on it: its detail, with no person's stars,
/// its album, the album's credits, and its tracks' audio in release order.
pub struct EditorReleaseFacts {
    pub detail: DbReleaseDetail,
    pub album: DbAlbum,
    pub album_artists: Vec<crate::credits::LibraryCredit>,
    pub tracks: Vec<DbReleaseEditTrack>,
}

impl EditorRelease {
    /// The rows read into the release's values.
    pub fn decoded(self) -> Result<EditorReleaseFacts, DbError> {
        let year = self.facts.year()?;
        let is_compilation = self.facts.is_compilation();
        let created_at = self.facts.created_at()?;
        let (album_id, title, credits) = self.album.into_parts()?;
        let detail = self.detail.decoded()?;
        const DOCUMENT: &str = "a release editor's tracks";
        let tracks = serde_json::from_str::<Vec<StoredEditorTrack>>(&self.editor_tracks)
            .map_err(|error| DbError::unreadable_json(DOCUMENT, &self.editor_tracks, error))?
            .into_iter()
            .map(StoredEditorTrack::decoded)
            .collect::<Result<_, _>>()?;
        Ok(EditorReleaseFacts {
            detail,
            album: DbAlbum { id: album_id, title, year, is_compilation, created_at },
            album_artists: super::read::credits_from_json(&credits)?,
            tracks,
        })
    }
}

/// The release `release_id` as its editor opens on it; `None` when the
/// library holds no such release.
pub fn editor_release_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Option<EditorRelease>, DbError> {
    let statement =
        OneEditorReleaseParams { release: release_id.to_owned(), unedited: UNEDITED.to_owned() };
    Ok(bae_store::statement_optional(sql, &statement)?.map(|row| row.shown))
}

/// The rows of a release an edit of its metadata writes over: the release,
/// its album and its tracks in release order.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseToEdit {
    pub release: DbRelease,
    pub album: DbAlbum,
    pub tracks: Vec<DbTrack>,
}

impl EditorReleaseFacts {
    /// The rows an edit of the release's metadata writes over.
    pub fn to_edit(&self) -> ReleaseToEdit {
        ReleaseToEdit {
            release: self.detail.release.clone(),
            album: self.album.clone(),
            tracks: self.tracks.iter().map(|track| track.track.clone()).collect(),
        }
    }
}

/// The rows of the release `release_id` an edit of its metadata writes over;
/// `None` when the library holds no such release.
pub fn release_to_edit_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Option<ReleaseToEdit>, DbError> {
    editor_release_on(sql, release_id)?.map(|row| Ok(row.decoded()?.to_edit())).transpose()
}

impl Records {
    /// The rows of the release `release_id` an edit of its metadata writes
    /// over; `None` when the library holds no such release.
    pub async fn find_release_to_edit(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseToEdit>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| release_to_edit_on(&sql, &release_id)).await
    }
}

/// What a save of a release editor's form reads beside the release as the
/// editor opens on it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseSaveFacts {
    /// The album the release is in, and the one its records file it under.
    pub filed: RecordsTargetAlbum,
    /// The ids of the tracks the save deletes, in release order.
    pub dropped: Vec<String>,
    /// The files the save adds a track for, in the order their first deleted
    /// track sat.
    pub added: Vec<DbAddedSheetFile>,
    /// The release's first medium, which a track the save adds starts on.
    pub first_medium: Option<String>,
    pub audio: DbReleaseAudio,
    /// The release's files, in the order every list of them shows.
    pub files: Vec<DbFile>,
}

/// A file a save adds a track for: its id and its name in the release.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DbAddedSheetFile {
    pub file_id: String,
    pub name: String,
}

/// One audio format as [`ReleaseAudio`] writes it.
#[derive(serde::Deserialize)]
struct FormatJson {
    id: String,
    track_id: String,
    content_type: String,
    pregap_samples: Option<u64>,
    generated_pregap_samples: Option<u64>,
    sample_rate: i64,
    bits_per_sample: Option<i64>,
    channels: i64,
    track_loudness_lufs: Option<f64>,
    track_peak_linear: Option<f64>,
    created_at: String,
}

/// One audio segment as [`ReleaseAudio`] writes it.
#[derive(serde::Deserialize)]
struct SegmentRowJson {
    id: String,
    audio_format_id: String,
    segment_index: i64,
    role: String,
    file_id: String,
    start_sample: u64,
    end_sample: Option<u64>,
    start_byte: Option<u64>,
    end_byte: Option<u64>,
    created_at: String,
}

/// When the row holding `what` was made, which its column states as `text`.
fn made_at(text: &str, what: &str) -> Result<time::UtcDateTime, DbError> {
    bae_util::timestamp::parse_rfc3339(text)
        .map_err(|_unparsed| DbError::unreadable(format!("{what}'s created_at"), text))
}

impl FormatJson {
    /// The audio format the object holds. A stored value playback can't use
    /// is an error naming the track and the column.
    fn decoded(self) -> Result<DbAudioFormat, DbError> {
        let unreadable = |reason: String| DbError::unreadable("a track's audio format", reason);
        let count = |raw: i64, column: &str| {
            super::read::track_audio_count(raw, column, &self.track_id).map_err(unreadable)
        };
        Ok(DbAudioFormat {
            sample_rate: count(self.sample_rate, "sample_rate")?,
            channels: count(self.channels, "channels")?,
            bits_per_sample: super::read::track_bit_depth(self.bits_per_sample, &self.track_id)
                .map_err(unreadable)?,
            created_at: made_at(&self.created_at, "an audio format")?,
            content_type: ContentType::from_mime(&self.content_type),
            id: self.id,
            track_id: self.track_id,
            pregap_samples: self.pregap_samples,
            generated_pregap_samples: self.generated_pregap_samples,
            track_loudness_lufs: self.track_loudness_lufs,
            track_peak_linear: self.track_peak_linear,
        })
    }
}

impl ReleaseAudio {
    /// The audio the rows hold.
    pub(super) fn decoded(self) -> Result<DbReleaseAudio, DbError> {
        let formats = rows_of::<FormatJson>(&self.audio_formats, "a release's audio formats")?
            .into_iter()
            .map(FormatJson::decoded)
            .collect::<Result<_, DbError>>()?;
        let segments = rows_of::<SegmentRowJson>(&self.audio_segments, "a release's segments")?
            .into_iter()
            .map(|segment| {
                let role = DbAudioSegmentRole::from_db_value(&segment.role)
                    .ok_or_else(|| DbError::unreadable("an audio segment's role", &segment.role))?;
                Ok(DbAudioSegment {
                    role,
                    created_at: made_at(&segment.created_at, "an audio segment")?,
                    id: segment.id,
                    audio_format_id: segment.audio_format_id,
                    segment_index: segment.segment_index,
                    file_id: segment.file_id,
                    start_sample: segment.start_sample,
                    end_sample: segment.end_sample,
                    start_byte: segment.start_byte,
                    end_byte: segment.end_byte,
                })
            })
            .collect::<Result<_, DbError>>()?;
        let carved = rows_of::<DbCarvedTrack>(&self.carved, "a release's carved tracks")?;
        Ok(DbReleaseAudio { formats, segments, carved })
    }
}

impl ReleaseSaveRows {
    /// The rows read into the values a save starts from.
    pub fn decoded(self) -> Result<DbReleaseSaveFacts, DbError> {
        let files = rows_of::<FileColumns>(&self.files, "a release's files")?
            .into_iter()
            .map(|columns| {
                file_of(columns).map_err(|(field, reason)| {
                    DbError::unreadable("a release's file", format!("{field}: {reason}"))
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(DbReleaseSaveFacts {
            filed: RecordsTargetAlbum {
                current_album_id: self.filed.current_album_id,
                target_album_id: self.filed.target_album_id,
            },
            dropped: rows_of(&self.changes.dropped, "the tracks a save deletes")?,
            added: rows_of(&self.changes.added, "the files a save adds tracks for")?,
            first_medium: self.changes.first_medium,
            audio: self.audio.decoded()?,
            files,
        })
    }
}

// The audio of one release's tracks.
#[cfg(any(test, feature = "test-utils"))]
sqlive_compose::read!(
    writer OneReleaseAudio(release: String),
    r#"
    SELECT audio.*
    FROM @crate::records::release_edit_rows::ReleaseAudio(release: :release) AS audio
"#
);

#[cfg(any(test, feature = "test-utils"))]
impl Records {
    /// The audio format of the track `track_id`; `None` when the library
    /// holds no such track, or no audio of it.
    pub async fn find_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, DbError> {
        let Some(track) = self.find_track_by_id(track_id).await? else {
            return Ok(None);
        };
        let audio = self.get_release_audio(&track.release_id).await?;
        Ok(audio.formats.into_iter().find(|format| format.track_id == track_id))
    }

    /// The segments of the audio format `format`, in order.
    pub async fn get_audio_segments_for_format(
        &self,
        format: &DbAudioFormat,
    ) -> Result<Vec<DbAudioSegment>, DbError> {
        let track =
            self.find_track_by_id(&format.track_id).await?.expect("the audio format's track");
        let mut segments = self.get_release_audio(&track.release_id).await?.segments;
        segments.retain(|segment| segment.audio_format_id == format.id);
        Ok(segments)
    }

    /// The audio of the release `release_id`'s tracks, as a save reads it.
    pub async fn get_release_audio(&self, release_id: &str) -> Result<DbReleaseAudio, DbError> {
        let statement = OneReleaseAudioParams { release: release_id.to_owned() };
        self.store
            .read(move |sql| bae_store::statement_row(&sql, &statement)?.audio.decoded())
            .await
    }
}

#[cfg(test)]
#[path = "release_edit_rows_tests.rs"]
mod tests;
