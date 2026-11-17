//! The candidate's files as its pane lists them, read in one statement over
//! the scan's rows: each file with its role, how many tracks each track sheet
//! lays out, which images a cover picker can show, how many tracks the folder
//! plays, and one summary of its audio.

use super::*;
use crate::folder_scanner::FileVersion;
use crate::{PaneFile, PaneFileRole, PaneFiles};
use bae_records::source_audio::{source_audio_summary_sql, SourceAudioSummary};
use bae_store::SchemaFunction;
use bae_types::audio_format::AudioFormat;
use bae_util::content_type::ContentType;
use bae_util::content_type_hint::ContentTypeHint;

/// `bae_supported_cover`: whether a cover picker can show the image at the
/// path its one argument names, as its extension says.
pub(super) static SUPPORTED_COVER: SchemaFunction = SchemaFunction {
    name: "bae_supported_cover",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let path = arguments.get::<String>(0)?;
        Ok(rusqlite::types::Value::Integer(i64::from(ContentTypeHint::path_is_supported_cover(
            std::path::Path::new(&path),
        ))))
    },
};

/// The pane's files of the candidate the statement binds as `:key`, as one
/// JSON document.
///
/// An audio file plays as a CUE image when the scan bound it to a sheet that
/// lays out the folder's tracks: a sheet whose binding resolved, which is on a
/// disc and has an audio track. The audio summary is
/// [`source_audio_summary_sql`] over the folder's audio files.
fn pane_files_sql() -> String {
    format!(
        "
WITH
candidate AS (
    SELECT watched_folder_path, path FROM scan_candidate WHERE path = :key LIMIT 1
),
f AS (
    SELECT file.* FROM scan_candidate_file AS file
    JOIN candidate
        ON file.watched_folder_path = candidate.watched_folder_path
       AND file.candidate_path = candidate.path
),
sheet_tracks (relative_path, count) AS (
    SELECT track.sheet_relative_path, COUNT(*) FROM scan_cue_track AS track
    JOIN candidate
        ON track.watched_folder_path = candidate.watched_folder_path
       AND track.candidate_path = candidate.path
    WHERE track.mode = 'audio'
    GROUP BY track.sheet_relative_path
),
carved (relative_path) AS (
    SELECT DISTINCT bound.audio_relative_path FROM scan_sheet_audio_file AS bound
    JOIN candidate
        ON bound.watched_folder_path = candidate.watched_folder_path
       AND bound.candidate_path = candidate.path
    JOIN f AS sheet ON sheet.relative_path = bound.sheet_relative_path
    JOIN sheet_tracks ON sheet_tracks.relative_path = sheet.relative_path
    WHERE sheet.sheet_binding = 'resolved' AND sheet.sheet_disc = 'disc'
),
entries AS (
    SELECT f.position, f.role,
           f.role = 'artwork' AND bae_supported_cover(f.relative_path) AS cover,
           json_object(
               'name', f.relative_path, 'size', f.size, 'dir_prefix', f.dir_prefix,
               'file_name', f.file_name, 'path', hex(f.absolute_path),
               'modified_at_ns', f.modified_at_ns, 'role', f.role,
               'content_type', f.audio_content_type, 'sample_rate_hz', f.audio_sample_rate_hz,
               'bits_per_sample', f.audio_bits_per_sample, 'bitrate_kbps', f.audio_bitrate_kbps,
               'channels', f.audio_channels,
               'sheet_tracks', CASE WHEN f.role = 'track_sheet'
                                    THEN COALESCE(sheet_tracks.count, 0) END) AS json
    FROM f LEFT JOIN sheet_tracks ON sheet_tracks.relative_path = f.relative_path
)
SELECT json_object(
    'files', json((SELECT json_group_array(json(json) ORDER BY position) FROM entries)),
    'cover_files', json((SELECT json_group_array(json(json) ORDER BY position)
                         FROM entries WHERE cover)),
    'images', json((SELECT json_group_array(json(json) ORDER BY position)
                    FROM entries WHERE role = 'artwork')),
    'track_count', (SELECT COUNT(*) FROM scan_candidate_unit AS unit
                    JOIN candidate
                        ON unit.watched_folder_path = candidate.watched_folder_path
                       AND unit.candidate_path = candidate.path),
    'source_audio', {source_audio})",
        source_audio = source_audio_summary_sql(
            "SELECT f.position,
                    CASE WHEN carved.relative_path IS NULL THEN 'file' ELSE 'cue' END AS layout,
                    f.audio_content_type AS content_type,
                    f.audio_sample_rate_hz AS sample_rate_hz,
                    f.audio_bits_per_sample AS bits_per_sample,
                    f.audio_bitrate_kbps AS bitrate_kbps, f.audio_channels AS channels,
                    f.audio_duration_ms AS duration_ms
             FROM f LEFT JOIN carved ON carved.relative_path = f.relative_path
             WHERE f.role = 'audio'"
        ),
    )
}

#[derive(serde::Deserialize)]
struct Document {
    files: Vec<FileJson>,
    cover_files: Vec<FileJson>,
    images: Vec<FileJson>,
    track_count: u32,
    #[serde(deserialize_with = "SourceAudioSummary::deserialize_stored")]
    source_audio: Option<SourceAudioSummary>,
}

#[derive(serde::Deserialize)]
struct FileJson {
    name: String,
    size: u64,
    dir_prefix: Option<String>,
    file_name: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
    modified_at_ns: i64,
    role: String,
    content_type: Option<String>,
    sample_rate_hz: Option<i64>,
    bits_per_sample: Option<i64>,
    bitrate_kbps: Option<i64>,
    channels: Option<i64>,
    sheet_tracks: Option<u32>,
}

/// The display name of the codec a stored content type names.
fn codec(content_type: &str) -> String {
    ContentType::from_mime(content_type).display_name().to_string()
}

impl FileJson {
    fn image(self) -> crate::mapping::MappingImage {
        crate::mapping::MappingImage {
            file_id: self.name,
            name: self.file_name,
            file: FileVersion {
                path: self.path,
                size: self.size,
                modified_at_ns: self.modified_at_ns,
            },
        }
    }

    fn file(self) -> Result<PaneFile, DbError> {
        let audio_format = match (self.content_type.as_deref(), self.sample_rate_hz, self.channels)
        {
            (Some(content_type), Some(sample_rate_hz), Some(channels)) => Some(AudioFormat {
                codec: codec(content_type),
                sample_rate_hz,
                bits_per_sample: self.bits_per_sample,
                bitrate_kbps: self.bitrate_kbps,
                channels,
            }),
            _ => None,
        };
        let role = match self.role.as_str() {
            "audio" => PaneFileRole::Audio,
            "track_sheet" => PaneFileRole::TrackSheet {
                track_count: self.sheet_tracks.ok_or_else(|| {
                    DbError::unreadable(format!("the tracks of sheet {}", self.name), None::<u32>)
                })?,
            },
            "artwork" => PaneFileRole::Artwork,
            "damaged_image" => PaneFileRole::DamagedImage,
            "document" => PaneFileRole::Document,
            "other" => PaneFileRole::Other,
            other => return Err(DbError::unreadable("scan_candidate_file.role", other)),
        };
        Ok(PaneFile {
            name: self.name,
            size: self.size,
            dir_prefix: self.dir_prefix,
            file_name: self.file_name,
            audio_format,
            role,
        })
    }
}

/// The pane's files of the candidate `key`.
pub(super) fn load_pane_files_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<PaneFiles, DbError> {
    let json: String =
        sql.query_row(&pane_files_sql(), named_params! { ":key": key }, |row| row.get(0))?;
    let document: Document = serde_json::from_str(&json)
        .map_err(|error| DbError::unreadable_json("the pane's files", &json, error))?;
    Ok(PaneFiles {
        files: document.files.into_iter().map(FileJson::file).collect::<Result<_, _>>()?,
        cover_files: document
            .cover_files
            .into_iter()
            .map(FileJson::file)
            .collect::<Result<_, _>>()?,
        images: document.images.into_iter().map(FileJson::image).collect(),
        track_count: document.track_count,
        source_audio: document.source_audio,
    })
}
