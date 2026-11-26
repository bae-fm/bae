//! The candidate's files as its pane lists them, read in one statement over
//! the scan's rows: each file with its role, how many tracks each track sheet
//! lays out, which images a cover picker can show, how many tracks the folder
//! plays, and one summary of its audio.

use super::*;
use crate::folder_scanner::FileVersion;
use crate::{PaneFile, PaneFileRole, PaneFiles};
use bae_records::source_audio::SourceAudioSummary;
use bae_types::audio_format::AudioFormat;
use bae_util::content_type::ContentType;
use bae_util::content_type_hint::ContentTypeHint;

/// `bae_supported_cover`: whether a cover picker can show the image at the
/// path its one argument names, as its extension says.
pub(crate) fn supported_cover(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let path = arguments.get::<String>(0)?;
    Ok(rusqlite::types::Value::Integer(i64::from(ContentTypeHint::path_is_supported_cover(
        std::path::Path::new(&path),
    ))))
}

#[derive(serde::Deserialize)]
struct Document {
    files: Vec<FileJson>,
    cover_files: Vec<FileJson>,
    images: Option<Vec<FileJson>>,
    track_count: u32,
    #[serde(deserialize_with = "SourceAudioSummary::deserialize_stored")]
    source_audio: Option<SourceAudioSummary>,
}

#[derive(serde::Deserialize)]
struct FileJson {
    name: String,
    size: u64,
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
            file_name: self.file_name,
            audio_format,
            role,
        })
    }
}

/// The pane's files that the statement wrote as one JSON document.
pub(super) fn pane_files_of_json(json: &str) -> Result<PaneFiles, DbError> {
    let document: Document = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("the pane's files", json, error))?;
    Ok(PaneFiles {
        files: document.files.into_iter().map(FileJson::file).collect::<Result<_, _>>()?,
        cover_files: document
            .cover_files
            .into_iter()
            .map(FileJson::file)
            .collect::<Result<_, _>>()?,
        images: document.images.map(|images| images.into_iter().map(FileJson::image).collect()),
        track_count: document.track_count,
        source_audio: document.source_audio,
    })
}
