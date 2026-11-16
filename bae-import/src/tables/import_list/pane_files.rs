//! The candidate's files as its pane lists them, read in one statement over
//! the scan's rows: each file with its role, how many tracks each track sheet
//! lays out, which images a cover picker can show, how many tracks the folder
//! plays, and one summary of its audio.

use super::*;
use crate::cover_art::CoverChoice;
use crate::folder_scanner::FileVersion;
use crate::{PaneFile, PaneFileRole, PaneFiles};
use bae_records::source_audio::{
    SourceAudioDescriptor, SourceAudioDifference, SourceAudioLayout, SourceAudioSummary,
};
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
/// An audio file plays as a CUE image when a sheet that lays out the folder's
/// tracks (bound, on a disc, with an audio track) is bound to it. The audio
/// summary is one format when every audio file has the same layout, codec,
/// rate, bit depth and channels, with the bitrate a lossy format states as
/// each file's weighed by how long it plays; otherwise it names each fact the
/// files disagree on, its values in the order the files first show them.
const PANE_FILES_SQL: &str = "
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
                                    THEN COALESCE(sheet_tracks.count, 0) END,
               'cover', f.role = 'artwork' AND bae_supported_cover(f.relative_path)) AS json
    FROM f LEFT JOIN sheet_tracks ON sheet_tracks.relative_path = f.relative_path
),
audio AS (
    SELECT f.position,
           CASE WHEN carved.relative_path IS NULL THEN 'file' ELSE 'cue' END AS layout,
           f.audio_content_type AS content_type, f.audio_sample_rate_hz AS rate,
           f.audio_bits_per_sample AS bits, f.audio_bitrate_kbps AS kbps,
           f.audio_channels AS channels, f.audio_duration_ms AS ms
    FROM f LEFT JOIN carved ON carved.relative_path = f.relative_path
    WHERE f.role = 'audio'
),
rated AS (SELECT kbps, ms FROM audio WHERE kbps IS NOT NULL)
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
    'source_audio', CASE WHEN EXISTS (SELECT 1 FROM audio) THEN json_object(
        'uniform', (SELECT COUNT(*) FROM (SELECT DISTINCT layout, content_type, rate, bits,
                                                          channels FROM audio)) = 1,
        'first', json((SELECT json_object('layout', layout, 'content_type', content_type,
                                          'sample_rate_hz', rate, 'bits_per_sample', bits,
                                          'channels', channels)
                       FROM audio ORDER BY position LIMIT 1)),
        'bitrate_kbps', (SELECT CASE WHEN COUNT(*) = 0 THEN NULL
                                     WHEN SUM(ms) > 0 THEN (SUM(kbps * ms) + SUM(ms) / 2) / SUM(ms)
                                     ELSE (SUM(kbps) + COUNT(*) / 2) / COUNT(*) END
                         FROM rated),
        'layouts', json((SELECT CASE WHEN COUNT(*) >= 2 THEN json_group_array(layout ORDER BY first) END
                    FROM (SELECT layout, MIN(position) AS first FROM audio GROUP BY layout))),
        'content_types', json((SELECT CASE WHEN COUNT(*) >= 2
                                      THEN json_group_array(content_type ORDER BY first) END
                          FROM (SELECT content_type, MIN(position) AS first FROM audio
                                GROUP BY content_type))),
        'sample_rates_hz', json((SELECT CASE WHEN COUNT(*) >= 2
                                        THEN json_group_array(rate ORDER BY first) END
                            FROM (SELECT rate, MIN(position) AS first FROM audio GROUP BY rate))),
        'bits_per_sample', json((SELECT CASE WHEN COUNT(*) >= 2
                                        THEN json_group_array(bits ORDER BY first) END
                            FROM (SELECT bits, MIN(position) AS first FROM audio
                                  WHERE bits IS NOT NULL GROUP BY bits))),
        'channels', json((SELECT CASE WHEN COUNT(*) >= 2
                                 THEN json_group_array(channels ORDER BY first) END
                     FROM (SELECT channels, MIN(position) AS first FROM audio
                           GROUP BY channels)))) END)";

#[derive(serde::Deserialize)]
struct Document {
    files: Vec<FileJson>,
    cover_files: Vec<FileJson>,
    images: Vec<FileJson>,
    track_count: u32,
    source_audio: Option<SummaryJson>,
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
    cover: i64,
}

#[derive(serde::Deserialize)]
struct FormatJson {
    layout: String,
    content_type: String,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    channels: i64,
}

#[derive(serde::Deserialize)]
struct SummaryJson {
    uniform: i64,
    first: FormatJson,
    bitrate_kbps: Option<i64>,
    layouts: Option<Vec<String>>,
    content_types: Option<Vec<String>>,
    sample_rates_hz: Option<Vec<i64>>,
    bits_per_sample: Option<Vec<i64>>,
    channels: Option<Vec<i64>>,
}

/// The display name of the codec a stored content type names.
fn codec(content_type: &str) -> String {
    ContentType::from_mime(content_type).display_name().to_string()
}

fn layout(stored: &str) -> Result<SourceAudioLayout, DbError> {
    match stored {
        "file" => Ok(SourceAudioLayout::File),
        "cue" => Ok(SourceAudioLayout::Cue),
        other => Err(DbError::unreadable("an audio file's layout", other)),
    }
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
            "artwork" => PaneFileRole::Artwork {
                choice: (self.cover != 0).then(|| CoverChoice::Local {
                    file_id: self.name.clone(),
                    file: FileVersion {
                        path: self.path.clone(),
                        size: self.size,
                        modified_at_ns: self.modified_at_ns,
                    },
                }),
            },
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

impl SummaryJson {
    fn summary(self) -> Result<SourceAudioSummary, DbError> {
        if self.uniform != 0 {
            return Ok(SourceAudioSummary::Uniform {
                descriptor: SourceAudioDescriptor {
                    layout: layout(&self.first.layout)?,
                    format: AudioFormat {
                        codec: codec(&self.first.content_type),
                        sample_rate_hz: self.first.sample_rate_hz,
                        bits_per_sample: self.first.bits_per_sample,
                        bitrate_kbps: self.bitrate_kbps,
                        channels: self.first.channels,
                    },
                },
            });
        }
        let layouts = self
            .layouts
            .map(|layouts| layouts.iter().map(|stored| layout(stored)).collect())
            .transpose()?
            .map(|layouts| SourceAudioDifference::Layout { layouts });
        let codecs = self.content_types.map(|content_types| SourceAudioDifference::Codec {
            codecs: content_types.iter().map(|content_type| codec(content_type)).collect(),
        });
        let differences = [
            layouts,
            codecs,
            self.sample_rates_hz
                .map(|sample_rates_hz| SourceAudioDifference::SampleRate { sample_rates_hz }),
            self.bits_per_sample
                .map(|bits_per_sample| SourceAudioDifference::BitDepth { bits_per_sample }),
            self.channels.map(|channels| SourceAudioDifference::Channels { channels }),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(SourceAudioSummary::Mixed { differences })
    }
}

/// The pane's files of the candidate `key`.
pub(super) fn load_pane_files_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<PaneFiles, DbError> {
    let json: String =
        sql.query_row(PANE_FILES_SQL, named_params! { ":key": key }, |row| row.get(0))?;
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
        source_audio: document.source_audio.map(SummaryJson::summary).transpose()?,
    })
}
