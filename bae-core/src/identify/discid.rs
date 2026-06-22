//! DiscID phase: folder scans and the shared DiscID lookup tail. The actual
//! DiscID calculation lives in `import::discid` and is reused unchanged — this
//! module just orchestrates when to call it.

use crate::db::LibraryStatus;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{lookup_by_discid, DiscIdResult, MetadataResult};
use crate::signals::LookupFailure;
use std::path::PathBuf;

/// Compute disc ID and track count for a release already in the library.
/// Used by the re-identify pipeline.
///
/// Resolves the release's local files (unmanaged folder or pinned managed
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
    // Resolve local paths for every release file. Cloud-only files
    // without a local copy are skipped — the disc-ID phase silently
    // bails to `None` for those, which `resolve_release_identity`'s
    // contract permits and the service treats as `DiscidUnavailable`.
    let mut local_paths: Vec<PathBuf> = Vec::new();
    for f in &files {
        if let Some(p) = library_manager
            .resolve_readable_local_path(f)
            .await
            .map_err(|e| format!("Failed to resolve local path: {e}"))?
        {
            if p.exists() {
                local_paths.push(p);
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

/// Local artwork paths for a release, for the re-identify pipeline's
/// barcode OCR phase. Includes the cover image (if on disk) and every
/// release-attached image file resolvable to a local path. Cloud-only
/// images without a cached copy are skipped — barcode OCR can't run on
/// them and the pipeline silently degrades to "no signals."
pub async fn resolve_release_artwork_paths(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let files = library_manager
        .get_files_for_release(release_id)
        .await
        .map_err(|e| format!("Failed to load release files: {e}"))?;
    let mut paths: Vec<PathBuf> = Vec::new();

    // Cover image, stored at `library_dir/images/{release_id}` for
    // releases whose cover came from a downloaded source. The path may
    // not exist for releases without a cover; skip silently.
    let cover_path = library_manager.image_path(release_id);
    if cover_path.exists() {
        paths.push(cover_path);
    }

    // Release-attached image files (in-folder artwork like cover.jpg).
    for file in &files {
        if !file.content_type.is_image() {
            continue;
        }
        if let Some(p) = library_manager
            .resolve_readable_local_path(file)
            .await
            .map_err(|e| format!("Failed to resolve local path: {e}"))?
        {
            if p.exists() {
                paths.push(p);
            }
        }
    }

    Ok(paths)
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
            track_count,
            Some(2),
            "track_count must equal the number of audio files, not 0"
        );
    }

    /// Re-identify sources LOG/CUE from the release's library files,
    /// not from a folder walk. For an unmanaged release, those files live
    /// at `unmanaged_path`; for a pinned release they live under managed
    /// storage. This test covers the unmanaged path: LOG + FLAC fixtures
    /// in a temp folder, release rows pointing at that folder, then
    /// `resolve_release_identity` returns the same disc ID a folder import
    /// would have computed.
    #[tokio::test]
    async fn test_resolve_release_identity_unmanaged() {
        use crate::config::{Config, ConfigHandle};
        use crate::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease, DbTrack};
        use crate::keys::KeyService;
        use crate::library::LibraryManager;
        use crate::library_dir::LibraryDir;
        use crate::util::content_type::ContentType;
        use chrono::Utc;
        use std::sync::Arc;
        use uuid::Uuid;

        let temp = tempfile::TempDir::new().unwrap();
        let library_root = temp.path().join("library");
        std::fs::create_dir_all(&library_root).unwrap();
        let unmanaged_dir = temp.path().join("rip");
        std::fs::create_dir_all(&unmanaged_dir).unwrap();

        // Stage LOG + FLAC fixtures under the unmanaged folder.
        std::fs::copy(
            std::path::Path::new("tests/fixtures/test_album.log"),
            unmanaged_dir.join("test_album.log"),
        )
        .unwrap();
        let fixture_dir = std::path::Path::new("tests/fixtures/flac");
        std::fs::copy(
            fixture_dir.join("01 Test Track 1.flac"),
            unmanaged_dir.join("01 Test Track 1.flac"),
        )
        .unwrap();
        std::fs::copy(
            fixture_dir.join("02 Test Track 2.flac"),
            unmanaged_dir.join("02 Test Track 2.flac"),
        )
        .unwrap();

        let database = Database::new_test(
            library_root.join("test.db").to_str().unwrap(),
            Arc::new(crate::clock::SystemClock),
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

        let library_dir = LibraryDir::new(library_root.clone());
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
            library_dir,
            config_handle,
            key_service,
            Arc::new(crate::clock::SystemClock),
            Arc::new(crate::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
            None,
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
            managed: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        };
        database.insert_release(&release).await.unwrap();
        database
            .test_set_unmanaged_source(&release.id, &unmanaged_dir.to_string_lossy())
            .await
            .unwrap();

        // One DbFile per real file on disk under the unmanaged folder.
        for (filename, content_type) in [
            ("test_album.log", ContentType::PlainText),
            ("01 Test Track 1.flac", ContentType::Flac),
            ("02 Test Track 2.flac", ContentType::Flac),
        ] {
            let abs = unmanaged_dir.join(filename);
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
            "LOG file in unmanaged folder must produce a disc ID"
        );
        assert_eq!(track_count, 2, "track count comes from the DB tracks");
    }
}
