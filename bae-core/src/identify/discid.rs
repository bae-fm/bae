//! DiscID phase: folder scans and the shared DiscID lookup tail. The actual
//! DiscID calculation lives in `import::discid` and is reused unchanged — this
//! module just orchestrates when to call it.

use crate::db::LibraryStatus;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{lookup_by_discid, DiscIdResult, MetadataResult};
use crate::signals::LookupFailure;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Compute disc ID and track count for a release already in the library.
/// Used by the re-identify pipeline.
///
/// Resolves the release's local files (local folder or pinned remote
/// storage), filters LOG / CUE / audio files, and computes a disc ID in the
/// same order folder imports use: LOG first (most accurate), then CUE+audio
/// pairs as fallback. Track count is read from the DB — the release's track
/// row count, which already reflects the user's truth.
///
/// Returns `(None, count)` when no LOG/CUE artifacts are available — for
/// cloud-only releases without a local copy, or for releases with track
/// files only and no rip metadata. Caller falls back to barcode OCR or
/// gives up depending on `has_artwork`.
pub async fn resolve_release_identity(
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
    // Resolve on-disk paths for every release file via coven's external refs
    // (a Local release's files are the user's own files in place). Remote files
    // have no external ref and are skipped — the disc-ID phase silently bails to
    // `None` for those, which the service treats as `DiscidUnavailable`.
    let mut local_paths: Vec<PathBuf> = Vec::new();
    for f in &files {
        if let Some(path) = library_manager
            .file_local_path(&f.id)
            .await
            .map_err(|e| format!("Failed to resolve local path: {e}"))?
        {
            if path.exists() {
                local_paths.push(path);
            }
        }
    }

    let log_paths: Vec<PathBuf> = local_paths
        .iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("log"))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    let cue_paths: Vec<PathBuf> = local_paths
        .iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("cue"))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    let audio_paths: Vec<PathBuf> = local_paths
        .iter()
        .filter(|p| crate::util::content_type_hint::ContentTypeHint::path_is_audio(p))
        .cloned()
        .collect();

    let disc_id = tokio::task::spawn_blocking(move || {
        crate::import::discid::compute_discid_from_paths(&log_paths, &cue_paths, &audio_paths)
    })
    .await
    .map_err(|e| format!("DiscID compute task failed: {e}"))?;

    Ok((disc_id, track_count))
}

/// Local artwork paths for a release, for the re-identify pipeline's barcode OCR
/// phase. Includes the release's cover (the bae-produced cover blob, staged to a
/// real file the OCR reader can open) and every release-attached image file
/// resolvable to the user's own path. Cloud-only image files without a local copy
/// are skipped — barcode OCR can't run on them and the pipeline degrades to "no
/// signals."
///
/// The cover blob lives in coven's store (no path bae may compute), so its bytes
/// come back through the cover's image ref and are written into a temp dir whose
/// guard is returned alongside the paths: the caller holds it until the OCR pass
/// finishes, then the staged file is cleaned up. `None` when the release has no
/// cover.
pub async fn resolve_release_artwork_paths(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
) -> Result<(Vec<PathBuf>, Option<tempfile::TempDir>), String> {
    let files = library_manager
        .get_files_for_release(release_id)
        .await
        .map_err(|e| format!("Failed to load release files: {e}"))?;
    let mut paths: Vec<PathBuf> = Vec::new();

    // The release's cover, read through coven and staged to a real file the OCR
    // reader opens. The cover is one of several OCR artwork inputs (the in-folder
    // image files below are the others), so a read/staging failure logs and skips
    // just the cover rather than failing the whole resolve.
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

    // Release-attached image files (in-folder artwork like cover.jpg), resolved
    // to the user's own file via coven's external ref (Local releases only).
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

/// Stage cover bytes to a temp file the OCR reader can open, pushing its path
/// onto `paths` and returning the temp-dir guard. A staging IO failure logs and
/// skips the cover (an optional OCR input) rather than failing the whole resolve.
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

/// Look up a disc ID on MusicBrainz and annotate matches with library
/// status. Returns `(matches, statuses)` — possibly empty when MB has no
/// hits for this disc ID. The triangulation reducer treats empty results
/// the same way as a barcode signal that produced no matches: settled
/// with zero, ready for combine.
pub async fn lookup_and_resolve(
    cover_art_archive: &CoverArtArchiveClient,
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
) -> Result<(Vec<MetadataResult>, Vec<LibraryStatus>), LookupFailure> {
    use crate::db::LibraryCheck;

    // The MB lookup carries its own typed failure (Network / Provider /
    // Timeout) — pass it through structured.
    let result = lookup_by_discid(cover_art_archive, disc_id).await?;

    let matches: Vec<MetadataResult> = match result {
        DiscIdResult::NoMatches => return Ok((Vec::new(), Vec::new())),
        DiscIdResult::SingleMatch(m) => vec![*m],
        DiscIdResult::MultipleMatches(matches) => matches,
    };

    // The in-library check is a local DB read — its failure is opaque
    // diagnostic detail, not a provider verdict.
    let checks: Vec<LibraryCheck> = matches.iter().map(LibraryCheck::from).collect();
    let library_statuses = library_manager
        .check_releases_in_library(&checks)
        .await
        .map_err(|e| LookupFailure::Diagnostic {
            detail: format!("Failed to check library status: {e}"),
        })?;
    Ok((matches, library_statuses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_logs::capture_warn_logs_async;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Regression: one categorize yields both the disc ID (from the LOG here)
    /// and the real track count — the pair the service's folder identify reads.
    #[test]
    fn test_categorized_yields_discid_and_track_count() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let fixture_log = std::path::Path::new("tests/fixtures/test_album.log");
        std::fs::copy(fixture_log, dir.join("test_album.log")).unwrap();

        let fixture_dir = std::path::Path::new("tests/fixtures/flac");
        std::fs::copy(
            fixture_dir.join("01 Test Track 1.flac"),
            dir.join("01 Test Track 1.flac"),
        )
        .unwrap();
        std::fs::copy(
            fixture_dir.join("02 Test Track 2.flac"),
            dir.join("02 Test Track 2.flac"),
        )
        .unwrap();

        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files(dir).unwrap();
        let disc_id = crate::import::discid::compute_discid_from_categorized(&categorized);
        let track_count = categorized.audio.track_count();

        assert!(disc_id.is_some(), "LOG fixture should produce a disc ID");
        assert_eq!(
            track_count, 2,
            "track_count must equal the number of audio files, not 0"
        );
    }

    #[tokio::test]
    async fn resolve_release_artwork_paths_warns_on_missing_registered_image_file() {
        use crate::config::{Config, ConfigHandle};
        use crate::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease};
        use crate::keys::KeyService;
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
        )
        .await
        .unwrap();
        let artist = DbArtist {
            id: "artist-1".to_string(),
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
        let key_service = KeyService::new(library_id);
        let manager = LibraryManager::new(
            database.clone(),
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            tokio::runtime::Handle::current(),
        );

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Album Title".to_string(),
            artist_id: "artist-1".to_string(),
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
            metadata_source: crate::db::ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();

        let file = DbFile {
            id: "image-file-1".to_string(),
            release_id: release.id.clone(),
            original_filename: "cover.jpg".to_string(),
            file_size: 5,
            content_type: ContentType::Jpeg,
            cloud_path: None,
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
            logs.contains("artwork OCR: registered image file image-file-1"),
            "expected missing image-file warning, got {logs:?}",
        );
        assert!(
            logs.contains(&missing_dir.join("cover.jpg").display().to_string()),
            "expected missing image path in warning, got {logs:?}",
        );
    }

    /// Re-identify sources LOG/CUE from the release's library files,
    /// not from a folder walk. For a local release, those files live
    /// at `local_path`; for a pinned release they live under remote
    /// storage. This test covers the local path: LOG + FLAC fixtures
    /// in a temp folder, release rows pointing at that folder, then
    /// `resolve_release_identity` returns the same disc ID a folder import
    /// would have computed.
    #[tokio::test]
    async fn test_resolve_release_identity_local() {
        use crate::config::{Config, ConfigHandle};
        use crate::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease, DbTrack};
        use crate::keys::KeyService;
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

        // Stage LOG + FLAC fixtures under the local folder.
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
        )
        .await
        .unwrap();
        let artist = DbArtist {
            id: "artist-1".to_string(),
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
        let key_service = KeyService::new(library_id);
        let manager = LibraryManager::new(
            database.clone(),
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            tokio::runtime::Handle::current(),
        );

        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Test Album".to_string(),
            artist_id: "artist-1".to_string(),
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
            metadata_source: crate::db::ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();

        // One DbFile per real file on disk under the local folder.
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
                cloud_path: None,
                created_at: Utc::now(),
            };
            database.insert_file(&file).await.unwrap();
        }
        // Register the files as coven external refs (a Local release's in-place
        // files) AFTER inserting them, so the DiscID re-read resolves their paths.
        database
            .register_release_external_refs_for_test(&release.id, &local_dir.to_string_lossy())
            .await
            .unwrap();

        // Two tracks so the count comes from the DB, not from a folder
        // walk that may find different audio counts.
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
