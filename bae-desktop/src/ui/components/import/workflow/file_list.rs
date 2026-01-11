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

#[cfg(test)]
mod tests {
    use super::*;
    use bae_core::import::folder_scanner::scan_for_releases;

    #[test]
    fn test_categorized_files_from_scanned_preserves_cue_flac_pairs() {
        let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("bae-core")
            .join("tests")
            .join("fixtures")
            .join("cue_flac");

        let releases = scan_for_releases(fixture_dir).unwrap();
        assert_eq!(releases.len(), 1);

        let files = categorized_files_from_scanned(&releases[0].files);

        match files.audio {
            AudioContentInfo::CueFlacPairs(pairs) => {
                assert_eq!(pairs.len(), 1, "Should have 1 CUE/FLAC pair");
                assert!(pairs[0].track_count > 0, "Should have track count");
            }
            AudioContentInfo::TrackFiles(tracks) => {
                panic!(
                    "Expected CueFlacPairs but got TrackFiles with {} files",
                    tracks.len()
                );
            }
        }
    }
}
