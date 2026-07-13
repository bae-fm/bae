//! Recursive folder scanner with leaf detection for imports.
//!
//! Handles three folder structures: a flat single release (audio in the root),
//! a multi-disc single release (disc subfolders with audio), and a collection (a
//! tree whose leaves are single releases). Detection runs over a `FileTree`
//! built once from the filesystem.
use super::file_validation;
use crate::cue_flac::parse_cue_sheet;
use crate::util::content_type_hint::ContentTypeHint;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

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
    /// The file's identity within the release (e.g. `CD1/CDImage.ape`) — used
    /// as the HashMap key throughout the import pipeline and as
    /// `DbFile.original_filename`. Two files may share a bare filename; they
    /// never share a relative_path within one release.
    ///
    /// Always `/`-separated, on every platform: it is stored, synced, and joined
    /// back onto a local directory by whichever device exports or unmanages the
    /// release, so it cannot carry the writing platform's separator (see
    /// [`crate::storage::path_fragment`], which refuses one that does).
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
/// The CUE is on disk and parsed during the scan.
#[derive(Debug, Clone)]
pub struct ScannedCueFlacPair {
    /// The CUE sheet file
    pub cue_file: ScannedFile,
    /// The first audio file referenced by the CUE sheet.
    pub audio_file: ScannedFile,
    /// Every audio file referenced by the CUE sheet, in CUE reference order.
    pub audio_files: Vec<ScannedFile>,
    /// Parsed CUE sheet.
    pub cue_sheet: crate::cue_flac::CueSheet,
    /// Combined size of CUE + referenced audio files.
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
    /// Total track count across the release.
    pub fn track_count(&self) -> u32 {
        match self {
            Self::CueFlacPairs { pairs, .. } => pairs
                .iter()
                .map(|p| p.cue_sheet.playable_track_count() as u32)
                .sum::<u32>(),
            Self::TrackFiles { tracks, .. } => tracks.len() as u32,
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
    /// Parsed sheets for CUEs that aren't part of a CUE+audio pair. Paired CUEs
    /// carry their sheet on the pair itself. The CUE is still listed in
    /// `documents`; this is the parsed-signal channel so
    /// catalog/performer/title harvesting reads parsed data instead of
    /// re-reading the file.
    pub unpaired_cue_sheets: Vec<(PathBuf, crate::cue_flac::CueSheet)>,
}

impl CategorizedFiles {
    /// Stable content fingerprint of this release's file structure: a SHA-256 over
    /// every audio, artwork, and document file's relative path + size, sorted so
    /// the digest is independent of discovery order. Relative (not absolute)
    /// paths make it location-independent — the same rip hashes identically under
    /// any parent folder. Drives "already imported?" detection and selects the
    /// overwrite target on re-import.
    ///
    /// Paired CUE files live on `audio`, unpaired ones in `documents`; together
    /// with `artwork` that's every on-disk file, each counted once.
    /// `unpaired_cue_sheets` is parsed-signal data, not a file — its CUE is
    /// already covered as a document — so it's excluded.
    pub fn content_hash(&self) -> String {
        let mut entries: Vec<(&str, u64)> = Vec::new();
        match &self.audio {
            AudioContent::CueFlacPairs { pairs, .. } => {
                for pair in pairs {
                    entries.push((&pair.cue_file.relative_path, pair.cue_file.size));
                    for audio_file in &pair.audio_files {
                        entries.push((&audio_file.relative_path, audio_file.size));
                    }
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
    #[error("CUE could not be parsed: {path}")]
    CueParseFailed { path: String },
    #[error("CUE layout is not supported")]
    CueUnsupportedLayout,
    #[error("CUE audio codec {codec} is not supported for single-file CUE playback")]
    CueUnsupportedCodec { codec: String },
    #[error("no valid audio files")]
    NoValidAudio,
}

/// A leaf folder that looks like a release (it has audio) but failed validation.
/// It can't be imported, so it carries no files and no identify state — only
/// enough to surface it under the Skipped tab with its reason.
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

impl FolderCandidate {
    pub fn track_count(&self) -> u32 {
        self.files.audio.track_count()
    }
}

// ── FileTree: indexed file source ───────────────────────────────────────────

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
    dirs: BTreeMap<PathBuf, DirEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum FolderScanError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl FolderScanError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Debug, Default)]
struct DirEntry {
    files: Vec<usize>,
    subdirs: BTreeSet<PathBuf>,
}

impl FileTree {
    pub fn new(files: Vec<FileEntry>) -> Self {
        let mut tree = Self {
            files,
            dirs: BTreeMap::new(),
        };
        tree.rebuild_index();
        tree
    }

    /// Build a FileTree by recursively walking the filesystem from `root`.
    /// Skips hidden files/directories (names starting with '.') and noise files.
    pub(crate) fn from_filesystem(root: &Path) -> Result<Self, FolderScanError> {
        let mut files = Vec::new();
        Self::walk_dir(root, root, &mut files)?;
        Ok(Self::new(files))
    }

    fn walk_dir(
        current: &Path,
        root: &Path,
        files: &mut Vec<FileEntry>,
    ) -> Result<(), FolderScanError> {
        let entries =
            fs::read_dir(current).map_err(|source| FolderScanError::io(current, source))?;

        for entry in entries {
            let entry = entry.map_err(|source| FolderScanError::io(current, source))?;
            let path = entry.path();

            // Skip hidden files and directories (including .bae/)
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            let metadata = entry
                .metadata()
                .map_err(|source| FolderScanError::io(path.clone(), source))?;

            if metadata.is_file() {
                if is_noise_file(&path) {
                    continue;
                }

                let size = metadata.len();

                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| FolderScanError::Other(format!("Failed to strip prefix: {e}")))?
                    .to_path_buf();

                files.push(FileEntry {
                    path: relative,
                    size,
                });
            } else if metadata.is_dir() {
                Self::walk_dir(&path, root, files)?;
            }
        }

        Ok(())
    }

    /// Files whose parent directory is exactly `dir` (not recursive).
    fn files_in_dir<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        let indices = self
            .dirs
            .get(&Self::normalize_dir(dir))
            .map(|entry| entry.files.clone())
            .unwrap_or_default();

        indices.into_iter().map(|idx| &self.files[idx])
    }

    /// Distinct immediate child directories of `dir`.
    fn immediate_subdirs(&self, dir: &Path) -> Vec<PathBuf> {
        self.dirs
            .get(&Self::normalize_dir(dir))
            .map(|entry| entry.subdirs.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// All files recursively under `dir` (inclusive).
    fn all_files_under<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        let mut indices = Vec::new();
        self.collect_file_indices_under(&Self::normalize_dir(dir), &mut indices);
        indices.into_iter().map(|idx| &self.files[idx])
    }

    fn rebuild_index(&mut self) {
        self.dirs.clear();
        self.dirs.entry(PathBuf::new()).or_default();

        for idx in 0..self.files.len() {
            let parent = {
                let file = &self.files[idx];
                Self::normalize_dir(file.path.parent().unwrap_or_else(|| Path::new("")))
            };
            self.dirs.entry(parent.clone()).or_default().files.push(idx);
            self.index_dir_path(&parent);
        }
    }

    fn index_dir_path(&mut self, dir: &Path) {
        let mut parent = PathBuf::new();
        self.dirs.entry(parent.clone()).or_default();

        for component in dir.components() {
            let child = parent.join(component.as_os_str());
            self.dirs
                .entry(parent.clone())
                .or_default()
                .subdirs
                .insert(child.clone());
            self.dirs.entry(child.clone()).or_default();
            parent = child;
        }
    }

    fn collect_file_indices_under(&self, dir: &Path, out: &mut Vec<usize>) {
        let Some(entry) = self.dirs.get(dir) else {
            return;
        };

        out.extend(entry.files.iter().copied());
        for subdir in &entry.subdirs {
            self.collect_file_indices_under(subdir, out);
        }
    }

    fn normalize_dir(dir: &Path) -> PathBuf {
        if dir.as_os_str().is_empty() || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        }
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

/// A CUE-paired audio file's probed codec identity.
enum CueCodecLabel {
    /// A codec bae can play back from a single-file CUE. Carries the
    /// `CUE+<codec>` label component (e.g. "FLAC", "APE").
    Supported(String),
    /// A readable codec that can't back single-file CUE playback (e.g. MP3,
    /// Vorbis). Carries the codec's display name for the invalid-candidate
    /// reason so the folder surfaces as skipped instead of aborting the scan.
    Unsupported(String),
    /// The file cleared the header-only magic check but FFmpeg can't identify a
    /// playable stream in it — a download truncated after the header, or
    /// otherwise corrupt audio. Surfaces the folder as a corrupt-audio invalid
    /// candidate instead of aborting the scan.
    Unprobeable,
}

/// Codec identity for a CUE-paired audio file, for the `CUE+<codec>` format
/// label. The label comes from FFmpeg's probe, never from the extension, because
/// containers such as MP4, Ogg, WAV, and AIFF don't prove the codec by filename.
///
/// `Err` is reserved for a non-UTF-8 path (which FFmpeg can't open at all). A
/// readable file whose codec bae can't play (`Ok(Unsupported)`) or that FFmpeg
/// can't probe (`Ok(Unprobeable)`) is not an error — each surfaces its folder as
/// an invalid candidate without aborting the whole watched-root walk.
fn cue_pair_codec_label(path: &Path) -> Result<CueCodecLabel, FolderScanError> {
    let path_str = path.to_str().ok_or_else(|| {
        FolderScanError::Other(format!("CUE audio path is not UTF-8: {}", path.display()))
    })?;
    let Some(probe) = crate::audio_codec::probe_audio_from_path(path_str) else {
        return Ok(CueCodecLabel::Unprobeable);
    };
    match probe.content_type {
        crate::util::content_type::ContentType::Flac
        | crate::util::content_type::ContentType::Ape
        | crate::util::content_type::ContentType::Alac
        | crate::util::content_type::ContentType::Pcm
        | crate::util::content_type::ContentType::WavPack
        | crate::util::content_type::ContentType::Dsd => Ok(CueCodecLabel::Supported(
            probe.content_type.display_name().to_string(),
        )),
        other => Ok(CueCodecLabel::Unsupported(other.display_name().to_string())),
    }
}

/// Check if a file is an image/artwork file
fn is_image_file(path: &Path) -> bool {
    ContentTypeHint::path_is_raster_image(path)
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

/// Whether `dir` holds audio files directly, by extension.
///
/// This drives tree-structure detection (leaf vs collection), not validation: a
/// directory of corrupt FLACs must still be detected as a candidate, so that
/// categorization can surface it as an *invalid* candidate with its reason
/// rather than have the folder vanish. Only 0-byte files are skipped — those are
/// empty placeholders, not audio.
fn tree_has_audio_files(tree: &FileTree, dir: &Path) -> bool {
    tree.files_in_dir(dir)
        .any(|f| is_audio_file(&f.path) && f.size > 0)
}

/// True when any immediate subdirectory of `dir` contains audio anywhere in its
/// subtree. The recursion matters: a shallow check would miss any collection
/// that wraps its releases under artist/label/discography dirs, making it look
/// audio-less to a caller asking "is this a container?".
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

/// True when `dir` or any descendant holds a partial-download marker. Checked
/// once a directory is identified as a leaf, so an in-progress release whose
/// markers live levels down (`Album/Disc 2/01.flac.part`) is still refused.
fn tree_has_partial_markers_deep(tree: &FileTree, dir: &Path) -> bool {
    tree.all_files_under(dir)
        .any(|f| is_partial_marker_file(&f.path))
}

/// True when EVERY audio-bearing immediate subdirectory of `dir` has a
/// disc-indicator name (`Disc N`, `CD N`, `Disk N`, `Side A/B`, `Part N`) — not
/// a majority, so one "Disc 1" next to a "Bonus Tracks" does not qualify. This
/// is what separates a true multi-disc release from an artist/discography/reissue
/// folder that merely has ≥2 audio-bearing children.
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

/// Whether `dir` is a candidate — one release. Prefers "zoomed out", stopping at
/// the shallowest directory that groups audio. A directory qualifies if it holds
/// audio files directly, or if it's a multi-disc release (every audio-bearing
/// subdir has a disc-indicator name). An artist / discography / reissue folder
/// with ≥2 audio-bearing children does NOT qualify — its children are
/// independent candidates, not parts of one release.
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

#[derive(Debug, Clone, Copy)]
struct DetectedCueAudioPair {
    cue_index: usize,
    audio_index: usize,
}

/// Shorthand for a failed-validation leaf carrying `reason`.
fn invalid(reason: InvalidReason) -> Result<CategorizeOutcome, FolderScanError> {
    Ok(CategorizeOutcome::Invalid(reason))
}

/// The directory holding a CUE sheet, where its `FILE` references resolve. A
/// CUE path with no parent is a filesystem impossibility for a scanned file,
/// so it's a hard scan error, not an invalid-candidate reason.
fn cue_parent_dir(cue_path: &Path) -> Result<&Path, FolderScanError> {
    cue_path
        .parent()
        .ok_or_else(|| FolderScanError::Other(format!("CUE file has no parent: {:?}", cue_path)))
}

/// Categorize a release root's files from a `FileTree`. `fs_root` is the folder
/// being imported — validation reads its actual bytes from disk. Returns
/// `Invalid(reason)` when the folder has audio but fails validation, so the
/// caller can surface why it can't be imported.
fn categorize_files_from_tree(
    tree: &FileTree,
    release_root: &Path,
    fs_root: &Path,
) -> Result<CategorizeOutcome, FolderScanError> {
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

        // Joined from the path's components rather than displayed, so the result is
        // `/`-separated on Windows too. A displayed `Path` uses the host's
        // separator, and this string is stored on the row and joined back onto a
        // directory by every other device in the library.
        let relative_path = relative_from_release
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        // The absolute path is fs_root + entry.path.
        let absolute_path = fs_root.join(&entry.path);

        if is_audio_file(&entry.path) {
            // Ok(false) is corruption (the candidate becomes Invalid); Err is a
            // genuine I/O fault (file vanished, permissions, flaky network
            // mount) — surface it rather than mis-label a system error as
            // corruption and silently drop the whole release.
            let valid = file_validation::is_valid_audio(&absolute_path).map_err(|e| {
                FolderScanError::Other(format!(
                    "Failed to validate audio file {absolute_path:?}: {e}"
                ))
            })?;
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
            // As with audio: Ok(false) is corruption, Err is a real I/O fault.
            let valid = file_validation::is_valid_image(&absolute_path).map_err(|e| {
                FolderScanError::Other(format!(
                    "Failed to validate image file {absolute_path:?}: {e}"
                ))
            })?;
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

    // Parse every CUE exactly once. A malformed sheet invalidates only this
    // candidate leaf; the watched root scan keeps walking siblings.
    let mut parsed_cues: HashMap<PathBuf, crate::cue_flac::CueSheet> = HashMap::new();
    for cue in &all_cue {
        match parse_cue_sheet(&cue.path) {
            Ok(sheet) => {
                parsed_cues.insert(cue.path.clone(), sheet);
            }
            Err(error) => {
                info!(
                    "Invalid candidate: failed to parse CUE {:?}: {error}",
                    cue.path
                );
                return invalid(InvalidReason::CueParseFailed {
                    path: cue.relative_path.clone(),
                });
            }
        }
    }

    // Detect CUE-backed releases by the CUE's own FILE directives. Every
    // referenced audio file must exist in the CUE's directory.
    let audio_by_path: HashMap<PathBuf, usize> = all_audio
        .iter()
        .enumerate()
        .map(|(index, audio)| (audio.path.clone(), index))
        .collect();
    let detected_pairs: Vec<DetectedCueAudioPair> = all_cue
        .iter()
        .enumerate()
        .map(|(cue_index, cue)| {
            let sheet = parsed_cues
                .get(&cue.path)
                .expect("parsed_cues is populated for every CUE");
            let file_references = sheet.audio_file_references();
            if file_references.is_empty() {
                return Ok(None);
            }
            let cue_dir = cue_parent_dir(&cue.path)?;
            let first_audio_path = cue_dir.join(file_references[0]);
            Ok(audio_by_path
                .get(&first_audio_path)
                .map(|audio_index| DetectedCueAudioPair {
                    cue_index,
                    audio_index: *audio_index,
                }))
        })
        .collect::<Result<Vec<_>, FolderScanError>>()?
        .into_iter()
        .flatten()
        .collect();
    let paired_cue_indices: HashSet<usize> =
        detected_pairs.iter().map(|pair| pair.cue_index).collect();

    // Every CUE FILE reference is part of the disc layout. If any referenced
    // audio is missing, refuse the candidate rather than import a release whose
    // layout points at absent audio.
    for (cue_index, cue) in all_cue.iter().enumerate() {
        let Some(cue_dir) = cue.path.parent() else {
            continue;
        };

        let cue_sheet = parsed_cues
            .get(&cue.path)
            .expect("parsed_cues is populated for every CUE");
        let unique_refs: HashSet<&str> = cue_sheet.audio_file_references().into_iter().collect();
        let references_missing = unique_refs.iter().any(|name| !cue_dir.join(name).exists());
        if references_missing {
            info!(
                "Invalid candidate: CUE {:?} references missing audio",
                cue.path
            );
            return invalid(InvalidReason::CueMissingAudio);
        }

        if paired_cue_indices.contains(&cue_index) {
            continue;
        }
    }

    let (audio, unpaired_cue_sheets) = if !detected_pairs.is_empty() {
        let mut pairs = Vec::new();
        let mut parsed_cues = parsed_cues;

        for pair in detected_pairs {
            let cue_file = all_cue[pair.cue_index].clone();
            let audio_file = all_audio[pair.audio_index].clone();

            let cue_sheet = parsed_cues
                .remove(&cue_file.path)
                .expect("detected CUE pair has a parsed CUE sheet");

            let cue_dir = cue_parent_dir(&cue_file.path)?;
            let mut audio_files = Vec::new();
            let mut seen = HashSet::new();
            for reference in cue_sheet.audio_file_references() {
                let audio_path = cue_dir.join(reference);
                let audio_index = audio_by_path.get(&audio_path).ok_or_else(|| {
                    FolderScanError::Other(format!(
                        "CUE reference missing after validation: {reference}"
                    ))
                })?;
                if seen.insert(*audio_index) {
                    audio_files.push(all_audio[*audio_index].clone());
                }
            }
            if audio_files.is_empty() {
                return invalid(InvalidReason::CueUnsupportedLayout);
            }

            let total_size = cue_file.size + audio_files.iter().map(|file| file.size).sum::<u64>();
            pairs.push(ScannedCueFlacPair {
                cue_file,
                audio_file,
                audio_files,
                cue_sheet,
                total_size,
            });
        }

        // CUE files with no playable audio references stay as documents.
        for (cue_index, cue) in all_cue.into_iter().enumerate() {
            if !paired_cue_indices.contains(&cue_index) {
                documents.push(cue);
            }
        }

        pairs.sort_by(|a, b| a.cue_file.relative_path.cmp(&b.cue_file.relative_path));
        let codec_label = match cue_pair_codec_label(&pairs[0].audio_file.path)? {
            CueCodecLabel::Supported(label) => label,
            CueCodecLabel::Unsupported(codec) => {
                info!(
                    "Invalid candidate: CUE audio codec {codec} not supported for single-file CUE playback"
                );
                return invalid(InvalidReason::CueUnsupportedCodec { codec });
            }
            CueCodecLabel::Unprobeable => {
                let path = pairs[0].audio_file.relative_path.clone();
                info!("Invalid candidate: CUE audio file could not be probed: {path}");
                return invalid(InvalidReason::CorruptAudioFile { path });
            }
        };
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
) -> Result<(), FolderScanError>
where
    F: FnMut(RawScanItem),
{
    if tree_is_leaf_directory(tree, dir) {
        // A leaf is a release, so markers anywhere under it mean the release
        // itself is incomplete. Don't emit and don't recurse — any disc-level
        // child would inherit the same problem.
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

        debug!("Found candidate leaf: {:?}", dir);

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
                // It looked like a release but failed validation. Surface it with
                // its reason so the user sees why the folder didn't import —
                // unlike the partial-markers check above, which truly suppresses.
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
pub fn scan_for_candidates_with_callback<F>(
    root: PathBuf,
    mut on_item: F,
) -> Result<(), FolderScanError>
where
    F: FnMut(ScanItem),
{
    debug!("Scanning for candidates in: {:?}", root);
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

/// Walk one release directory recursively and categorize its files, preserving
/// relative paths, into audio (CUE/FLAC pairs or track files), artwork, and
/// documents. Unrecognized file types are ignored.
pub fn collect_release_candidate_files(
    release_root: &Path,
) -> Result<CategorizedFiles, crate::import::ImportError> {
    let tree = FileTree::from_filesystem(release_root)?;
    // An invalid folder can't be imported: surface its typed reason so the
    // import-commit caller fails with why the folder is unusable.
    match categorize_files_from_tree(&tree, &PathBuf::new(), release_root)? {
        CategorizeOutcome::Valid(files) => Ok(files),
        CategorizeOutcome::Invalid(reason) => Err(reason.into()),
    }
}

#[cfg(test)]
mod tests;
