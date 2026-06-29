//! Recursive folder scanner with leaf detection for imports.
//!
//! Supports three folder structures:
//! 1. Single release (flat) - audio files in root, optional artwork subfolders
//! 2. Single release (multi-disc) - disc subfolders with audio, optional artwork
//! 3. Collections - recursive tree where leaves are single releases
//!
//! The detection logic is abstract over the file source via the `FileTree`
//! representation.
use super::file_validation;
use crate::cue_flac::CueFlacProcessor;
use crate::util::content_type_hint::ContentTypeHint;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];
const DOCUMENT_EXTENSIONS: &[&str] = &["cue", "log", "txt", "m3u", "m3u8"];

/// Extensions used by download clients and browsers to mark an
/// in-progress download. Presence of any of these anywhere in a folder means
/// the folder is mid-download and should not surface as an import candidate.
const PARTIAL_MARKER_EXTENSIONS: &[&str] = &["part", "crdownload", "download", "aria2", "partial"];

// ── Public types ────────────────────────────────────────────────────────────

/// A file discovered during folder scanning
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Absolute filesystem path. A scanned file is always on disk.
    pub path: PathBuf,
    /// Relative path from release root (for display)
    pub relative_path: String,
    /// File size in bytes
    pub size: u64,
    /// Directory prefix of relative_path (e.g. "Disc 1/"). `None` when the
    /// file is at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// File name without directory prefix.
    pub file_name: String,
}

impl ScannedFile {
    pub fn new(path: PathBuf, relative_path: String, size: u64) -> Self {
        let (dir_prefix, file_name) = match relative_path.rfind('/') {
            Some(idx) => (
                Some(relative_path[..=idx].to_string()),
                relative_path[idx + 1..].to_string(),
            ),
            None => (None, relative_path.clone()),
        };
        Self {
            path,
            relative_path,
            size,
            dir_prefix,
            file_name,
        }
    }
}

/// A CUE/audio pair detected during folder scanning.
///
/// The CUE is on disk and parsed during the scan, so `cue_sheet` is `Some`.
/// Consumers that need parsed CUE data — the import mapper — require `Some`.
#[derive(Debug, Clone)]
pub struct ScannedCueFlacPair {
    /// The CUE sheet file
    pub cue_file: ScannedFile,
    /// The audio file
    pub audio_file: ScannedFile,
    /// Parsed CUE sheet.
    pub cue_sheet: Option<crate::cue_flac::CueSheet>,
    /// Combined size of CUE + audio file.
    pub total_size: u64,
}

/// The audio content type of a release - mutually exclusive
#[derive(Debug, Clone)]
pub enum AudioContent {
    /// One or more CUE+audio pairs (multi-disc releases can have multiple).
    /// `format_label` is e.g. "CUE+FLAC", "CUE+APE".
    CueFlacPairs {
        pairs: Vec<ScannedCueFlacPair>,
        format_label: String,
    },
    /// Individual track files (file-per-track releases).
    /// `format_label` is e.g. "FLAC", "MP3", "APE".
    TrackFiles {
        tracks: Vec<ScannedFile>,
        format_label: String,
    },
}

impl AudioContent {
    /// Total track count across the release, or `None` if any CUE pair has
    /// no parsed sheet. Callers that render a count must handle the `None`
    /// case directly rather than substitute a placeholder.
    pub fn track_count(&self) -> Option<u32> {
        match self {
            Self::CueFlacPairs { pairs, .. } => pairs
                .iter()
                .map(|p| p.cue_sheet.as_ref().map(|s| s.tracks.len()))
                .sum::<Option<usize>>()
                .map(|n| n as u32),
            Self::TrackFiles { tracks, .. } => Some(tracks.len() as u32),
        }
    }

    pub fn format_label(&self) -> &str {
        match self {
            Self::CueFlacPairs { format_label, .. } => format_label,
            Self::TrackFiles { format_label, .. } => format_label,
        }
    }
}

/// Files from a release, pre-categorized by type
#[derive(Debug, Clone)]
pub struct CategorizedFiles {
    /// Audio content - either CUE/FLAC pairs or individual track files
    pub audio: AudioContent,
    /// Artwork/image files (.jpg, .png, etc.)
    pub artwork: Vec<ScannedFile>,
    /// Document files (.log, .txt, .m3u) - CUE files in pairs are NOT included here
    pub documents: Vec<ScannedFile>,
    /// Parsed sheets for CUEs that aren't part of a CUE+audio pair: multi-FILE
    /// CUEs (one FILE per TRACK — never pair) and aggregate CUEs alongside
    /// per-track audio. Paired CUEs carry their sheet on the pair itself. The
    /// CUE is still listed in `documents`; this is the parsed-signal channel so
    /// catalog/performer/title harvesting reads parsed data instead of
    /// re-reading the file.
    pub unpaired_cue_sheets: Vec<(PathBuf, crate::cue_flac::CueSheet)>,
}

impl CategorizedFiles {
    /// Stable content fingerprint of this release's file structure: a SHA-256
    /// over every audio, artwork, and document file's relative path + size,
    /// sorted so the digest is independent of discovery order. Relative (not
    /// absolute) paths make it location-independent — the same rip under any
    /// parent folder hashes identically. Drives "already imported?" detection
    /// and selects the overwrite target on re-import.
    ///
    /// Paired CUE files live on `audio` (not `documents`); unpaired CUE files
    /// are in `documents`. Together with `artwork` that's every on-disk file,
    /// each counted once. `unpaired_cue_sheets` is parsed-signal data, not a
    /// file — its CUE is already covered as a document, so it's excluded.
    pub fn content_hash(&self) -> String {
        let mut entries: Vec<(&str, u64)> = Vec::new();
        match &self.audio {
            AudioContent::CueFlacPairs { pairs, .. } => {
                for pair in pairs {
                    entries.push((&pair.cue_file.relative_path, pair.cue_file.size));
                    entries.push((&pair.audio_file.relative_path, pair.audio_file.size));
                }
            }
            AudioContent::TrackFiles { tracks, .. } => {
                for track in tracks {
                    entries.push((&track.relative_path, track.size));
                }
            }
        }
        for file in self.artwork.iter().chain(&self.documents) {
            entries.push((&file.relative_path, file.size));
        }
        entries.sort_unstable();

        let mut hasher = Sha256::new();
        for (path, size) in entries {
            hasher.update(path.as_bytes());
            hasher.update([0u8]);
            hasher.update(size.to_le_bytes());
            hasher.update([b'\n']);
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Commit-ready release data: the parsed DB-shape album/release/tracks plus
/// the raw JSON pairs for archival. Produced by `commit_mb_release` /
/// `commit_discogs_release` on the worker side. Prefetch returns
/// `ImportSearchReleaseDetail` directly and never builds this struct —
/// the picker doesn't need the DB shape.
#[derive(Debug, Clone)]
pub struct PreparedRelease {
    pub source: crate::import::types::MetadataSource,
    pub release_id: String,
    pub parsed: crate::import::ParsedAlbum,
    /// `(source_name, raw_json)` pairs for the `release_metadata` table.
    pub metadata_pairs: Vec<(String, String)>,
}

/// A leaf folder that looks like a release (it has audio) but failed
/// validation: corrupt or zero-byte audio, a corrupt image, or a CUE that
/// references missing audio. Carries no files or identify state — it can't be
/// imported — only enough to surface it under the Skipped tab with its reason.
/// Why a candidate folder failed validation. The `Display` text is the terse
/// internal form (used by the import-commit error channel); the UI localizes the
/// typed variant for the Skipped tab.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidReason {
    #[error("corrupt or zero-byte audio file: {path}")]
    CorruptAudioFile { path: String },
    #[error("corrupt or zero-byte image: {path}")]
    CorruptImage { path: String },
    #[error("CUE references a missing audio file")]
    CueMissingAudio,
    #[error("no valid audio files")]
    NoValidAudio,
}

#[derive(Debug, Clone)]
pub struct InvalidCandidate {
    /// Root path of the folder that failed validation.
    pub path: PathBuf,
    /// Display name (derived from folder name).
    pub name: String,
    /// Absolute path of the watched folder this was scanned from — the
    /// candidate-list group it belongs to. Equal to the scan root.
    pub watched_folder_path: String,
    /// Why the folder failed validation — the UI localizes this typed reason.
    pub reason: InvalidReason,
}

/// One item the scan callback yields per leaf folder: a valid release
/// candidate, or an invalid one (looked like a release but failed validation).
#[derive(Debug, Clone)]
pub enum ScanItem {
    Valid(FolderCandidate),
    Invalid(InvalidCandidate),
}

/// A folder candidate (leaf directory) detected during filesystem scanning.
/// Called "candidate" because it hasn't been identified yet. One candidate
/// per release; per-disc breakdown lives on `ScannedFile.dir_prefix`.
#[derive(Debug, Clone)]
pub struct FolderCandidate {
    /// Root path of this release
    pub path: PathBuf,
    /// Display name (derived from folder name)
    pub name: String,
    /// Pre-categorized files for this release
    pub files: CategorizedFiles,
    /// Absolute path of the watched folder this candidate was scanned from —
    /// the candidate-list group it belongs to. Equal to the scan root. The
    /// group's display name comes from the watched-folder list, not here.
    pub watched_folder_path: String,
    /// Whether the user manually marked this candidate as skipped. The scanner's
    /// blocking walk has no registry access, so it leaves this `false`; the
    /// watcher stamps the real value from the folder registry after the scan.
    pub skipped: bool,
    /// Whether this folder's file structure was already imported (its
    /// `CategorizedFiles::content_hash` matches a release in the library). Like
    /// `skipped`, the scanner can't query the DB, so it leaves this `false`; the
    /// watcher stamps it after the scan. Drives the import view's "Added" tab so
    /// a re-scanned, already-imported folder still surfaces as added across
    /// restarts.
    pub is_added: bool,
}

// ── FileTree: abstract file source ──────────────────────────────────────────

/// A single file entry in a `FileTree`.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the scan root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// A pre-collected list of files for release candidate detection.
///
/// Built by walking the filesystem once.
pub struct FileTree {
    files: Vec<FileEntry>,
}

impl FileTree {
    pub fn new(files: Vec<FileEntry>) -> Self {
        Self { files }
    }

    /// Build a FileTree by recursively walking the filesystem from `root`.
    /// Skips hidden files/directories (names starting with '.') and noise files.
    pub(crate) fn from_filesystem(root: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        Self::walk_dir(root, root, &mut files)?;
        Ok(Self { files })
    }

    fn walk_dir(current: &Path, root: &Path, files: &mut Vec<FileEntry>) -> Result<(), String> {
        let entries = fs::read_dir(current)
            .map_err(|e| format!("Failed to read dir {:?}: {}", current, e))?;

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip hidden files and directories (including .bae/)
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            if path.is_file() {
                if is_noise_file(&path) {
                    continue;
                }

                let size = entry
                    .metadata()
                    .map_err(|e| format!("Failed to read metadata for {:?}: {}", path, e))?
                    .len();

                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| format!("Failed to strip prefix: {}", e))?
                    .to_path_buf();

                files.push(FileEntry {
                    path: relative,
                    size,
                });
            } else if path.is_dir() {
                Self::walk_dir(&path, root, files)?;
            }
        }

        Ok(())
    }

    /// Files whose parent directory is exactly `dir` (not recursive).
    fn files_in_dir<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        // For root dir, we use "" as the canonical form
        let dir = if dir == Path::new("") || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        };

        self.files.iter().filter(move |f| {
            f.path
                .parent()
                .map(|p| p == dir)
                .unwrap_or(dir.as_os_str().is_empty())
        })
    }

    /// Distinct immediate child directories of `dir`.
    fn immediate_subdirs(&self, dir: &Path) -> Vec<PathBuf> {
        let dir_normalized = if dir == Path::new("") || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        };

        let mut subdirs = BTreeSet::new();
        for f in &self.files {
            // Check if this file is under `dir`
            if let Ok(relative) = f.path.strip_prefix(&dir_normalized) {
                // If there's at least one component before the filename, the first
                // component is an immediate subdirectory
                let mut components = relative.components();
                if let Some(first) = components.next() {
                    // Only count as subdir if there's more after the first component
                    // (i.e., the first component is a directory, not the file itself)
                    if components.next().is_some() {
                        subdirs.insert(dir_normalized.join(first));
                    }
                }
            }
        }
        subdirs.into_iter().collect()
    }

    /// All files recursively under `dir` (inclusive).
    fn all_files_under<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        let dir_normalized = if dir == Path::new("") || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        };

        self.files.iter().filter(move |f| {
            if dir_normalized.as_os_str().is_empty() {
                true
            } else {
                f.path.starts_with(&dir_normalized)
            }
        })
    }
}

// ── Extension-based classification (pure, no I/O) ──────────────────────────

/// Check if a file is an audio file based on extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ContentTypeHint::from_extension(ext).is_audio())
        .unwrap_or(false)
}

/// Codec label for a CUE-paired audio file extension, used to build the
/// `CUE+<codec>` format label. Mirrors the analyzer's extension dispatch
/// (`track_to_file_mapper::analyze_cue_audio`): `.flac` → FLAC, `.ape` → APE,
/// `.m4a` → ALAC (CUE+MP4 pairs are treated as ALAC by policy; AAC rejected
/// at analysis time). Returns `ContentType::display_name()` so the label
/// follows the canonical codec name.
///
/// The CUE pair detector in `cue_flac.rs` only admits these three extensions,
/// so any other value reaching here is a programming error.
fn cue_pair_codec_label(ext: &str) -> &'static str {
    use crate::util::content_type::ContentType;
    match ext.to_ascii_lowercase().as_str() {
        "flac" => ContentType::Flac.display_name(),
        "ape" => ContentType::Ape.display_name(),
        "m4a" => ContentType::Alac.display_name(),
        other => panic!(
            "cue_pair_codec_label: unsupported CUE pair extension '{}' \
             (detector in cue_flac.rs should have filtered this out)",
            other
        ),
    }
}

/// Check if a file is an image/artwork file
fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a file is a document file (.cue, .log, .txt, .m3u)
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

/// True when `path`'s extension matches a known in-progress-download marker
/// (e.g. `01.flac.part`, `02.flac.crdownload`, `03.aria2`). Match is case-insensitive.
fn is_partial_marker_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| PARTIAL_MARKER_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ── Tree-based detection heuristics (no I/O) ───────────────────────────────

/// Check if a directory contains audio files directly (by extension).
///
/// This is used for tree-structure detection (leaf vs collection), not for
/// validation. Even a directory with only corrupt FLAC files should be detected
/// as a candidate (though corrupt files are silently skipped during categorization).
/// Only skips 0-byte files since those are empty placeholders.
fn tree_has_audio_files(tree: &FileTree, dir: &Path) -> bool {
    tree.files_in_dir(dir)
        .any(|f| is_audio_file(&f.path) && f.size > 0)
}

/// True when any immediate subdirectory of `dir` contains audio somewhere
/// in its subtree. The check must recurse — shallow matches only catch
/// releases whose audio sits directly one level below `dir`, so any
/// collection that wraps releases under artist/label/discography dirs
/// would look audio-less to a caller asking "is this a container?".
fn tree_has_subdirs_with_audio(tree: &FileTree, dir: &Path) -> bool {
    tree.immediate_subdirs(dir).iter().any(|subdir| {
        tree.all_files_under(subdir)
            .any(|f| is_audio_file(&f.path) && f.size > 0)
    })
}

/// Check if any subdirectory has its own subdirectories with audio files.
fn tree_has_nested_audio_dirs(tree: &FileTree, dir: &Path) -> bool {
    tree.immediate_subdirs(dir)
        .iter()
        .any(|subdir| tree_has_subdirs_with_audio(tree, subdir))
}

/// True when `dir` or any of its descendants contain a partial-download
/// marker. Applied when a directory has been identified as a candidate
/// (leaf) so we refuse to emit an in-progress release whose markers live
/// one or more levels down (e.g. `Album/Disc 2/01.flac.part` under a
/// multi-disc leaf).
fn tree_has_partial_markers_deep(tree: &FileTree, dir: &Path) -> bool {
    tree.all_files_under(dir)
        .any(|f| is_partial_marker_file(&f.path))
}

/// True when every audio-bearing immediate subdirectory of `dir` has a name
/// matching a common disc-indicator pattern (`Disc N`, `CD N`, `Disk N`,
/// `Side A/B`, `Part N`). Applied to ALL audio-bearing subdirs, not a
/// majority — one "Disc 1" sibling next to "Bonus Tracks" does not qualify.
///
/// This discriminates true multi-disc releases from artist/discography/reissue
/// folders that merely happen to have ≥2 audio-bearing children.
fn looks_like_multi_disc_siblings(tree: &FileTree, dir: &Path) -> bool {
    let audio_subdirs: Vec<_> = tree
        .immediate_subdirs(dir)
        .into_iter()
        .filter(|s| tree_has_audio_files(tree, s) || tree_has_subdirs_with_audio(tree, s))
        .collect();

    if audio_subdirs.len() < 2 {
        return false;
    }

    audio_subdirs.iter().all(|subdir| {
        subdir
            .file_name()
            .and_then(|n| n.to_str())
            .map(is_disc_indicator_name)
            .unwrap_or(false)
    })
}

/// Match names that uniquely identify one slice of a multi-disc release —
/// `Disc 1`, `CD2`, `Disk-03`, `Side A`, `Part_2` (case-insensitive, with
/// optional `[ \t\-_.]` between keyword and suffix), or a bare numeric name
/// (`1`, `02`). Anything with additional descriptive text (`1991 - Album A2`,
/// `Vol. 01 (catalog)`, `Artist - Album`) deliberately does NOT match —
/// that's the signal we use to distinguish true multi-disc releases from
/// artist / discography / reissue folders.
fn is_disc_indicator_name(name: &str) -> bool {
    const SEPARATORS: [char; 4] = [' ', '\t', '-', '_'];
    let lower = name.to_lowercase();

    // Bare numeric: `1`, `02`, `003`.
    if !lower.is_empty() && lower.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // Numeric-suffix prefixes: Disc/CD/Disk/Part. Accept descriptive trailing
    // text (e.g. `CD 4 - 1937-40 • NYC` or `Disc 1 (Bonus)`) as long as the
    // digit run after the prefix is terminated by end-of-string or any
    // non-alphanumeric character. `Discography`/`Disc1A` still fail because
    // their prefix is followed directly by alpha / alphanumeric.
    for prefix in ["disc", "cd", "disk", "part"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            // Allow `.` as a separator too (e.g. `CD.3`). `.` can't be in the
            // SEPARATORS const used later for `Side`, since a single-char
            // alpha suffix would collide with trimming multiple dots.
            let rest = rest.trim_start_matches(SEPARATORS).trim_start_matches('.');
            let digit_run: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digit_run.is_empty() {
                continue;
            }
            let after_digits = &rest[digit_run.len()..];
            match after_digits.chars().next() {
                None => return true,
                Some(c) if !c.is_ascii_alphanumeric() => return true,
                Some(_) => {}
            }
        }
    }

    // Alpha-suffix prefix: Side (e.g. "Side A", "Side-A"). Require at least
    // one separator between `Side` and the alpha char so `Sider`, `Sideshow`
    // etc. do not match.
    if let Some(rest) = lower.strip_prefix("side") {
        let trimmed = rest.trim_start_matches(SEPARATORS).trim_start_matches('.');
        // Separator must have consumed at least one character (or `rest` was
        // empty, which we'd reject as no alpha suffix to test).
        let consumed_separator = trimmed.len() < rest.len();
        if consumed_separator {
            let mut chars = trimmed.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if c.is_ascii_alphabetic() {
                    return true;
                }
            }
        }
    }

    false
}

/// Determine if a directory is a candidate (a release or group of releases).
///
/// Prefers "zoomed out": stops at the shallowest directory that groups audio.
/// A directory is a candidate if:
/// - Has audio files directly in it, OR
/// - Is a multi-disc release: audio-bearing subdirs all match a disc-indicator
///   pattern (`Disc N`, `CD N`, `Side A`, etc.). Artist / discography / reissue
///   folders with ≥2 audio-bearing children do NOT qualify — their children
///   are independent candidates, not parts of one release.
fn tree_is_leaf_directory(tree: &FileTree, dir: &Path) -> bool {
    let has_direct_audio = tree_has_audio_files(tree, dir);
    let has_subdirs_with_audio = tree_has_subdirs_with_audio(tree, dir);

    if has_direct_audio && has_subdirs_with_audio {
        debug!(
            "Directory {:?} has direct audio and audio-bearing subdirs; treating as container, descending into subdirs",
            dir
        );
        return false;
    }

    if has_direct_audio {
        debug!("Directory {:?} is a candidate (has audio files)", dir);
        return true;
    }

    if has_subdirs_with_audio
        && !tree_has_nested_audio_dirs(tree, dir)
        && looks_like_multi_disc_siblings(tree, dir)
    {
        debug!(
            "Directory {:?} is a candidate (disc-indicator subdirs)",
            dir
        );
        return true;
    }

    debug!("Directory {:?} is not a candidate", dir);
    false
}

// ── File categorization ─────────────────────────────────────────────────────

/// The result of categorizing a leaf folder's files: a valid release, or an
/// invalid one carrying the reason it failed validation (corrupt/zero-byte
/// audio, corrupt image, CUE referencing missing audio). `Err` is reserved for
/// genuine I/O faults, which are not the same as a failed-validation leaf.
#[derive(Debug)]
enum CategorizeOutcome {
    Valid(CategorizedFiles),
    Invalid(InvalidReason),
}

/// Shorthand for a failed-validation leaf carrying `reason`.
fn invalid(reason: InvalidReason) -> Result<CategorizeOutcome, String> {
    Ok(CategorizeOutcome::Invalid(reason))
}

/// Categorize files from a FileTree for a given release root.
///
/// `fs_root` is the folder being imported: file validation reads actual bytes
/// from disk.
/// Returns `Invalid(reason)` when the folder has audio but fails validation, so
/// the caller can surface why it can't be imported.
fn categorize_files_from_tree(
    tree: &FileTree,
    release_root: &Path,
    fs_root: &Path,
) -> Result<CategorizeOutcome, String> {
    let mut all_audio: Vec<ScannedFile> = Vec::new();
    let mut all_cue: Vec<ScannedFile> = Vec::new();
    let mut artwork: Vec<ScannedFile> = Vec::new();
    let mut documents: Vec<ScannedFile> = Vec::new();

    for entry in tree.all_files_under(release_root) {
        let relative_from_release = if release_root.as_os_str().is_empty() {
            entry.path.clone()
        } else {
            entry
                .path
                .strip_prefix(release_root)
                .unwrap_or(&entry.path)
                .to_path_buf()
        };

        let relative_path = relative_from_release.to_string_lossy().to_string();

        // The absolute path is fs_root + entry.path.
        let absolute_path = fs_root.join(&entry.path);

        if is_audio_file(&entry.path) {
            // Ok(false) is corruption (skip the candidate); Err is a genuine
            // I/O fault (file vanished, permissions, flaky network mount) —
            // surface it rather than mis-label a system error as corruption
            // and silently drop the whole release.
            let valid = file_validation::is_valid_audio(&absolute_path)
                .map_err(|e| format!("Failed to validate audio file {absolute_path:?}: {e}"))?;
            if entry.size == 0 || !valid {
                info!("Invalid candidate: corrupt or zero-byte audio file {relative_path}");
                return invalid(InvalidReason::CorruptAudioFile {
                    path: relative_path.to_string(),
                });
            }

            all_audio.push(ScannedFile::new(
                absolute_path.clone(),
                relative_path,
                entry.size,
            ));
        } else if is_cue_file(&entry.path) {
            all_cue.push(ScannedFile::new(
                absolute_path.clone(),
                relative_path,
                entry.size,
            ));
        } else if is_image_file(&entry.path) {
            // As with audio: Ok(false) is corruption (skip), Err is a real
            // I/O fault that must surface rather than drop the release.
            let valid = file_validation::is_valid_image(&absolute_path)
                .map_err(|e| format!("Failed to validate image file {absolute_path:?}: {e}"))?;
            if entry.size == 0 || !valid {
                info!("Invalid candidate: corrupt or zero-byte image {relative_path}");
                return invalid(InvalidReason::CorruptImage {
                    path: relative_path.to_string(),
                });
            }

            artwork.push(ScannedFile::new(
                absolute_path.clone(),
                relative_path,
                entry.size,
            ));
        } else if is_document_file(&entry.path) {
            documents.push(ScannedFile::new(absolute_path, relative_path, entry.size));
        }
        // Other file types are ignored
    }

    // Parse every CUE exactly once. The pair builder, the incomplete-rip
    // guard, and the pair-detection pass below all read from this map —
    // single source of truth per CUE.
    let parsed_cues: std::collections::HashMap<PathBuf, crate::cue_flac::CueSheet> = all_cue
        .iter()
        .map(|cue| {
            CueFlacProcessor::parse_cue_sheet(&cue.path)
                .map(|sheet| (cue.path.clone(), sheet))
                .map_err(|e| format!("Failed to parse CUE {:?}: {}", cue.path, e))
        })
        .collect::<Result<_, _>>()?;

    // Detect CUE+audio pairs by the CUE's own FILE directive — single-FILE
    // sheets pair with the named audio file in the same directory; multi-FILE
    // sheets (one FILE per TRACK) cannot pair on purpose.
    let audio_paths_set: std::collections::HashSet<&PathBuf> =
        all_audio.iter().map(|f| &f.path).collect();
    let detected_pairs: Vec<crate::cue_flac::CueFlacPair> = all_cue
        .iter()
        .filter_map(|cue| {
            let sheet = parsed_cues.get(&cue.path)?;
            let file_reference = sheet.single_file()?;
            let cue_dir = cue.path.parent()?;
            let audio_path = cue_dir.join(file_reference);
            audio_paths_set
                .contains(&audio_path)
                .then_some(crate::cue_flac::CueFlacPair {
                    audio_path,
                    cue_path: cue.path.clone(),
                })
        })
        .collect();

    // CUE mismatch guard: a non-pairing CUE whose FILE directive references
    // something not on disk AND whose declared track count exceeds the audio
    // files in the CUE's directory signals an incomplete rip (e.g. 10 per-
    // track FLACs alongside a CUE declaring 15 tracks of `Album.flac`).
    // Refuse the candidate rather than surface half a release.
    //
    // Legitimate non-pairing CUEs — where the CUE is a redundant aggregate of
    // the same per-track FLACs and track counts match — stay as documents.
    // Paired CUEs are skipped outright: pair-detection already verified their
    // audio exists.
    let paired_cues: std::collections::HashSet<PathBuf> =
        detected_pairs.iter().map(|p| p.cue_path.clone()).collect();

    for cue in &all_cue {
        if paired_cues.contains(&cue.path) {
            continue;
        }

        let Some(cue_dir) = cue.path.parent() else {
            continue;
        };

        let cue_sheet = parsed_cues
            .get(&cue.path)
            .expect("parsed_cues is populated for every CUE");
        let unique_refs: std::collections::HashSet<&str> = cue_sheet
            .tracks
            .iter()
            .map(|t| t.file_reference.as_str())
            .collect();
        let references_missing = unique_refs.iter().any(|name| !cue_dir.join(name).exists());
        if !references_missing {
            continue;
        }

        // Count audio files directly in the CUE's directory. Co-location
        // is the rule for CUE+audio: a CUE at a parent of disc subdirs
        // is not a legitimate shape, so we don't count descendants.
        let on_disk_audio = all_audio
            .iter()
            .filter(|f| f.path.parent() == Some(cue_dir))
            .count();

        if cue_sheet.tracks.len() > on_disk_audio {
            info!(
                "Invalid candidate: CUE {:?} declares {} tracks but only {} audio files on disk",
                cue.path,
                cue_sheet.tracks.len(),
                on_disk_audio
            );
            return invalid(InvalidReason::CueMissingAudio);
        }
    }

    let (audio, unpaired_cue_sheets) = if !detected_pairs.is_empty() {
        let mut pairs = Vec::new();
        let mut used_audio_paths = std::collections::HashSet::new();
        let mut used_cue_paths = std::collections::HashSet::new();
        let mut parsed_cues = parsed_cues;

        for pair in detected_pairs {
            let cue_file = all_cue
                .iter()
                .find(|f| f.path == pair.cue_path)
                .cloned()
                .ok_or_else(|| format!("CUE file not found: {:?}", pair.cue_path))?;
            let audio_file = all_audio
                .iter()
                .find(|f| f.path == pair.audio_path)
                .cloned()
                .ok_or_else(|| format!("Audio file not found: {:?}", pair.audio_path))?;

            let cue_sheet = parsed_cues.remove(&pair.cue_path);

            used_audio_paths.insert(pair.audio_path);
            used_cue_paths.insert(pair.cue_path);
            let total_size = cue_file.size + audio_file.size;
            pairs.push(ScannedCueFlacPair {
                cue_file,
                audio_file,
                cue_sheet,
                total_size,
            });
        }

        // Unused CUE files (not part of a pair) become documents
        for cue in all_cue {
            if !used_cue_paths.contains(&cue.path) {
                documents.push(cue);
            }
        }

        pairs.sort_by(|a, b| a.cue_file.relative_path.cmp(&b.cue_file.relative_path));
        let ext = pairs
            .first()
            .and_then(|p| p.audio_file.path.extension())
            .and_then(|e| e.to_str())
            .expect("CUE pair audio file must have an extension");
        let codec_label = cue_pair_codec_label(ext);
        let audio = AudioContent::CueFlacPairs {
            pairs,
            format_label: format!("CUE+{codec_label}"),
        };
        // Whatever's left in the map after pairs drained their entries are the
        // unpaired CUEs (multi-FILE, aggregates) — keep their parsed sheets.
        let mut unpaired: Vec<(PathBuf, crate::cue_flac::CueSheet)> =
            parsed_cues.into_iter().collect();
        unpaired.sort_by(|a, b| a.0.cmp(&b.0));
        (audio, unpaired)
    } else {
        documents.extend(all_cue);
        let mut tracks = all_audio;
        tracks.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let format_label = match tracks.first() {
            Some(t) => t
                .path
                .extension()
                .and_then(|e| e.to_str())
                .expect("Audio file must have an extension")
                .to_uppercase(),
            None => {
                info!("Invalid candidate: no valid audio files after categorization");
                return invalid(InvalidReason::NoValidAudio);
            }
        };
        let audio = AudioContent::TrackFiles {
            tracks,
            format_label,
        };
        // No pairs detected, so every parsed CUE is unpaired (e.g. a multi-FILE
        // CUE sitting alongside its per-track audio).
        let mut unpaired: Vec<(PathBuf, crate::cue_flac::CueSheet)> =
            parsed_cues.into_iter().collect();
        unpaired.sort_by(|a, b| a.0.cmp(&b.0));
        (audio, unpaired)
    };

    artwork.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    documents.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(CategorizeOutcome::Valid(CategorizedFiles {
        audio,
        artwork,
        documents,
        unpaired_cue_sheets,
    }))
}

// ── Tree walker ─────────────────────────────────────────────────────────────

/// Internal leaf data emitted by the tree walker. The public API stamps
/// `watched_folder_path` to turn this into a `ScanItem`.
pub(crate) enum RawScanItem {
    Valid {
        path: PathBuf,
        name: String,
        files: CategorizedFiles,
    },
    Invalid {
        path: PathBuf,
        name: String,
        reason: InvalidReason,
    },
}

fn scan_tree_recursive<F>(
    tree: &FileTree,
    dir: &Path,
    fs_root: &Path,
    on_item: &mut F,
) -> Result<(), String>
where
    F: FnMut(RawScanItem),
{
    if tree_is_leaf_directory(tree, dir) {
        // Release-level marker check: a leaf is a release, and markers
        // anywhere under a release mean the release itself is incomplete.
        // Do not emit and do not recurse — any disc-level child would
        // inherit the same problem.
        if tree_has_partial_markers_deep(tree, dir) {
            info!(
                "Skipping leaf {:?}: partial-download markers present under it",
                dir
            );
            return Ok(());
        }

        let name = if dir.as_os_str().is_empty() {
            fs_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        } else {
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        };

        info!("Found candidate leaf: {:?}", dir);

        let candidate_path = fs_root.join(dir);
        match categorize_files_from_tree(tree, dir, fs_root)? {
            CategorizeOutcome::Valid(files) => {
                on_item(RawScanItem::Valid {
                    path: candidate_path,
                    name,
                    files,
                });
            }
            CategorizeOutcome::Invalid(reason) => {
                // Structurally this looked like a leaf (a release), but
                // categorization failed: zero-byte audio, corrupt file, CUE
                // references a missing file, etc. Surface it with its reason so
                // the user sees why the folder didn't import. Symmetric with
                // the partial-markers-deep check above (which truly suppresses).
                info!("Invalid leaf {:?}: {reason}", dir);
                on_item(RawScanItem::Invalid {
                    path: candidate_path,
                    name,
                    reason,
                });
            }
        }

        return Ok(());
    }

    for subdir in tree.immediate_subdirs(dir) {
        scan_tree_recursive(tree, &subdir, fs_root, on_item)?;
    }

    Ok(())
}

// ── Public API: folder scanning (filesystem) ────────────────────────────────

/// Scan a folder and invoke `on_item` for each leaf: a valid release candidate,
/// or an invalid one (looked like a release but failed validation). Both carry
/// `watched_folder_path` stamped from the scan root.
pub fn scan_for_candidates_with_callback<F>(root: PathBuf, mut on_item: F) -> Result<(), String>
where
    F: FnMut(ScanItem),
{
    info!("Scanning for candidates in: {:?}", root);
    // Every item from this scan belongs to the same watched folder (the scan
    // root) — the group it renders under in the candidate list.
    let watched_folder_path = root.to_string_lossy().into_owned();
    let tree = FileTree::from_filesystem(&root)?;
    scan_tree_recursive(&tree, &PathBuf::new(), &root, &mut |raw| match raw {
        RawScanItem::Valid { path, name, files } => {
            on_item(ScanItem::Valid(FolderCandidate {
                path,
                name,
                files,
                watched_folder_path: watched_folder_path.clone(),
                // The blocking walk has neither the registry nor the DB; the
                // watcher stamps the real per-candidate facts after this scan.
                skipped: false,
                is_added: false,
            }));
        }
        RawScanItem::Invalid { path, name, reason } => {
            on_item(ScanItem::Invalid(InvalidCandidate {
                path,
                name,
                watched_folder_path: watched_folder_path.clone(),
                reason,
            }));
        }
    })
}

/// Collect all files from a release directory and categorize them.
///
/// This collects files recursively within a single release, preserving relative paths,
/// and categorizes them into audio (CUE/FLAC pairs or track files), artwork, and documents.
/// Unrecognized file types are ignored.
pub fn collect_release_candidate_files(release_root: &Path) -> Result<CategorizedFiles, String> {
    let tree = FileTree::from_filesystem(release_root)?;
    // An invalid folder can't be imported: surface its reason as the error so
    // the import-commit caller fails with why the folder is unusable.
    match categorize_files_from_tree(&tree, &PathBuf::new(), release_root)? {
        CategorizeOutcome::Valid(files) => Ok(files),
        CategorizeOutcome::Invalid(reason) => Err(reason.to_string()),
    }
}

#[cfg(test)]
mod tests;
