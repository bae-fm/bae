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
use std::path::PathBuf;
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
    /// One of the release's rip files could not be read.
    #[error(transparent)]
    Document(#[from] LibraryDocumentError),
    /// A rip file of the release could not be read.
    #[error(transparent)]
    Unreadable(#[from] crate::ImportError),
    /// Reading the rip files panicked, reported as the bug it is.
    #[error(transparent)]
    Panicked(#[from] TaskFailed),
}

/// Why a library release's artwork could not be listed or read for the OCR
/// pass.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseArtworkError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("bae could not read the release's cover: {0}")]
    CoverRead(coven::FileReadError),
    /// coven could not read one of the release's image files.
    #[error("bae could not read the release's image {name}: {error}")]
    ImageRead { name: String, error: DbError },
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
    let documents = LibraryDocuments::read(records, &lease, &read).await?;
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

/// A file of a library release that the OCR pass reads as an image: coven
/// holds its bytes.
#[derive(Debug, Clone)]
pub(crate) enum LibraryImageFile {
    /// The release's cover.
    Cover(coven::FileRef),
    /// An image among the release's files.
    ReleaseFile { id: String, name: String },
}

/// The artwork the re-identify OCR pass reads: the release's cover, plus every
/// image file of the release, each named by where coven has it. The pass
/// reads each one's bytes as it reaches it ([`read_library_image`]).
pub(crate) async fn resolve_release_artwork(
    records: &Records,
    release_id: &str,
) -> Result<Vec<(PathBuf, LibraryImageFile)>, ReleaseArtworkError> {
    let files = records.get_files_for_release(release_id).await?;
    let mut images = Vec::new();
    match records.library_image_file(release_id, &LibraryImageType::Cover).await? {
        Some(file) => images.push((PathBuf::from("cover"), LibraryImageFile::Cover(file))),
        None => {
            debug!("artwork OCR: release {release_id} has no cover file; skipping the cover");
        }
    }
    // In-folder artwork (cover.jpg and the like).
    for file in files.iter().filter(|file| file.content_type.is_image()) {
        images.push((
            PathBuf::from(&file.original_filename),
            LibraryImageFile::ReleaseFile {
                id: file.id.clone(),
                name: file.original_filename.clone(),
            },
        ));
    }
    Ok(images)
}

/// The bytes of `image`, which coven reads from wherever it has them, in one
/// place of `budget`. An image coven can't read fails the read.
pub(crate) async fn read_library_image(
    store: &bae_store::Store,
    records: &Records,
    budget: &coven::FileBudget,
    image: &LibraryImageFile,
) -> Result<Vec<u8>, ReleaseArtworkError> {
    let lease = bae_store::lease_one(budget).await;
    match image {
        LibraryImageFile::Cover(file) => {
            store.read_file(file, &lease).await.map_err(ReleaseArtworkError::CoverRead)
        }
        LibraryImageFile::ReleaseFile { id, name } => records
            .read_release_file(id, &lease)
            .await
            .map_err(|error| ReleaseArtworkError::ImageRead { name: name.clone(), error }),
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
