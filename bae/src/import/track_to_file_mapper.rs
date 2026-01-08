use crate::cue_flac::{CueFlacPair, CueFlacProcessor};
use crate::db::DbTrack;
use crate::import::types::{CueFlacMetadata, DiscoveredFile, TrackFile, TrackToFileMappingResult};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};
/// Map tracks to their source audio files using already-discovered files.
///
/// This is an analysis and validation step that runs BEFORE database insertion.
/// - For file-per-track imports, this will map tracks to individual audio files.
/// - For CUE/FLAC imports, this will use the CUE sheet to map tracks to the FLAC file that contains the album.
///
/// The data computed here is used later during import and playback.
pub async fn map_tracks_to_files(
    tracks: &[DbTrack],
    discovered_files: &[DiscoveredFile],
) -> Result<TrackToFileMappingResult, String> {
    info!(
        "Mapping {} tracks using {} pre-discovered files",
        tracks.len(),
        discovered_files.len()
    );
    let file_paths: Vec<PathBuf> = discovered_files.iter().map(|f| f.path.clone()).collect();
    let cue_flac_pairs = CueFlacProcessor::detect_cue_flac_from_paths(&file_paths)
        .map_err(|e| format!("CUE/FLAC detection failed: {}", e))?;
    if cue_flac_pairs.is_empty() {
        return map_tracks_to_individual_files(tracks, &file_paths);
    }
    info!("Found {} CUE/FLAC pairs", cue_flac_pairs.len());
    map_tracks_to_cue_flacs(tracks, cue_flac_pairs)
}
/// Map tracks to CUE/FLAC source files using CUE sheet parsing.
/// Returns track mappings AND the parsed CUE metadata for use in later stages.
fn map_tracks_to_cue_flacs(
    tracks: &[DbTrack],
    cue_flac_pairs: Vec<CueFlacPair>,
) -> Result<TrackToFileMappingResult, String> {
    let mut track_files = Vec::new();
    let mut cue_flac_metadata = HashMap::new();
    for pair in cue_flac_pairs {
        let (pair_mappings, pair_metadata) = map_tracks_to_cue_flac(&pair, tracks)?;
        track_files.extend(pair_mappings);
        cue_flac_metadata.insert(pair.flac_path.clone(), pair_metadata);
    }
    info!(
        "Created {} CUE/FLAC mappings with validated metadata",
        track_files.len()
    );
    Ok(TrackToFileMappingResult {
        track_files,
        cue_flac_metadata: Some(cue_flac_metadata),
    })
}
/// Process a single CUE/FLAC pair: parse, validate, and create track mappings.
/// Returns the track mappings and metadata for this pair.
fn map_tracks_to_cue_flac(
    pair: &CueFlacPair,
    tracks: &[DbTrack],
) -> Result<(Vec<TrackFile>, CueFlacMetadata), String> {
    debug!(
        "Processing CUE/FLAC pair: {} + {}",
        pair.flac_path.display(),
        pair.cue_path.display()
    );
    let cue_sheet = CueFlacProcessor::parse_cue_sheet(&pair.cue_path)
        .map_err(|e| format!("Failed to parse CUE sheet: {}", e))?;
    debug!("CUE sheet contains {} tracks", cue_sheet.tracks.len());
    if cue_sheet.tracks.is_empty() {
        return Err(format!(
            "CUE sheet '{}' contains no tracks. Check CUE file format.",
            pair.cue_path.display(),
        ));
    }
    if cue_sheet.tracks.len() != tracks.len() {
        return Err(format!(
            "Track count mismatch: CUE sheet has {} tracks but Discogs has {} tracks",
            cue_sheet.tracks.len(),
            tracks.len(),
        ));
    }
    let mut mappings = Vec::new();
    for (index, cue_track) in cue_sheet.tracks.iter().enumerate() {
        if let Some(db_track) = tracks.get(index) {
            mappings.push(TrackFile {
                db_track_id: db_track.id.clone(),
                file_path: pair.flac_path.clone(),
            });
            debug!(
                "Mapped CUE track '{}' to DB track '{}'",
                cue_track.title, db_track.title
            );
        }
    }
    let metadata = CueFlacMetadata {
        cue_sheet,
        cue_path: pair.cue_path.clone(),
        flac_path: pair.flac_path.clone(),
    };
    Ok((mappings, metadata))
}
/// Map tracks to individual audio files using simple name-based matching
fn map_tracks_to_individual_files(
    tracks: &[DbTrack],
    file_paths: &[PathBuf],
) -> Result<TrackToFileMappingResult, String> {
    let audio_files = filter_audio_files(file_paths);
    if audio_files.is_empty() {
        return Err("No audio files found in discovered files".to_string());
    }
    if audio_files.len() != tracks.len() {
        return Err(format!(
            "Track count mismatch: found {} audio files but have {} tracks",
            audio_files.len(),
            tracks.len(),
        ));
    }
    let formats: std::collections::HashSet<_> = audio_files
        .iter()
        .filter_map(|p| p.extension())
        .filter_map(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .collect();
    if formats.len() > 1 {
        return Err(format!(
            "Mixed audio formats detected: {:?}. All tracks should be in the same format",
            formats,
        ));
    }
    let mut mappings = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        if let Some(audio_file) = audio_files.get(index) {
            mappings.push(TrackFile {
                db_track_id: track.id.clone(),
                file_path: audio_file.clone(),
            });
        }
    }
    info!("Mapped {} tracks to source files", mappings.len());
    Ok(TrackToFileMappingResult {
        track_files: mappings,
        cue_flac_metadata: None,
    })
}
/// Filter audio files from a list of paths
fn filter_audio_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let audio_extensions = ["flac"];
    let mut audio_files: Vec<PathBuf> = paths
        .iter()
        .filter(|path| {
            if let Some(extension) = path.extension() {
                if let Some(ext_str) = extension.to_str() {
                    return audio_extensions.contains(&ext_str.to_lowercase().as_str());
                }
            }
            false
        })
        .cloned()
        .collect();
    audio_files.sort();
    debug!("Filtered {} audio files", audio_files.len());
    audio_files
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ImportStatus;
    use chrono::Utc;
    fn create_test_tracks(count: usize) -> Vec<DbTrack> {
        (0..count)
            .map(|i| DbTrack {
                id: format!("track-{}", i),
                release_id: "release-1".to_string(),
                title: format!("Track {}", i + 1),
                disc_number: None,
                track_number: Some((i + 1) as i32),
                duration_ms: None,
                discogs_position: Some((i + 1).to_string()),
                import_status: ImportStatus::Queued,
                created_at: Utc::now(),
            })
            .collect()
    }
    fn create_discovered_files(paths: Vec<&str>) -> Vec<DiscoveredFile> {
        paths
            .into_iter()
            .map(|p| DiscoveredFile {
                path: PathBuf::from(p),
                size: 1024 * 1024,
            })
            .collect()
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_individual_files() {
        let tracks = create_test_tracks(3);
        let discovered_files = create_discovered_files(vec![
            "/album/01-track1.flac",
            "/album/02-track2.flac",
            "/album/03-track3.flac",
        ]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_ok());
        let mapping_result = result.unwrap();
        let mappings = &mapping_result.track_files;
        assert_eq!(mappings.len(), 3);
        assert_eq!(mappings[0].db_track_id, "track-0");
        assert_eq!(
            mappings[0].file_path,
            PathBuf::from("/album/01-track1.flac")
        );
        assert_eq!(mappings[1].db_track_id, "track-1");
        assert_eq!(
            mappings[1].file_path,
            PathBuf::from("/album/02-track2.flac")
        );
        assert_eq!(mappings[2].db_track_id, "track-2");
        assert_eq!(
            mappings[2].file_path,
            PathBuf::from("/album/03-track3.flac")
        );
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_no_audio_files() {
        let tracks = create_test_tracks(2);
        let discovered_files =
            create_discovered_files(vec!["/album/cover.jpg", "/album/readme.txt"]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No audio files found"));
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_more_tracks_than_files() {
        let tracks = create_test_tracks(5);
        let discovered_files =
            create_discovered_files(vec!["/album/01.flac", "/album/02.flac", "/album/03.flac"]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Track count mismatch"));
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_more_files_than_tracks() {
        let tracks = create_test_tracks(2);
        let discovered_files = create_discovered_files(vec![
            "/album/01.flac",
            "/album/02.flac",
            "/album/03.flac",
            "/album/04.flac",
        ]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Track count mismatch"));
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_mixed_formats() {
        let tracks = create_test_tracks(4);
        let discovered_files = create_discovered_files(vec![
            "/album/cover.jpg",
            "/album/track1.mp3",
            "/album/track2.flac",
            "/album/track3.wav",
            "/album/track4.m4a",
            "/album/readme.txt",
        ]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_err());
        // Non-FLAC files are filtered out, so we get a track count mismatch
        assert!(result.unwrap_err().contains("Track count mismatch"));
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_cue_flac() {
        let tracks = create_test_tracks(10);
        let discovered_files = create_discovered_files(vec![
            "/album/album.flac",
            "/album/album.cue",
            "/album/cover.jpg",
        ]);
        let result = map_tracks_to_files(&tracks, &discovered_files).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Failed to parse CUE sheet") || err.contains("CUE"),
            "Expected CUE parsing error, got: {}",
            err,
        );
    }
}
