//! The files a scan finds in a folder: each file and the role it plays, and
//! a release's files with the content hash they are known by. The audio a
//! track sheet is bound to is settled in `sheet_bindings`.

use super::*;

// ── Public types ────────────────────────────────────────────────────────────

/// A file discovered during folder scanning
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScannedFile {
    /// Absolute filesystem path. A scanned file is always on disk.
    pub path: PathBuf,
    /// The file's identity within the release (e.g. `CD1/CDImage.ape`) — used
    /// as the HashMap key throughout the import pipeline and as
    /// `DbFile.original_filename`. Two files may share a bare filename; they
    /// never share a relative_path within one release
    /// ([`InvalidReason::SameName`] refuses a release where they would).
    ///
    /// Each component is its name as `coven::path_text` shows it, so a name
    /// that isn't UTF-8 is its escaped text (`Caf\xE9.flac`): what the library
    /// syncs and shows. `path` is what opens the file.
    ///
    /// Always `/`-separated, on every platform: it is stored and synced, and
    /// whichever device exports the release or makes it local writes the file
    /// under these names (see [`bae_db::stored_name`]), so it cannot
    /// carry the writing platform's separator.
    pub relative_path: String,
    /// File size in bytes
    pub size: u64,
    pub modified_at_ns: i64,
    /// Directory prefix of relative_path (e.g. "Disc 1/"). `None` when the
    /// file is at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// File name without directory prefix.
    pub file_name: String,
    pub source_audio: Option<ScannedAudio>,
}

impl ScannedFile {
    pub fn new(path: PathBuf, relative_path: String, size: u64, modified_at_ns: i64) -> Self {
        let (dir_prefix, file_name) = match relative_path.rfind('/') {
            Some(idx) => {
                (Some(relative_path[..=idx].to_string()), relative_path[idx + 1..].to_string())
            }
            None => (None, relative_path.clone()),
        };
        Self {
            path,
            relative_path,
            size,
            modified_at_ns,
            dir_prefix,
            file_name,
            source_audio: None,
        }
    }

    /// The version of the file the scan read: where it is, with the size and
    /// modification time it had then.
    pub fn version(&self) -> FileVersion {
        FileVersion {
            path: self.path.clone(),
            size: self.size,
            modified_at_ns: self.modified_at_ns,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_flac_audio(mut self) -> Self {
        self.source_audio = Some(ScannedAudio {
            content_type: bae_util::content_type::ContentType::Flac,
            duration_ms: 5_000,
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        });
        self
    }
}

/// One version of a file on disk: its absolute path, and the size and
/// modification time (nanoseconds since the Unix epoch) that tell the bytes
/// there from whatever is written at that path later. A reader that caches
/// what it read from the file keys it by this, so a replaced file is new
/// content without the reader looking at the file again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FileVersion {
    pub path: PathBuf,
    pub size: u64,
    pub modified_at_ns: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScannedAudio {
    pub content_type: bae_util::content_type::ContentType,
    pub duration_ms: u64,
    pub format: bae_types::audio_format::AudioFormat,
}

/// The job the scan proposes for a file. Every file the scan finds carries
/// exactly one: the scan *proposes*, so nothing is discarded and no folder is
/// refused because a filename disagrees with a sheet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum FileRole {
    /// Playable audio.
    Audio,
    /// A track sheet (CUE) that parsed, carrying its parsed sheet, what its
    /// `FILE` directive resolved to, and which disc of the release its entries
    /// become.
    TrackSheet { sheet: crate::cue_sheet::CueSheet, binding: SheetBinding, disc: SheetDisc },
    /// An image the folder carries. Which one leads the release is not a
    /// property of the file: it is the cover choice, answered by the stored
    /// row, then the picked release's own art, then these ranked by name.
    Artwork,
    /// A file named as an image whose bytes do not decode as one: zero bytes,
    /// truncated, or overwritten. It never leads the release or sits among
    /// its artwork, and the import stores it as plain bytes, so nothing in the
    /// library reads it as a picture. The folder is the release, so it is
    /// carried and comes back on export like every other file.
    DamagedImage,
    /// Readable evidence: a rip log, a tracklist, a playlist, or a CUE that
    /// could not be parsed as a sheet.
    Document,
    /// In the folder and carried with the release, unrecognized: a scene
    /// sidecar (`.nfo`, `.sfv`, `.md5`), a stray video, a file with no
    /// extension. The folder is the release, so it imports, uploads, and comes
    /// back on export like everything else.
    Other,
}

/// A file the scan found, and its role.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CandidateFile {
    pub file: ScannedFile,
    pub role: FileRole,
}

impl CandidateFile {
    /// A cover action exists only for artwork the cover decoder supports.
    pub fn cover_choice(&self) -> Option<crate::CoverChoice> {
        (matches!(self.role, FileRole::Artwork)
            && bae_util::content_type_hint::ContentTypeHint::path_is_supported_cover(
                &self.file.path,
            ))
        .then(|| crate::CoverChoice::Local {
            file_id: self.file.relative_path.clone(),
            file: self.file.version(),
        })
    }
}

/// A release's files, each carrying the role the scan proposed for it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CategorizedFiles {
    /// Every file under the release root, in `relative_path` order, each with
    /// the role the scan proposed. All of them are the release's — see
    /// [`Self::release_files`].
    pub files: Vec<CandidateFile>,
    /// The folders a release read from several is made of, in play order,
    /// each a run of discs of its own. Empty for a release read from one
    /// folder, which numbers no discs of its own.
    pub parts: Vec<ReleasePart>,
}

/// One folder of a release read from several: where it is, and the prefix
/// every one of its files' release paths starts with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReleasePart {
    pub folder: PathBuf,
    /// `/`-terminated, or empty for the folder the release is rooted at.
    pub prefix: String,
}

impl ReleasePart {
    /// The folder's own name, as the release's source list shows it.
    pub fn name(&self) -> String {
        bae_util::path_text::name_text(&self.folder)
    }
}

/// Which part of `parts` the file at `relative_path` belongs to: the one with
/// the longest prefix it starts with.
pub(crate) fn part_of(parts: &[ReleasePart], relative_path: &str) -> Option<usize> {
    parts
        .iter()
        .enumerate()
        .filter(|(_, part)| relative_path.starts_with(&part.prefix))
        .max_by_key(|(_, part)| part.prefix.len())
        .map(|(index, _)| index)
}

impl CategorizedFiles {
    /// The release's audio files, in `relative_path` order.
    pub fn audio(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Audio))
            .map(|entry| &entry.file)
    }

    /// What the candidate's audio is, summarized over every audio file whose
    /// format the scan read.
    pub fn source_audio_summary(&self) -> Option<bae_db::source_audio::SourceAudioSummary> {
        use bae_db::source_audio::{
            SourceAudioDescriptor, SourceAudioLayout, SourceAudioSummary, SummarizedAudio,
        };
        let carved_audio: std::collections::HashSet<&str> = self
            .carving_sheets()
            .into_iter()
            .flat_map(|sheet| sheet.audio_files)
            .map(|(_, audio)| audio.relative_path.as_str())
            .collect();
        SourceAudioSummary::from_files(self.audio().filter_map(|file| {
            file.source_audio.as_ref().map(|audio| SummarizedAudio {
                descriptor: SourceAudioDescriptor {
                    layout: if carved_audio.contains(file.relative_path.as_str()) {
                        SourceAudioLayout::Cue
                    } else {
                        SourceAudioLayout::File
                    },
                    format: audio.format.clone(),
                },
                duration_ms: audio.duration_ms,
            })
        }))
    }

    /// The release's images — the proposed cover and everything else.
    pub fn artwork(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Artwork))
            .map(|entry| &entry.file)
    }

    /// Artwork offered by cover pickers; other images remain in `artwork`.
    pub fn cover_files(&self) -> impl Iterator<Item = &CandidateFile> {
        self.files.iter().filter(|entry| entry.cover_choice().is_some())
    }

    /// The release's readable evidence files.
    pub fn documents(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Document))
            .map(|entry| &entry.file)
    }

    /// Every file the release carries, in `relative_path` order — the rows the
    /// import writes, the files coven uploads, and the set
    /// [`Self::content_hash`] covers. The folder is the release: what you
    /// import is what is stored and what comes back on export, so nothing the
    /// walk keeps is left behind.
    ///
    /// One definition, read by both the payload and the hash. Computing them
    /// from parallel lists would let them drift, and a hash that stopped
    /// describing the payload is the bug that duplicates a release instead of
    /// replacing it.
    pub fn release_files(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files.iter().map(|entry| &entry.file)
    }

    /// Every parsed track sheet, bound or not.
    pub fn track_sheets(&self) -> impl Iterator<Item = TrackSheetFile<'_>> {
        self.files.iter().filter_map(|entry| match &entry.role {
            FileRole::TrackSheet { sheet, binding, disc } => {
                Some(TrackSheetFile { file: &entry.file, sheet, binding, disc: *disc })
            }
            _ => None,
        })
    }

    /// The track sheets whose `FILE` directives resolved, each with the audio
    /// it describes, in the sheets' `relative_path` order.
    pub fn bound_sheets(&self) -> Vec<BoundTrackSheet<'_>> {
        self.track_sheets().filter_map(|sheet| self.bound_sheet(sheet)).collect()
    }

    /// One parsed sheet with the audio its binding names, or `None` when it
    /// describes nothing.
    ///
    /// The binding is settled against the folder's audio — every file it names
    /// has the audio role — so a name that finds no audio here is a candidate
    /// written by something other than the scan.
    pub fn bound_sheet<'a>(&'a self, sheet: TrackSheetFile<'a>) -> Option<BoundTrackSheet<'a>> {
        if !sheet.binding.is_resolved() {
            return None;
        }
        let audio_files = sheet
            .binding
            .audio_files()?
            .iter()
            .map(|audio| {
                let file = self
                    .audio()
                    .find(|file| file.relative_path == audio.file_id)
                    .expect("a settled sheet binding names this folder's audio");
                (audio.file_reference.as_str(), file)
            })
            .collect();
        Some(BoundTrackSheet {
            file: sheet.file,
            sheet: sheet.sheet,
            audio_files,
            disc: sheet.disc,
        })
    }

    /// The bound sheets that carve the release's tracks — the ones the
    /// tracklist, the disc IDs and the file-metadata seed are all read from.
    pub fn carving_sheets(&self) -> Vec<BoundTrackSheet<'_>> {
        self.bound_sheets().into_iter().filter(BoundTrackSheet::carves).collect()
    }

    /// Total playable tracks, including loose audio beside carving sheets.
    pub fn track_count(&self) -> u32 {
        u32::try_from(crate::audio_layout::audio_units(self).len())
            .expect("release track count fits u32")
    }

    /// This release's audio file paths, in `relative_path` order. The file metadata
    /// import path reads embedded cover art from these.
    pub fn audio_paths(&self) -> Vec<PathBuf> {
        self.audio().map(|file| file.path.clone()).collect()
    }

    /// Stable scan fingerprint of this release: a SHA-256 over the relative
    /// path, size, and modification time of every file in
    /// [`Self::release_files`], sorted so the result is independent of discovery
    /// order. Absolute location is deliberately absent, so a folder moved or
    /// renamed keeps it: what this device records of each import
    /// (`local_release_origin`), which places the folder in Done. Never
    /// identifies a release.
    ///
    /// It fingerprints exactly what the release carries: same iterator as the
    /// payload, so the fingerprint cannot describe a different set of files
    /// than the one that gets stored.
    ///
    /// It fingerprints **files**, never decisions. A sheet's binding and disc
    /// are user decisions stored under this hash, so a hash that moved when one
    /// of them changed would orphan the very row it addresses on every edit.
    /// That holds because no decision removes a file from
    /// [`Self::release_files`]: the folder is the release, so every file
    /// imports, uploads and comes back on export.
    pub fn content_hash(&self) -> String {
        content_hash_of(self.release_files())
    }

    /// Identity of the files whose tags populate the file-metadata preview.
    /// Changing an audio file's metadata changes it.
    pub fn file_metadata_identity(&self) -> String {
        content_hash_of(self.audio())
    }
}

/// SHA-256 over each file's sorted relative path, size, and modification time.
/// This is a scan fingerprint, not a digest of the file bytes. The free function
/// [`CategorizedFiles::content_hash`] is the method form of. Separate because the
/// scan needs a candidate's fingerprint while it is still assembling the candidate,
/// to look up the bindings stored under it.
pub(super) fn content_hash_of<'a>(files: impl Iterator<Item = &'a ScannedFile>) -> String {
    let mut entries: Vec<&ScannedFile> = files.collect();
    entries.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut hasher = Sha256::new();
    for file in entries {
        hasher.update((file.relative_path.len() as u64).to_le_bytes());
        hasher.update(file.relative_path.as_bytes());
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.modified_at_ns.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
