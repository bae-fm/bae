//! Recursive folder scanner with leaf detection for imports.
//!
//! Supports three folder structures:
//! 1. Single release (flat) - audio files in root, optional artwork subfolders
//! 2. Single release (multi-disc) - disc subfolders with audio, optional artwork
//! 3. Collections - recursive tree where leaves are single releases
use super::file_validation;
use crate::cue_flac::CueFlacProcessor;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
const MAX_RECURSION_DEPTH: usize = 10;
const AUDIO_EXTENSIONS: &[&str] = &["flac"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];
const DOCUMENT_EXTENSIONS: &[&str] = &["cue", "log", "txt", "nfo", "m3u", "m3u8"];
/// A file discovered during folder scanning
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Full path to the file
    pub path: PathBuf,
    /// Relative path from release root (for display)
    pub relative_path: String,
    /// File size in bytes
    pub size: u64,
}
/// A CUE/FLAC pair representing a single disc with track count
#[derive(Debug, Clone)]
pub struct ScannedCueFlacPair {
    /// The CUE sheet file
    pub cue_file: ScannedFile,
    /// The audio file
    pub audio_file: ScannedFile,
    /// Number of tracks defined in the CUE sheet
    pub track_count: usize,
}
/// The audio content type of a release - mutually exclusive
#[derive(Debug, Clone)]
pub enum AudioContent {
    /// One or more CUE/FLAC pairs (multi-disc releases can have multiple)
    CueFlacPairs(Vec<ScannedCueFlacPair>),
    /// Individual track files (file-per-track releases)
    TrackFiles(Vec<ScannedFile>),
}
impl Default for AudioContent {
    fn default() -> Self {
        AudioContent::TrackFiles(Vec::new())
    }
}
/// Files from a release, pre-categorized by type
#[derive(Debug, Clone, Default)]
pub struct CategorizedFiles {
    /// Audio content - either CUE/FLAC pairs or individual track files
    pub audio: AudioContent,
    /// Artwork/image files (.jpg, .png, etc.)
    pub artwork: Vec<ScannedFile>,
    /// Document files (.log, .txt, .nfo) - CUE files in pairs are NOT included here
    pub documents: Vec<ScannedFile>,
    /// Number of audio files that are corrupt or incomplete (0-byte, bad headers,
    /// or truncated). Not included in `audio` — counted here so the UI can
    /// explain why a release is incomplete.
    pub bad_audio_count: usize,
    /// Number of image files that are corrupt (0-byte or bad magic bytes).
    /// Not included in `artwork`. Any bad file — audio or image — blocks import.
    pub bad_image_count: usize,
}
/// A detected candidate (leaf directory) in a collection.
/// Called "candidate" because it hasn't been identified yet.
#[derive(Debug, Clone)]
pub struct DetectedCandidate {
    /// Root path of this release
    pub path: PathBuf,
    /// Display name (derived from folder name)
    pub name: String,
    /// Pre-categorized files for this release
    pub files: CategorizedFiles,
}
/// Check if a file is an audio file based on extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}
/// Check if a file is an image/artwork file
fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}
/// Check if a file is a document file (.cue, .log, .txt, .nfo)
fn is_document_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| DOCUMENT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}
/// Check if a file is a CUE file
fn is_cue_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase() == "cue")
        .unwrap_or(false)
}
/// Check if a file is noise (.DS_Store, Thumbs.db, etc.)
fn is_noise_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name == ".DS_Store" || name == "Thumbs.db" || name == "desktop.ini")
        .unwrap_or(false)
}
/// Check if a directory contains audio files directly (by extension).
///
/// This is used for tree-structure detection (leaf vs collection), not for
/// validation. Even a directory with only corrupt FLAC files should be detected
/// as a candidate — the incompleteness is reported at the candidate level.
/// Only skips 0-byte files since those are empty placeholders.
fn has_audio_files(dir: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_audio_file(&path) {
            if let Ok(metadata) = entry.metadata() {
                if metadata.len() > 0 {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
/// Check if any subdirectory contains audio files
fn has_subdirs_with_audio(dir: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_audio_files(&path)? {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Check if any subdirectory has its own subdirectories with audio files
fn has_nested_audio_dirs(dir: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_subdirs_with_audio(&path)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Find the longest common prefix of a list of strings (case-insensitive)
fn longest_common_prefix(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let first = names[0].to_lowercase();
    let mut prefix_len = first.len();

    for name in &names[1..] {
        let name_lower = name.to_lowercase();
        prefix_len = first
            .chars()
            .zip(name_lower.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
        if prefix_len == 0 {
            break;
        }
    }

    first[..prefix_len].to_string()
}

/// Maximum length for a folder name to be considered a "disc folder"
/// Album titles are typically longer than this
const MAX_DISC_FOLDER_NAME_LENGTH: usize = 15;

/// Check if all audio-containing subdirectories look like disc folders.
/// Uses a heuristic: disc folders are SHORT and share a common prefix.
fn subdirs_are_disc_folders(dir: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;
    let mut subdir_names: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_audio_files(&path)? {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            subdir_names.push(name);
        }
    }

    if subdir_names.is_empty() {
        return Ok(false);
    }

    // All just numbers? (1, 2, 3 or 01, 02, 03)
    if subdir_names
        .iter()
        .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        return Ok(true);
    }

    // Check if names are short (disc folders are typically short)
    let all_short = subdir_names
        .iter()
        .all(|n| n.len() <= MAX_DISC_FOLDER_NAME_LENGTH);
    if !all_short {
        return Ok(false);
    }

    // Check for a meaningful common prefix (at least 2 chars)
    let prefix = longest_common_prefix(&subdir_names);
    Ok(prefix.len() >= 2)
}

/// Determine if a directory is a leaf (single release).
///
/// A directory is a leaf if:
/// - Has FLAC audio files directly in it, OR
/// - Has subdirectories that look like disc folders (CD1, Disc 2, etc.) containing audio
fn is_leaf_directory(dir: &Path) -> Result<bool, String> {
    if has_audio_files(dir)? {
        debug!("Directory {:?} is a leaf (has audio files)", dir);
        return Ok(true);
    }
    // Only treat as multi-disc if subdirs look like disc folders (CD1, Disc 2, etc.)
    // This prevents album collections from being treated as a single release
    if subdirs_are_disc_folders(dir)? && !has_nested_audio_dirs(dir)? {
        debug!(
            "Directory {:?} is a leaf (has disc subfolders with audio)",
            dir
        );
        return Ok(true);
    }
    debug!("Directory {:?} is not a leaf", dir);
    Ok(false)
}
/// Recursively scan for release leaves in a folder tree, emitting as found.
fn scan_recursive_with_callback<F>(
    dir: &Path,
    depth: usize,
    on_candidate: &mut F,
) -> Result<(), String>
where
    F: FnMut(DetectedCandidate),
{
    if depth > MAX_RECURSION_DEPTH {
        warn!(
            "Max recursion depth {} reached at {:?}, stopping",
            MAX_RECURSION_DEPTH, dir
        );
        return Ok(());
    }
    if is_leaf_directory(dir)? {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        info!("Found candidate leaf: {:?}", dir);
        let files = collect_release_files(dir)?;
        on_candidate(DetectedCandidate {
            path: dir.to_path_buf(),
            name,
            files,
        });
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_recursive_with_callback(&path, depth + 1, on_candidate)?;
        }
    }
    Ok(())
}
/// Scan a folder for candidates and invoke callback as each is found.
pub fn scan_for_candidates_with_callback<F>(
    root: PathBuf,
    mut on_candidate: F,
) -> Result<(), String>
where
    F: FnMut(DetectedCandidate),
{
    info!("Scanning for candidates in: {:?}", root);
    scan_recursive_with_callback(&root, 0, &mut on_candidate)?;
    Ok(())
}
/// Collect all files from a release directory and categorize them
///
/// This collects files recursively within a single release, preserving relative paths,
/// and categorizes them into audio (CUE/FLAC pairs or track files), artwork, and documents.
/// Unrecognized file types are ignored.
pub fn collect_release_files(release_root: &Path) -> Result<CategorizedFiles, String> {
    let mut all_audio: Vec<ScannedFile> = Vec::new();
    let mut all_cue: Vec<ScannedFile> = Vec::new();
    let mut artwork: Vec<ScannedFile> = Vec::new();
    let mut documents: Vec<ScannedFile> = Vec::new();
    let mut bad_audio_count: usize = 0;
    let mut bad_image_count: usize = 0;
    collect_files_into_vectors(
        release_root,
        release_root,
        &mut all_audio,
        &mut all_cue,
        &mut artwork,
        &mut documents,
        &mut bad_audio_count,
        &mut bad_image_count,
    )?;
    let audio_paths: Vec<PathBuf> = all_audio.iter().map(|f| f.path.clone()).collect();
    let cue_paths: Vec<PathBuf> = all_cue.iter().map(|f| f.path.clone()).collect();
    let all_paths: Vec<PathBuf> = audio_paths
        .iter()
        .chain(cue_paths.iter())
        .cloned()
        .collect();
    let detected_pairs = CueFlacProcessor::detect_cue_flac_from_paths(&all_paths)
        .map_err(|e| format!("CUE/FLAC detection failed: {}", e))?;
    let audio = if !detected_pairs.is_empty() {
        let mut pairs = Vec::new();
        let mut used_audio_paths = std::collections::HashSet::new();
        let mut used_cue_paths = std::collections::HashSet::new();
        for pair in detected_pairs {
            let cue_file = all_cue
                .iter()
                .find(|f| f.path == pair.cue_path)
                .cloned()
                .ok_or_else(|| format!("CUE file not found: {:?}", pair.cue_path))?;
            let audio_file = all_audio
                .iter()
                .find(|f| f.path == pair.flac_path)
                .cloned()
                .ok_or_else(|| format!("Audio file not found: {:?}", pair.flac_path))?;
            let track_count = match CueFlacProcessor::parse_cue_sheet(&pair.cue_path) {
                Ok(cue_sheet) => cue_sheet.tracks.len(),
                Err(e) => {
                    warn!("Failed to parse CUE sheet {:?}: {}", pair.cue_path, e);
                    0
                }
            };
            used_audio_paths.insert(pair.flac_path);
            used_cue_paths.insert(pair.cue_path);
            pairs.push(ScannedCueFlacPair {
                cue_file,
                audio_file,
                track_count,
            });
        }
        // Unused audio files (not part of a CUE/FLAC pair) are ignored
        for cue in all_cue {
            if !used_cue_paths.contains(&cue.path) {
                documents.push(cue);
            }
        }
        pairs.sort_by(|a, b| a.cue_file.relative_path.cmp(&b.cue_file.relative_path));
        AudioContent::CueFlacPairs(pairs)
    } else {
        documents.extend(all_cue);
        let mut tracks = all_audio;
        tracks.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        AudioContent::TrackFiles(tracks)
    };
    artwork.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    documents.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(CategorizedFiles {
        audio,
        artwork,
        documents,
        bad_audio_count,
        bad_image_count,
    })
}
/// Recursively collect files into separate vectors by type
fn collect_files_into_vectors(
    current_dir: &Path,
    release_root: &Path,
    audio: &mut Vec<ScannedFile>,
    cue: &mut Vec<ScannedFile>,
    artwork: &mut Vec<ScannedFile>,
    documents: &mut Vec<ScannedFile>,
    bad_audio_count: &mut usize,
    bad_image_count: &mut usize,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read dir {:?}: {}", current_dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files and directories (e.g. .DS_Store), but allow .bae/
        // which stores bae-managed files like downloaded cover art
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.') && n != ".bae")
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_file() {
            if is_noise_file(&path) {
                continue;
            }
            let size = entry
                .metadata()
                .map_err(|e| format!("Failed to read metadata for {:?}: {}", path, e))?
                .len();

            if is_audio_file(&path) {
                if size == 0 || !file_validation::is_valid_flac(&path).unwrap_or(false) {
                    *bad_audio_count += 1;
                    continue;
                }
            } else if is_image_file(&path)
                && (size == 0 || !file_validation::is_valid_image(&path).unwrap_or(false))
            {
                *bad_image_count += 1;
                continue;
            }

            let relative_path = path
                .strip_prefix(release_root)
                .map_err(|e| format!("Failed to strip prefix: {}", e))?
                .to_string_lossy()
                .to_string();
            let file = ScannedFile {
                path: path.clone(),
                relative_path,
                size,
            };
            if is_audio_file(&path) {
                audio.push(file);
            } else if is_cue_file(&path) {
                cue.push(file);
            } else if is_image_file(&path) {
                artwork.push(file);
            } else if is_document_file(&path) {
                documents.push(file);
            }
            // Other file types are ignored
        } else if path.is_dir() {
            collect_files_into_vectors(
                &path,
                release_root,
                audio,
                cue,
                artwork,
                documents,
                bad_audio_count,
                bad_image_count,
            )?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid FLAC header (42 bytes) with total_samples=0 (unknown length).
    /// This passes is_valid_flac without needing a realistic file size.
    fn fake_flac() -> Vec<u8> {
        let mut buf = vec![
            b'f', b'L', b'a', b'C', // magic
            0x00, 0x00, 0x00, 34, // STREAMINFO block: type=0, length=34
        ];
        // STREAMINFO: 34 bytes, all zeros → sample_rate=0 so size check is skipped
        buf.extend_from_slice(&[0u8; 34]);
        buf
    }

    #[test]
    fn test_is_audio_file() {
        assert!(is_audio_file(Path::new("track.flac")));
        assert!(is_audio_file(Path::new("track.FLAC")));
        assert!(!is_audio_file(Path::new("track.mp3")));
        assert!(!is_audio_file(Path::new("cover.jpg")));
        assert!(!is_audio_file(Path::new("notes.txt")));
    }
    #[test]
    fn test_is_cue_file() {
        assert!(is_cue_file(Path::new("album.cue")));
        assert!(is_cue_file(Path::new("album.CUE")));
        assert!(!is_cue_file(Path::new("album.flac")));
    }

    #[test]
    fn test_collect_release_files_skips_hidden_but_allows_bae() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // Create visible files
        std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
        std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        // Create hidden file that should be ignored
        std::fs::write(root.join(".DS_Store"), b"mac junk").unwrap();

        // Create .bae directory with cover art (should be included)
        let bae_dir = root.join(".bae");
        std::fs::create_dir(&bae_dir).unwrap();
        std::fs::write(bae_dir.join("cover-mb.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        // Non-image files in .bae/ are ignored (unrecognized extension)
        std::fs::write(bae_dir.join("cache.db"), b"cache data").unwrap();

        // Create another hidden directory that should be ignored
        let other_hidden = root.join(".hidden");
        std::fs::create_dir(&other_hidden).unwrap();
        std::fs::write(other_hidden.join("secret.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        let files = collect_release_files(root).unwrap();

        // Check audio files
        let audio_paths: Vec<_> = match &files.audio {
            AudioContent::TrackFiles(tracks) => {
                tracks.iter().map(|f| f.relative_path.as_str()).collect()
            }
            AudioContent::CueFlacPairs(_) => vec![],
        };
        assert_eq!(audio_paths, vec!["track.flac"]);

        // Check artwork - should include both root cover.jpg and .bae/cover-mb.jpg
        let mut artwork_paths: Vec<_> = files
            .artwork
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        artwork_paths.sort();
        assert_eq!(artwork_paths, vec![".bae/cover-mb.jpg", "cover.jpg"]);

        // .hidden/secret.jpg should NOT be included
        assert!(
            !files
                .artwork
                .iter()
                .any(|f| f.relative_path.contains(".hidden")),
            ".hidden directory should be skipped"
        );

        // Non-image files from .bae/ should not appear in documents
        assert!(files.documents.is_empty());
    }

    /// Creates a minimal CUE file content that references the given FLAC filename
    fn make_cue_content(flac_filename: &str, title: &str) -> String {
        format!(
            r#"PERFORMER "Test Artist"
TITLE "{title}"
FILE "{flac_filename}" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:00:00
"#
        )
    }

    #[test]
    fn test_collection_of_albums_detected_as_separate_candidates() {
        // This replicates a common layout: a collection folder containing multiple
        // independent albums, each with its own CUE+FLAC pair.
        //
        // Structure:
        //   Artist Collection/
        //   ├── 2020 - Album One [CAT001]/
        //   │   ├── Artist - Album One.cue
        //   │   ├── Artist - Album One.flac
        //   │   └── cover.jpg
        //   ├── 2021 - Album Two [CAT002]/
        //   │   └── ...
        //   └── 2022 - Album Three [CAT003]/
        //       └── ...
        //
        // Each subdirectory should be detected as a separate candidate (3 total),
        // NOT as a single multi-disc release.

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Artist Collection");
        std::fs::create_dir(&root).unwrap();

        let albums = [
            ("2020 - Album One [CAT001]", "Artist - Album One"),
            ("2021 - Album Two [CAT002]", "Artist - Album Two"),
            ("2022 - Album Three [CAT003]", "Artist - Album Three"),
        ];

        for (folder_name, file_base) in &albums {
            let album_dir = root.join(folder_name);
            std::fs::create_dir(&album_dir).unwrap();

            let flac_name = format!("{}.flac", file_base);
            let cue_name = format!("{}.cue", file_base);

            std::fs::write(album_dir.join(&flac_name), fake_flac()).unwrap();
            std::fs::write(
                album_dir.join(&cue_name),
                make_cue_content(&flac_name, file_base),
            )
            .unwrap();
            std::fs::write(album_dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        // We expect 3 separate candidates, one per album
        assert_eq!(
            candidates.len(),
            3,
            "Expected 3 separate album candidates, got {}. \
             The scanner is incorrectly treating a collection as a single multi-disc release.",
            candidates.len()
        );

        // Each candidate should have exactly 1 CUE/FLAC pair
        for candidate in &candidates {
            match &candidate.files.audio {
                AudioContent::CueFlacPairs(pairs) => {
                    assert_eq!(
                        pairs.len(),
                        1,
                        "Each album should have exactly 1 CUE/FLAC pair"
                    );
                }
                AudioContent::TrackFiles(_) => {
                    panic!(
                        "Expected CUE/FLAC pairs, got track files for {}",
                        candidate.name
                    );
                }
            }
        }
    }

    #[test]
    fn test_multi_disc_release_detected_as_single_candidate() {
        // A multi-disc release with CD1/CD2 subfolders should be detected as ONE candidate.
        //
        // Structure:
        //   Multi Disc Album/
        //   ├── CD1/
        //   │   ├── Artist - Album CD1.cue
        //   │   └── Artist - Album CD1.flac
        //   └── CD2/
        //       ├── Artist - Album CD2.cue
        //       └── Artist - Album CD2.flac

        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Multi Disc Album");
        std::fs::create_dir(&root).unwrap();

        let discs = [("CD1", "Artist - Album CD1"), ("CD2", "Artist - Album CD2")];

        for (folder_name, file_base) in &discs {
            let disc_dir = root.join(folder_name);
            std::fs::create_dir(&disc_dir).unwrap();

            let flac_name = format!("{}.flac", file_base);
            let cue_name = format!("{}.cue", file_base);

            std::fs::write(disc_dir.join(&flac_name), fake_flac()).unwrap();
            std::fs::write(
                disc_dir.join(&cue_name),
                make_cue_content(&flac_name, file_base),
            )
            .unwrap();
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        // We expect 1 candidate (the multi-disc album as a whole)
        assert_eq!(
            candidates.len(),
            1,
            "Expected 1 multi-disc release candidate, got {}",
            candidates.len()
        );

        // That candidate should have 2 CUE/FLAC pairs (one per disc)
        match &candidates[0].files.audio {
            AudioContent::CueFlacPairs(pairs) => {
                assert_eq!(
                    pairs.len(),
                    2,
                    "Multi-disc release should have 2 CUE/FLAC pairs"
                );
            }
            AudioContent::TrackFiles(_) => {
                panic!("Expected CUE/FLAC pairs for multi-disc release");
            }
        }
    }

    /// Helper to create a multi-disc test structure and verify it's detected as 1 candidate
    fn assert_multi_disc_detected(folder_names: &[&str]) {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Test Album");
        std::fs::create_dir(&root).unwrap();

        for folder_name in folder_names {
            let disc_dir = root.join(folder_name);
            std::fs::create_dir(&disc_dir).unwrap();
            std::fs::write(disc_dir.join("track.flac"), fake_flac()).unwrap();
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            1,
            "Folders {:?} should be detected as 1 multi-disc release, got {}",
            folder_names,
            candidates.len()
        );
    }

    /// Helper to create a collection test structure and verify each is a separate candidate
    fn assert_collection_detected(folder_names: &[&str]) {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Collection");
        std::fs::create_dir(&root).unwrap();

        for folder_name in folder_names {
            let album_dir = root.join(folder_name);
            std::fs::create_dir(&album_dir).unwrap();
            std::fs::write(album_dir.join("track.flac"), fake_flac()).unwrap();
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            folder_names.len(),
            "Folders {:?} should be detected as {} separate albums, got {}",
            folder_names,
            folder_names.len(),
            candidates.len()
        );
    }

    #[test]
    fn test_multi_disc_disc_1_disc_2() {
        assert_multi_disc_detected(&["Disc 1", "Disc 2"]);
    }

    #[test]
    fn test_multi_disc_side_a_side_b() {
        assert_multi_disc_detected(&["Side A", "Side B"]);
    }

    #[test]
    fn test_multi_disc_numbered() {
        assert_multi_disc_detected(&["1", "2", "3"]);
    }

    #[test]
    fn test_multi_disc_zero_padded() {
        assert_multi_disc_detected(&["01", "02"]);
    }

    #[test]
    fn test_collection_year_prefixed() {
        assert_collection_detected(&["2020 - Album One", "2021 - Album Two", "2022 - Album Three"]);
    }

    #[test]
    fn test_collection_artist_prefixed() {
        assert_collection_detected(&[
            "Artist - First Album",
            "Artist - Second Album",
            "Artist - Third Album",
        ]);
    }

    #[test]
    fn test_collection_with_catalog_numbers() {
        assert_collection_detected(&[
            "Album One [CAT001]",
            "Album Two [CAT002]",
            "Album Three [CAT003]",
        ]);
    }

    #[test]
    fn test_cue_without_flac_not_detected() {
        // A folder with CUE + unsupported audio (APE, WAV, etc.) should NOT be detected.
        // We only support FLAC.
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("APE Album");
        std::fs::create_dir(&root).unwrap();

        // Create CUE file that references an APE file
        let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "album.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
        std::fs::write(root.join("album.cue"), cue_content).unwrap();
        std::fs::write(root.join("album.ape"), b"fake ape data").unwrap();
        std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            0,
            "CUE + APE (no FLAC) should not be detected as a candidate"
        );
    }

    #[test]
    fn test_empty_folder_not_detected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Empty Album");
        std::fs::create_dir(&root).unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(candidates.len(), 0, "Empty folder should not be detected");
    }

    #[test]
    fn test_folder_with_only_images_not_detected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Just Images");
        std::fs::create_dir(&root).unwrap();

        std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        std::fs::write(
            root.join("back.png"),
            [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        )
        .unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            0,
            "Folder with only images should not be detected"
        );
    }

    #[test]
    fn test_video_ts_folder_not_detected() {
        // DVD rips with VIDEO_TS folders should not be detected (no FLAC)
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Concert DVD");
        std::fs::create_dir(&root).unwrap();

        let video_ts = root.join("VIDEO_TS");
        std::fs::create_dir(&video_ts).unwrap();
        std::fs::write(video_ts.join("VIDEO_TS.VOB"), b"fake video").unwrap();
        std::fs::write(video_ts.join("VTS_01_1.VOB"), b"fake video").unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            0,
            "VIDEO_TS folder (DVD rip) should not be detected"
        );
    }

    #[test]
    fn test_volume_folders_with_long_names_are_separate() {
        // "Vol. 01 (Catalog Info)" style folders are long enough (> 15 chars)
        // to be treated as separate albums, not multi-disc
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Compilation Series");
        std::fs::create_dir(&root).unwrap();

        let volumes = ["Vol. 01 (R2 70921 - 1990)", "Vol. 02 (R2 70922 - 1991)"];

        for vol_name in &volumes {
            let vol_dir = root.join(vol_name);
            std::fs::create_dir(&vol_dir).unwrap();
            std::fs::write(vol_dir.join("track.flac"), fake_flac()).unwrap();
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            2,
            "Long 'Vol. XX (catalog)' folders should be separate candidates, not multi-disc"
        );
    }

    #[test]
    fn test_zero_byte_files_ignored() {
        // Torrent clients often leave 0-byte placeholder files for incomplete downloads.
        // These should not be considered real audio files.
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Incomplete Download");
        std::fs::create_dir(&root).unwrap();

        // Create 0-byte FLAC files (incomplete download placeholders)
        std::fs::write(root.join("01 - Track One.flac"), b"").unwrap();
        std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
        std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            0,
            "Folder with only 0-byte FLAC files should not be detected"
        );
    }

    #[test]
    fn test_real_discography_scan() {
        let root = PathBuf::from("/Users/dima/Downloads/A Tribe Called Quest - Discography [FLAC]");
        if !root.exists() {
            eprintln!("Skipping test: real discography folder not found");
            return;
        }

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        eprintln!("\n=== Found {} candidates ===\n", candidates.len());
        for (i, c) in candidates.iter().enumerate() {
            let audio_desc = match &c.files.audio {
                AudioContent::CueFlacPairs(pairs) => {
                    let total_tracks: usize = pairs.iter().map(|p| p.track_count).sum();
                    format!("{} CUE/FLAC pair(s), {} tracks", pairs.len(), total_tracks)
                }
                AudioContent::TrackFiles(tracks) => format!("{} track files", tracks.len()),
            };
            let artwork_count = c.files.artwork.len();
            let doc_count = c.files.documents.len();
            eprintln!(
                "  [{}] {}\n      path: {}\n      audio: {}\n      artwork: {}, documents: {}\n",
                i + 1,
                c.name,
                c.path.display(),
                audio_desc,
                artwork_count,
                doc_count,
            );
        }
    }

    #[test]
    fn test_mix_of_real_and_zero_byte_files() {
        // One valid FLAC file, two 0-byte placeholders
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Partial Download");
        std::fs::create_dir(&root).unwrap();

        std::fs::write(root.join("01 - Track One.flac"), fake_flac()).unwrap();
        std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
        std::fs::write(root.join("03 - Track Three.flac"), b"").unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root.clone(), |c| candidates.push(c)).unwrap();

        assert_eq!(
            candidates.len(),
            1,
            "Should detect folder with at least one valid file"
        );

        let audio_count = match &candidates[0].files.audio {
            AudioContent::TrackFiles(tracks) => tracks.len(),
            AudioContent::CueFlacPairs(pairs) => pairs.len(),
        };
        assert_eq!(audio_count, 1, "Should only count the valid FLAC file");

        assert_eq!(
            candidates[0].files.bad_audio_count, 2,
            "Should count 2 bad audio files (0-byte)"
        );
    }

    #[test]
    fn test_corrupt_image_counted_as_bad() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Bad Images");
        std::fs::create_dir(&root).unwrap();

        // Valid audio so the folder is detected
        std::fs::write(root.join("track.flac"), fake_flac()).unwrap();

        // Valid JPEG magic
        std::fs::write(root.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();

        // Corrupt JPEG (wrong magic)
        std::fs::write(root.join("back.jpg"), b"not a jpeg").unwrap();

        // 0-byte image
        std::fs::write(root.join("inlay.png"), b"").unwrap();

        let mut candidates = Vec::new();
        scan_for_candidates_with_callback(root, |c| candidates.push(c)).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].files.artwork.len(),
            1,
            "Only valid image kept"
        );
        assert_eq!(
            candidates[0].files.bad_image_count, 2,
            "Two bad images: corrupt + 0-byte"
        );
    }
}
