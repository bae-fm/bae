//! Extraction inputs for the `Release` source (re-identify): read a library
//! release's rows and files into a rip reading and its audio facts, and the
//! artwork paths for the OCR pass. Reading the rip artifacts themselves lives
//! in `import::discid`.

use bae_db::{Database, LibraryImageType};
use bae_diagnostics::Diagnostics;
use bae_store::DbError;
use bae_util::task_failure::TaskFailed;
use std::path::PathBuf;
use tracing::{debug, warn};

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
    /// Reading the rip files panicked, reported as the bug it is.
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

/// Read the release's local files the way a folder's scan reads its rip files
/// (`folder_scanner::categorize_library_files`), so a sheet is
/// bound to the audio the scan would bind it to and the release hashes the
/// discs its folder did. The track count and lengths come from the release's
/// track rows, not the files — those rows are the user's truth. A panic
/// reading the files is reported to `diagnostics`.
///
/// A cloud-only release with no local copy has no files to read, so its
/// reading proves nothing and computes no disc ID.
pub async fn resolve_release_identity(
    database: &Database,
    diagnostics: &Diagnostics,
    release_id: &str,
) -> Result<ReleaseIdentity, ReleaseIdentityError> {
    database
        .find_release_by_id(release_id)
        .await?
        .ok_or_else(|| ReleaseIdentityError::NotFound { release_id: release_id.to_string() })?;
    let files = database.get_files_with_originals_for_release(release_id).await?;
    let tracks = database.get_tracks_for_release(release_id).await?;
    // The lengths a pick of this release is laid out against, so a document
    // is read against the discs these tracks are.
    let track_lengths_ms: Vec<u64> = tracks.iter().map(|track| track.duration_ms).collect();
    let (rip, formats) =
        bae_util::task_failure::blocking("read a release's rip files", move || {
            let categorized = crate::folder_scanner::categorize_library_files(
                files.into_iter().filter_map(|(file, original)| local_file(file, original?)),
            );
            let formats: Vec<bae_types::audio_format::AudioFormat> = categorized
                .audio()
                .filter_map(|file| file.source_audio.as_ref())
                .map(|audio| audio.format.clone())
                .collect();
            (crate::rip_reading::read_library_rip_artifacts(&categorized), formats)
        })
        .await
        .map_err(|failed| diagnostics.bug(failed))?;
    let audio =
        bae_identify::signals::AudioFacts::of(tracks.len() as u32, track_lengths_ms, &formats);

    Ok(ReleaseIdentity { rip, audio })
}

/// A release file as the file at `original`, the path this device recorded
/// at import, with the audio facts the import stored; `None` when nothing is
/// there now. A release imported elsewhere records no originals here, so it
/// has no such files. Blocking: it reads the file's metadata.
fn local_file(
    file: bae_db::DbFile,
    original: PathBuf,
) -> Option<crate::folder_scanner::ScannedFile> {
    let metadata = match std::fs::metadata(&original) {
        Ok(metadata) => metadata,
        Err(error) => {
            debug!("release file {} is not at {}: {error}", file.id, original.display());
            return None;
        }
    };
    let modified_at_ns = match crate::folder_scanner::file_modified_at_ns(&original, &metadata) {
        Ok(modified_at_ns) => modified_at_ns,
        Err(error) => {
            warn!(
                "release file {} at {} has no usable modification time: {error}",
                file.id,
                original.display()
            );
            return None;
        }
    };
    let mut scanned = crate::folder_scanner::ScannedFile::new(
        original,
        file.original_filename,
        metadata.len(),
        modified_at_ns,
    );
    scanned.source_audio = file.source_audio.map(|audio| crate::folder_scanner::ScannedAudio {
        content_type: audio.content_type,
        duration_ms: audio.duration_ms,
        format: audio.format,
    });
    Some(scanned)
}

/// The artwork the re-identify OCR pass reads: the release's cover, plus every
/// release-attached image file that resolves to a path on this device. A
/// cloud-only image has no local copy and is skipped — OCR can't run on it.
///
/// The cover's bytes live in coven's store, which exposes no path bae may
/// compute, so they are read out and staged into a temp dir. That dir's guard
/// comes back alongside the paths and must outlive the OCR pass; `None` when the
/// release has no cover. A panic staging the cover is reported to
/// `diagnostics`.
pub(crate) async fn resolve_release_artwork_paths(
    store: &bae_store::Store,
    database: &Database,
    diagnostics: &Diagnostics,
    release_id: &str,
) -> Result<(Vec<PathBuf>, Option<tempfile::TempDir>), DbError> {
    let files = database.get_files_with_originals_for_release(release_id).await?;
    let mut paths: Vec<PathBuf> = Vec::new();

    // The cover is one of several OCR inputs (the image files below are the
    // others), so a read/staging failure skips just the cover, logged, rather than
    // failing the whole resolve.
    let cover_staging = match database
        .library_image_file(release_id, &LibraryImageType::Cover)
        .await
    {
        Ok(Some(file)) => match store.read_file(&file).await {
            Ok(bytes) => {
                let staging_release = release_id.to_string();
                match bae_util::task_failure::blocking("stage a cover for reading", move || {
                    stage_cover_for_ocr(&staging_release, &bytes)
                })
                .await
                {
                    Ok(Some((dir, cover_path))) => {
                        paths.push(cover_path);
                        Some(dir)
                    }
                    Ok(None) => None,
                    // A staging that panicked skips the cover, as one
                    // that failed does, reported as the bug it is.
                    Err(failed) => {
                        diagnostics.bug(failed);
                        None
                    }
                }
            }
            Err(e) => {
                warn!(
                    "artwork OCR: reading cover for release {release_id} failed: {e}; skipping cover"
                );
                None
            }
        },
        Ok(None) => {
            debug!("artwork OCR: release {release_id} has no cover file; skipping cover staging");
            None
        }
        Err(e) => {
            warn!(
                "artwork OCR: reading cover for release {release_id} failed: {e}; skipping cover"
            );
            None
        }
    };

    // In-folder artwork (cover.jpg and the like), resolved to this device's
    // original file whether or not it has been attached for upload.
    for (file, original) in &files {
        if !file.content_type.is_image() {
            continue;
        }
        if let Some(p) = original.clone() {
            match tokio::fs::try_exists(&p).await {
                Ok(true) => paths.push(p),
                Ok(false) => warn!(
                    "artwork OCR: registered image file {} resolved to missing path {}; skipping image",
                    file.id,
                    p.display()
                ),
                Err(error) => warn!(
                    "artwork OCR: registered image file {} at {} could not be checked: {error}; \
                     skipping image",
                    file.id,
                    p.display()
                ),
            }
        }
    }

    Ok((paths, cover_staging))
}

/// Write cover bytes to a temp file the OCR reader can open, returning the
/// temp-dir guard and the file's path. An IO failure skips the cover, logged,
/// rather than failing the whole resolve. Blocking; run it off the runtime.
fn stage_cover_for_ocr(release_id: &str, bytes: &[u8]) -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            warn!("artwork OCR: cover staging dir for release {release_id} failed: {e}; skipping cover");
            return None;
        }
    };
    let cover_path = dir.path().join("cover");
    if let Err(e) = std::fs::write(&cover_path, bytes) {
        warn!("artwork OCR: staging cover for release {release_id} failed: {e}; skipping cover");
        return None;
    }
    Some((dir, cover_path))
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
