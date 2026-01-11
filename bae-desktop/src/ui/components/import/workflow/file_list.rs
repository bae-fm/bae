//! File list conversion utilities
//!
//! Converts backend scanned file types to bae-ui display types.

use bae_core::import::folder_scanner::{AudioContent, ScannedCueFlacPair, ScannedFile};
use bae_ui::display_types::{AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, FileInfo};

/// Convert from backend CategorizedFiles to UI display type
pub fn categorized_files_from_scanned(
    categorized: &bae_core::import::CategorizedFiles,
) -> CategorizedFileInfo {
    let convert = |files: &[ScannedFile]| -> Vec<FileInfo> {
        files
            .iter()
            .map(|f| {
                let format = std::path::Path::new(&f.relative_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_uppercase();
                FileInfo {
                    name: f.relative_path.clone(),
                    size: f.size,
                    format,
                }
            })
            .collect()
    };

    let convert_pair = |pair: &ScannedCueFlacPair| -> CueFlacPairInfo {
        CueFlacPairInfo {
            cue_name: pair.cue_file.relative_path.clone(),
            flac_name: pair.audio_file.relative_path.clone(),
            total_size: pair.cue_file.size + pair.audio_file.size,
            track_count: pair.track_count,
        }
    };

    let audio = match &categorized.audio {
        AudioContent::CueFlacPairs(pairs) => {
            AudioContentInfo::CueFlacPairs(pairs.iter().map(convert_pair).collect())
        }
        AudioContent::TrackFiles(tracks) => AudioContentInfo::TrackFiles(convert(tracks)),
    };

    CategorizedFileInfo {
        audio,
        artwork: convert(&categorized.artwork),
        documents: convert(&categorized.documents),
        other: convert(&categorized.other),
    }
}
