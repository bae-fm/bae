//! Extraction inputs for the `Release` source (re-identify): read a library
//! release's rows and files into a rip reading and its audio facts, and the
//! artwork paths for the OCR pass. Reading the rip artifacts themselves lives
//! in `import::discid`.

use crate::library_documents::{LibraryDocumentError, LibraryDocuments};
use bae_diagnostics::Diagnostics;
use bae_records::LibraryImageType;
use bae_records::Records;
use bae_store::DbError;
use bae_util::task_failure::TaskFailed;
use std::path::{Path, PathBuf};
use tracing::debug;

/// What a release already in the library was copied from: what its rip
/// artifacts say, and what its audio is.
pub struct ReleaseIdentity {
    pub(crate) rip: crate::rip_reading::RipReading,
    pub(crate) audio: bae_identify::signals::AudioFacts,
}

/// Why a library release's identity could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseIdentityError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("release '{release_id}' not found")]
    NotFound { release_id: String },
    /// coven could not read one of the release's rip files.
    #[error("bae could not read the release's file {name}: {error}")]
    FileRead { name: String, error: DbError },
    /// A rip file of the release could not be read.
    #[error(transparent)]
    Unreadable(#[from] crate::ImportError),
    /// Reading the rip files panicked, reported as the bug it is.
    #[error(transparent)]
    Panicked(#[from] TaskFailed),
}

/// Why a library release's artwork could not be gathered for the OCR pass.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseArtworkError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("bae could not read the release's cover: {0}")]
    CoverRead(coven::FileReadError),
    /// coven could not read one of the release's image files.
    #[error("bae could not read the release's image {name}: {error}")]
    ImageRead { name: String, error: DbError },
    /// bae could not write the copies of the artwork the OCR reader opens.
    #[error("bae could not copy the release's artwork to read it: {0}")]
    Staging(std::io::Error),
    /// Staging the artwork panicked, reported as the bug it is.
    #[error(transparent)]
    Panicked(#[from] TaskFailed),
}

impl ReleaseIdentity {
    /// The audio a catalog release is read against for this release: its
    /// tracks' stored lengths, its files' origin and their Disc IDs.
    pub fn read_audio(self) -> Result<bae_catalogs::reading::ReadAudio, crate::ImportError> {
        let lengths = bae_catalogs::reading::MeasuredLengths::new(self.audio.track_lengths_ms)
            .ok_or_else(|| crate::ImportError::Internal {
                detail: "a library release with no tracks has nothing to read a release against"
                    .to_string(),
            })?;
        Ok(self.rip.read_audio(lengths))
    }
}

/// Read the release's files the way a folder's scan reads its rip files
/// (`folder_scanner::categorize_library_files`), so the function binds each
/// sheet to the audio the person chose for it, or else to the audio the scan
/// would choose, and the release hashes the discs its folder did. The track
/// count and lengths come from the release's track rows, not the files: the
/// person sets those rows, and the files may not match them. The function
/// reports a panic reading the files to `diagnostics`.
///
/// coven reads each file the reading opens (`rip_reading::opens_library_file`)
/// from wherever it has it: the original on this device, its cache or
/// storage. A file coven can't read, such as an original that moved or
/// changed since its import, or a file only on another device, fails the
/// reading, and so does a text document over the limit bae reads whole.
/// Audio is read by the facts its import stored, so none of its bytes are
/// read.
pub async fn resolve_release_identity(
    tables: &crate::tables::ImportTables,
    records: &Records,
    budget: &coven::FileBudget,
    diagnostics: &Diagnostics,
    release_id: &str,
) -> Result<ReleaseIdentity, ReleaseIdentityError> {
    records
        .find_release_by_id(release_id)
        .await?
        .ok_or_else(|| ReleaseIdentityError::NotFound { release_id: release_id.to_string() })?;
    let files = records.get_files_for_release(release_id).await?;
    let tracks = records.get_tracks_for_release(release_id).await?;
    // The lengths a pick of this release is laid out against, so a document
    // is read against the discs these tracks are.
    let track_lengths_ms: Vec<u64> = tracks.iter().map(|track| track.duration_ms).collect();
    let sheet_decisions = tables.load_release_sheet_decisions(release_id).await?;
    let lease = bae_store::lease_one(budget).await;
    let read: Vec<&bae_records::DbFile> =
        files.iter().filter(|file| crate::rip_reading::opens_library_file(&file.role)).collect();
    let documents =
        LibraryDocuments::read(records, &lease, &read).await.map_err(|error| match error {
            LibraryDocumentError::Coven { name, error } => {
                ReleaseIdentityError::FileRead { name, error }
            }
            refused @ LibraryDocumentError::Refused { .. } => {
                ReleaseIdentityError::Unreadable(refused.into())
            }
        })?;
    drop(lease);
    let library_files: Vec<_> = files.into_iter().map(library_file).collect();
    let (rip, formats) = diagnostics
        .blocking("read a release's rip files", move || {
            let categorized = crate::folder_scanner::categorize_library_files(
                library_files,
                &documents,
                sheet_decisions,
            );
            let formats: Vec<bae_types::audio_format::AudioFormat> = categorized
                .audio()
                .filter_map(|file| file.source_audio.as_ref())
                .map(|audio| audio.format.clone())
                .collect();
            Ok::<_, crate::ImportError>((
                crate::rip_reading::read_library_rip_artifacts(&categorized, &documents)?,
                formats,
            ))
        })
        .await??;
    let audio =
        bae_identify::signals::AudioFacts::of(tracks.len() as u32, track_lengths_ms, &formats);

    Ok(ReleaseIdentity { rip, audio })
}

/// A release file as a scanned file, with the audio facts the import stored
/// and the role it stored.
pub(crate) fn library_file(
    file: bae_records::DbFile,
) -> (crate::folder_scanner::ScannedFile, bae_records::ReleaseFileRole) {
    let mut scanned =
        crate::folder_scanner::ScannedFile::library(file.original_filename, file.file_size);
    scanned.source_audio = file.source_audio.map(|audio| crate::folder_scanner::ScannedAudio {
        content_type: audio.content_type,
        container: audio.container,
        duration_ms: audio.duration_ms,
        format: audio.format,
    });
    (scanned, file.role)
}

/// The artwork the re-identify OCR pass reads: the release's cover, plus every
/// image file of the release. coven reads each from wherever it has it, and
/// one coven can't read fails the gathering.
///
/// The OCR reader opens paths, so the function stages a copy of each image
/// in a temp folder, whose guard comes back alongside the paths and must
/// outlive the OCR pass. A panic staging the artwork is reported to
/// `diagnostics`.
pub(crate) async fn resolve_release_artwork_paths(
    store: &bae_store::Store,
    records: &Records,
    budget: &coven::FileBudget,
    diagnostics: &Diagnostics,
    release_id: &str,
) -> Result<(Vec<PathBuf>, StagedFiles), ReleaseArtworkError> {
    let files = records.get_files_for_release(release_id).await?;
    let staged = diagnostics
        .blocking("make a folder for a release's artwork", StagedFiles::create)
        .await?
        .map_err(ReleaseArtworkError::Staging)?;
    let lease = bae_store::lease_one(budget).await;
    let mut paths: Vec<PathBuf> = Vec::new();

    match records.library_image_file(release_id, &LibraryImageType::Cover).await? {
        Some(file) => {
            let bytes =
                store.read_file(&file, &lease).await.map_err(ReleaseArtworkError::CoverRead)?;
            let path = staged.cover();
            staged
                .write(diagnostics, path.clone(), bytes)
                .await?
                .map_err(ReleaseArtworkError::Staging)?;
            paths.push(path);
        }
        None => {
            debug!("artwork OCR: release {release_id} has no cover file; skipping cover staging");
        }
    }

    // In-folder artwork (cover.jpg and the like).
    for file in files.iter().filter(|file| file.content_type.is_image()) {
        let bytes = records.read_release_file(&file.id, &lease).await.map_err(|error| {
            ReleaseArtworkError::ImageRead { name: file.original_filename.clone(), error }
        })?;
        let path = staged.release_file(&file.original_filename);
        staged
            .write(diagnostics, path.clone(), bytes)
            .await?
            .map_err(ReleaseArtworkError::Staging)?;
        paths.push(path);
    }

    Ok((paths, staged))
}

/// A temp folder holding copies of a release's files that coven read, for
/// readers that open a path. The release's files sit below `files/` at the
/// paths their names in the release give, each name written as coven writes
/// it (see [`bae_records::stored_name`]), so a sheet finds the audio its
/// `FILE` lines name where it did in the release's folder. The folder goes
/// when this drops.
pub(crate) struct StagedFiles {
    dir: tempfile::TempDir,
}

impl StagedFiles {
    /// A new, empty folder. Blocking.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: temporary copies of a release's files for readers that open a path, which coven does not own"
    )]
    pub(crate) fn create() -> std::io::Result<Self> {
        Ok(Self { dir: tempfile::tempdir()? })
    }

    /// Where the release file named `name` in its release is staged.
    pub(crate) fn release_file(&self, name: &str) -> PathBuf {
        self.dir.path().join("files").join(bae_records::stored_name::file_path(name).local_path())
    }

    /// Where the release's cover is staged.
    fn cover(&self) -> PathBuf {
        self.dir.path().join("cover")
    }

    /// Write `bytes` to `path`, one of this folder's paths, making the
    /// folders above it.
    pub(crate) async fn write(
        &self,
        diagnostics: &Diagnostics,
        path: PathBuf,
        bytes: Vec<u8>,
    ) -> Result<std::io::Result<()>, TaskFailed> {
        diagnostics.blocking("stage a release's file", move || write_file(&path, &bytes)).await
    }
}

/// Create a file at `path`, making the folders above it. Blocking.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: temporary copies of a release's files for readers that open a path, which coven does not own"
)]
fn create_file(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::File::create(path)
}

/// Write `bytes` to a file at `path`, making the folders above it.
/// Blocking.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    create_file(path)?.write_all(bytes)
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
