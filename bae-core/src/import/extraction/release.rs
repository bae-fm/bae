//! Extraction inputs for the `Release` source (re-identify): read a library
//! release's rows and files into a rip reading and its audio facts, and the
//! artwork paths for the OCR pass. Reading the rip artifacts themselves lives
//! in `import::discid`.

use crate::db::{Database, DbError, LibraryImageType};
use crate::diagnostics::Diagnostics;
use bae_util::task_failure::TaskFailed;
use std::path::PathBuf;
use tracing::{debug, warn};

/// What a release already in the library was copied from: what its rip
/// artifacts say, and what its audio is.
pub(crate) struct ReleaseIdentity {
    pub(crate) rip: crate::import::discid::RipReading,
    pub(crate) audio: crate::identify::signals::AudioFacts,
}

/// Why a library release's identity could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReleaseIdentityError {
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
    pub(crate) fn read_audio(
        self,
    ) -> Result<bae_catalogs::reading::ReadAudio, crate::import::ImportError> {
        let lengths = bae_catalogs::reading::MeasuredLengths::new(self.audio.track_lengths_ms)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: "a library release with no tracks has nothing to read a release against"
                    .to_string(),
            })?;
        Ok(self.rip.read_audio(lengths))
    }
}

/// Read the release's local files, pick out its rip documents, track sheets
/// and audio, and read them the way a folder's are read (see
/// [`crate::import::discid::read_rip_artifacts_from_paths`]). The track count
/// and lengths come from the release's track rows, not the files — those rows
/// are the user's truth. A panic reading the files is reported to
/// `diagnostics`.
///
/// A cloud-only release with no local copy has no files to read, so its
/// reading proves nothing and computes no disc ID.
pub(crate) async fn resolve_release_identity(
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
    // Original paths are the ones this device recorded at import.
    // A release imported elsewhere has no originals here, so yields no paths or
    // disc ID.
    let mut documents = Vec::new();
    let mut cue_paths = Vec::new();
    let mut audio_files = Vec::new();
    for (f, original) in files {
        if let Some(path) = original {
            if !path.exists() {
                continue;
            }
            if crate::import::discid::is_rip_document(&path) {
                documents.push(path.clone());
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cue"))
            {
                cue_paths.push(path.clone());
            }
            if let Some(source_audio) = &f.source_audio {
                let duration_ms = source_audio.duration_ms;
                audio_files.push(crate::import::discid::ReleaseAudioFile {
                    path,
                    duration_ms,
                    format: source_audio.format.clone(),
                });
            }
        }
    }

    let audio = crate::identify::signals::AudioFacts::of(
        tracks.len() as u32,
        track_lengths_ms,
        audio_files.iter().map(|file| &file.format),
    );
    let rip = bae_util::task_failure::blocking("read a release's rip files", move || {
        crate::import::discid::read_rip_artifacts_from_paths(&documents, &cue_paths, &audio_files)
    })
    .await
    .map_err(|failed| diagnostics.bug(failed))?;

    Ok(ReleaseIdentity { rip, audio })
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
        Ok(Some(file)) => match database.read_file(&file).await {
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
mod tests {
    use super::*;
    use bae_util::test_logs::capture_warn_logs_async;
    use std::sync::Arc;

    #[tokio::test]
    async fn resolve_release_artwork_paths_warns_on_missing_registered_image_file() {
        use crate::db::{Database, DbAlbum, DbArtist, DbRelease, NewReleaseFile};
        use bae_util::content_type::ContentType;
        use chrono::Utc;
        use uuid::Uuid;

        let temp = tempfile::TempDir::new().unwrap();
        let library_root = temp.path().join("library");
        std::fs::create_dir_all(&library_root).unwrap();

        let database = Database::new_test(
            library_root.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let artist = DbArtist {
            id: bae_test_support::test_uuid("artist-1"),
            name: "Artist Name".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        database.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Album Title".to_string(),
            year: None,
            is_compilation: false,
            created_at: Utc::now(),
        };
        database.insert_album(&album).await.unwrap();
        database
            .insert_album_artist(&crate::db::DbAlbumArtist::new_test(&album.id, &artist.id))
            .await
            .unwrap();

        let release = DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: bae_types::pressing::Pressing::blank(),

            source_folder_name: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();

        let file = NewReleaseFile {
            id: "4c3b28d5-7315-48f1-8352-500492675441".to_string(),
            release_id: release.id.clone(),
            original_filename: "cover.jpg".to_string(),
            content_type: ContentType::Jpeg,
            source_audio: None,
            created_at: Utc::now(),
        };
        let missing_dir = temp.path().join("missing-image-dir");
        std::fs::create_dir_all(&missing_dir).unwrap();
        let missing_path = missing_dir.join(&file.original_filename);
        std::fs::write(&missing_path, b"cover").unwrap();
        database.insert_file_for_test(&file, &missing_path).await.unwrap();
        std::fs::remove_file(&missing_path).unwrap();

        let logs = capture_warn_logs_async(|| async {
            let (paths, cover_staging) = resolve_release_artwork_paths(
                &database,
                &crate::diagnostics::Diagnostics::noop(),
                &release.id,
            )
            .await
            .unwrap();
            assert!(paths.is_empty());
            assert!(cover_staging.is_none());
        })
        .await;

        assert!(
            logs.contains(&format!("artwork OCR: registered image file {}", file.id)),
            "expected missing image-file warning, got {logs:?}",
        );
        assert!(
            logs.contains(&missing_dir.join("cover.jpg").display().to_string()),
            "expected missing image path in warning, got {logs:?}",
        );
    }

    /// Re-identify reads LOG/CUE from the release's library files, not from a
    /// folder walk. Covers the local case: LOG + FLAC fixtures in a temp folder
    /// with release rows pointing at it must yield the disc ID a folder import
    /// would have computed.
    #[tokio::test]
    async fn test_resolve_release_identity_local() {
        use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, NewReleaseFile};
        use bae_util::content_type::ContentType;
        use chrono::Utc;
        use std::sync::Arc;
        use uuid::Uuid;

        let temp = tempfile::TempDir::new().unwrap();
        let library_root = temp.path().join("library");
        std::fs::create_dir_all(&library_root).unwrap();
        let local_dir = temp.path().join("rip");
        std::fs::create_dir_all(&local_dir).unwrap();

        std::fs::copy(
            std::path::Path::new("tests/fixtures/logs/test_album.log"),
            local_dir.join("test_album.log"),
        )
        .unwrap();
        let fixture_dir = std::path::Path::new("tests/fixtures/flac");
        std::fs::copy(
            fixture_dir.join("01 Test Track 1.flac"),
            local_dir.join("01 Test Track 1.flac"),
        )
        .unwrap();
        std::fs::copy(
            fixture_dir.join("02 Test Track 2.flac"),
            local_dir.join("02 Test Track 2.flac"),
        )
        .unwrap();

        let database = Database::new_test(
            library_root.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let artist = DbArtist {
            id: bae_test_support::test_uuid("artist-1"),
            name: "Test Artist".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        database.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Test Album".to_string(),
            year: None,
            is_compilation: false,
            created_at: Utc::now(),
        };
        database.insert_album(&album).await.unwrap();
        database
            .insert_album_artist(&crate::db::DbAlbumArtist::new_test(&album.id, &artist.id))
            .await
            .unwrap();

        let release = DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: bae_types::pressing::Pressing::blank(),

            source_folder_name: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();

        for (filename, content_type) in [
            ("test_album.log", ContentType::PlainText),
            ("01 Test Track 1.flac", ContentType::Flac),
            ("02 Test Track 2.flac", ContentType::Flac),
        ] {
            let abs = local_dir.join(filename);
            let file = NewReleaseFile {
                id: Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                original_filename: filename.to_string(),
                content_type,
                source_audio: None,
                created_at: Utc::now(),
            };
            database.insert_file_for_test(&file, &abs).await.unwrap();
        }

        // Two track rows, so the assertion below pins the count and the
        // lengths to the DB rather than to whatever a folder walk would have
        // read.
        database
            .insert_release_medium(&crate::db::DbReleaseMedium::new_test(&release.id))
            .await
            .unwrap();
        for n in 1..=2 {
            let track = DbTrack {
                id: Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                title: format!("Track {n}"),
                medium_id: crate::db::identity::release_medium_id(&release.id, 1),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::new(
                    u32::try_from(n).expect("a test track number fits u32"),
                )
                .expect("track numbers count from one"),
                duration_ms: 180_000 + n as u64,
                created_at: Utc::now(),
            };
            database.insert_track(&track).await.unwrap();
        }

        let identity = resolve_release_identity(
            &database,
            &crate::diagnostics::Diagnostics::noop(),
            &release.id,
        )
        .await
        .unwrap();
        let (rip, track_count) = (identity.rip, identity.audio.track_count);

        assert!(
            rip.disc_id.computed().len() == 1,
            "LOG file in local folder must produce a Disc ID"
        );
        assert_eq!(track_count, 2, "track count comes from the DB tracks");
        assert_eq!(
            identity.audio.track_lengths_ms,
            vec![180_001, 180_002],
            "a document is read against the lengths a pick lays the release out on"
        );
    }
}
