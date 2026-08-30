//! Extraction inputs for the `Release` source (re-identify): resolve a library
//! release's files into a disc ID + track count and the artwork paths for the OCR
//! pass. The disc-ID calculation itself lives in `import::discid`.

use std::path::PathBuf;
use tracing::{debug, warn};

/// Disc ID and track count for a release already in the library.
///
/// Resolves the release's local files, filters LOG / CUE / audio, and computes a
/// disc ID in the order folder imports use: LOG first (most accurate), then
/// CUE+audio pairs. The track count comes from the DB's track rows, not the
/// files — those rows are the user's truth.
///
/// `(None, count)` when no LOG/CUE artifact is available — a cloud-only release
/// with no local copy, or one with track files but no rip metadata. The caller
/// turns that into `DiscIdSignal::Absent`.
pub(crate) async fn resolve_release_identity(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
) -> Result<(Option<String>, u32), String> {
    library_manager
        .get_release_by_id(release_id)
        .await
        .map_err(|e| format!("Failed to load release: {e}"))?
        .ok_or_else(|| format!("Release '{release_id}' not found"))?;
    let files = library_manager
        .get_files_for_release(release_id)
        .await
        .map_err(|e| format!("Failed to load release files: {e}"))?;
    let track_count = library_manager
        .get_tracks_for_release(release_id)
        .await
        .map_err(|e| format!("Failed to load tracks: {e}"))?
        .len() as u32;
    // On-disk paths come from coven's external refs (a Local release's files are
    // the user's own, in place). A remote file has no external ref and is skipped,
    // so a cloud-only release yields no paths and thus no disc ID.
    let mut log_paths = Vec::new();
    let mut cue_paths = Vec::new();
    let mut audio_files = Vec::new();
    for f in &files {
        if let Some(path) = library_manager
            .file_local_path(&f.id)
            .await
            .map_err(|e| format!("Failed to resolve local path: {e}"))?
        {
            if !path.exists() {
                continue;
            }
            match path.extension().and_then(|extension| extension.to_str()) {
                Some(extension) if extension.eq_ignore_ascii_case("log") => {
                    log_paths.push(path.clone());
                }
                Some(extension) if extension.eq_ignore_ascii_case("cue") => {
                    cue_paths.push(path.clone());
                }
                _ => {}
            }
            if let Some(source_audio) = &f.source_audio {
                let duration_ms = u64::try_from(source_audio.duration_ms).map_err(|_| {
                    format!(
                        "Release file '{}' has a negative source-audio duration",
                        f.id
                    )
                })?;
                audio_files.push((path, duration_ms));
            }
        }
    }

    let disc_id = tokio::task::spawn_blocking(move || {
        crate::import::discid::compute_discid_from_paths(&log_paths, &cue_paths, &audio_files)
    })
    .await
    .map_err(|e| format!("DiscID compute task failed: {e}"))?;

    Ok((disc_id, track_count))
}

/// The artwork the re-identify OCR pass reads: the release's cover, plus every
/// release-attached image file that resolves to a path on this device. A
/// cloud-only image has no local copy and is skipped — OCR can't run on it.
///
/// The cover's bytes live in coven's store, which exposes no path bae may
/// compute, so they are read out and staged into a temp dir. That dir's guard
/// comes back alongside the paths and must outlive the OCR pass; `None` when the
/// release has no cover.
pub(crate) async fn resolve_release_artwork_paths(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
) -> Result<(Vec<PathBuf>, Option<tempfile::TempDir>), String> {
    let files = library_manager
        .get_files_for_release(release_id)
        .await
        .map_err(|e| format!("Failed to load release files: {e}"))?;
    let mut paths: Vec<PathBuf> = Vec::new();

    // The cover is one of several OCR inputs (the image files below are the
    // others), so a read/staging failure skips just the cover, logged, rather than
    // failing the whole resolve.
    let cover_staging = match library_manager.cover_ref(release_id).await {
        Ok(Some(image)) => {
            match library_manager.read_image_blob(&image).await {
                Ok(Some(bytes)) => stage_cover_for_ocr(release_id, &bytes, &mut paths),
                Ok(None) => {
                    debug!("artwork OCR: release {release_id} has no cover blob; skipping cover staging");
                    None
                }
                Err(e) => {
                    warn!(
                    "artwork OCR: reading cover for release {release_id} failed: {e}; skipping cover"
                );
                    None
                }
            }
        }
        Ok(None) => {
            debug!("artwork OCR: release {release_id} has no cover blob; skipping cover staging");
            None
        }
        Err(e) => {
            warn!(
                "artwork OCR: reading cover for release {release_id} failed: {e}; skipping cover"
            );
            None
        }
    };

    // In-folder artwork (cover.jpg and the like), resolved to the user's own file
    // through coven's external ref — Local releases only.
    for file in &files {
        if !file.content_type.is_image() {
            continue;
        }
        if let Some(p) = library_manager
            .file_local_path(&file.id)
            .await
            .map_err(|e| format!("Failed to resolve image path: {e}"))?
        {
            if p.exists() {
                paths.push(p);
            } else {
                warn!(
                    "artwork OCR: registered image file {} resolved to missing path {}; skipping image",
                    file.id,
                    p.display()
                );
            }
        }
    }

    Ok((paths, cover_staging))
}

/// Write cover bytes to a temp file the OCR reader can open, pushing its path
/// onto `paths` and returning the temp-dir guard. An IO failure skips the cover,
/// logged, rather than failing the whole resolve.
fn stage_cover_for_ocr(
    release_id: &str,
    bytes: &[u8],
    paths: &mut Vec<PathBuf>,
) -> Option<tempfile::TempDir> {
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
    paths.push(cover_path);
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_logs::capture_warn_logs_async;
    use std::sync::Arc;

    #[tokio::test]
    async fn resolve_release_artwork_paths_warns_on_missing_registered_image_file() {
        use crate::config::{Config, ConfigHandle};
        use crate::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease};
        use crate::library::LibraryManager;
        use crate::util::content_type::ContentType;
        use chrono::Utc;
        use coven::StoreDir;
        use uuid::Uuid;

        let temp = tempfile::TempDir::new().unwrap();
        let library_root = temp.path().join("library");
        std::fs::create_dir_all(&library_root).unwrap();

        let database = Database::new_test(
            library_root.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
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

        let library_dir = StoreDir::new(library_root.clone());
        let library_id = format!("test-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        let config_handle = Arc::new(ConfigHandle::new(config));
        crate::config::install_test_keyring();
        let manager = LibraryManager::new(
            database.clone(),
            config_handle,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(),
        );

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Album Title".to_string(),
            artist_id: bae_test_support::test_uuid("artist-1"),
            year: None,
            primary_release_id: None,
            is_compilation: false,
            created_at: Utc::now(),
        };
        database.insert_album(&album).await.unwrap();

        let release = DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: crate::db::Pressing::blank(),
            disc_id: None,
            metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();

        let file = DbFile {
            id: "4c3b28d5-7315-48f1-8352-500492675441".to_string(),
            release_id: release.id.clone(),
            original_filename: "cover.jpg".to_string(),
            file_size: 5,
            content_type: ContentType::Jpeg,
            source_audio: None,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: Utc::now(),
        };
        database.insert_file(&file).await.unwrap();
        let missing_dir = temp.path().join("missing-image-dir");
        database
            .register_release_external_refs_for_test(&release.id, &missing_dir.to_string_lossy())
            .await
            .unwrap();

        let logs = capture_warn_logs_async(|| async {
            let (paths, cover_staging) = resolve_release_artwork_paths(&manager, &release.id)
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
        use crate::config::{Config, ConfigHandle};
        use crate::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease, DbTrack};
        use crate::library::LibraryManager;
        use crate::util::content_type::ContentType;
        use chrono::Utc;
        use coven::StoreDir;
        use std::sync::Arc;
        use uuid::Uuid;

        let temp = tempfile::TempDir::new().unwrap();
        let library_root = temp.path().join("library");
        std::fs::create_dir_all(&library_root).unwrap();
        let local_dir = temp.path().join("rip");
        std::fs::create_dir_all(&local_dir).unwrap();

        std::fs::copy(
            std::path::Path::new("tests/fixtures/test_album.log"),
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
            std::sync::Arc::new(coven::UuidProvider),
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

        let library_dir = StoreDir::new(library_root.clone());
        // Unique id per test so keyring entries don't collide in the shared
        // process-global mock store (see `install_test_keyring`).
        let library_id = format!("test-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        let config_handle = Arc::new(ConfigHandle::new(config));
        crate::config::install_test_keyring();
        let manager = LibraryManager::new(
            database.clone(),
            config_handle,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(),
        );

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Test Album".to_string(),
            artist_id: bae_test_support::test_uuid("artist-1"),
            year: None,
            primary_release_id: None,
            is_compilation: false,
            created_at: Utc::now(),
        };
        database.insert_album(&album).await.unwrap();

        let release = DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: crate::db::Pressing::blank(),
            disc_id: None,
            metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
            remote: false,
            source_folder_name: None,
            content_hash: None,
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
            let size = std::fs::metadata(&abs).unwrap().len() as i64;
            let file = DbFile {
                id: Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                original_filename: filename.to_string(),
                file_size: size,
                content_type,
                source_audio: None,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"fixture"),
                created_at: Utc::now(),
            };
            database.insert_file(&file).await.unwrap();
        }
        // Register the external refs *after* inserting the files, so the disc-ID
        // re-read can resolve their paths.
        database
            .register_release_external_refs_for_test(&release.id, &local_dir.to_string_lossy())
            .await
            .unwrap();

        // Two track rows, so the assertion below pins the count to the DB rather
        // than to whatever a folder walk would have counted.
        for n in 1..=2 {
            let track = DbTrack {
                id: Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                title: format!("Track {n}"),
                side: 1,
                track_number: Some(n),
                duration_ms: None,
                discogs_position: None,
                created_at: Utc::now(),
            };
            database.insert_track(&track).await.unwrap();
        }

        let (disc_id, track_count) = resolve_release_identity(&manager, &release.id)
            .await
            .unwrap();

        assert!(
            disc_id.is_some(),
            "LOG file in local folder must produce a disc ID"
        );
        assert_eq!(track_count, 2, "track count comes from the DB tracks");
    }
}
