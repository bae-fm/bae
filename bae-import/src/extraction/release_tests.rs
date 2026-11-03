use super::*;
use bae_util::test_logs::capture_warn_logs_async;
use std::sync::Arc;

#[tokio::test]
async fn resolve_release_artwork_paths_warns_on_missing_registered_image_file() {
    use bae_db::{Database, DbAlbum, DbArtist, DbRelease, NewReleaseFile};
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
        id: bae_util::test_ids::test_uuid("artist-1"),
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
        .insert_album_artist(&bae_db::DbAlbumArtist::new_test(&album.id, &artist.id))
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
            &bae_diagnostics::Diagnostics::noop(),
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
    use bae_db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, NewReleaseFile};
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
    std::fs::copy(fixture_dir.join("01 Test Track 1.flac"), local_dir.join("01 Test Track 1.flac"))
        .unwrap();
    std::fs::copy(fixture_dir.join("02 Test Track 2.flac"), local_dir.join("02 Test Track 2.flac"))
        .unwrap();

    let database = Database::new_test(
        library_root.join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let artist = DbArtist {
        id: bae_util::test_ids::test_uuid("artist-1"),
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
        .insert_album_artist(&bae_db::DbAlbumArtist::new_test(&album.id, &artist.id))
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
    database.insert_release_medium(&bae_db::DbReleaseMedium::new_test(&release.id)).await.unwrap();
    for n in 1..=2 {
        let track = DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release.id.clone(),
            title: format!("Track {n}"),
            medium_id: bae_db::identity::release_medium_id(&release.id, 1),
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

    let identity =
        resolve_release_identity(&database, &bae_diagnostics::Diagnostics::noop(), &release.id)
            .await
            .unwrap();
    let (rip, track_count) = (identity.rip, identity.audio.track_count);

    assert!(rip.disc_id.computed().len() == 1, "LOG file in local folder must produce a Disc ID");
    assert_eq!(track_count, 2, "track count comes from the DB tracks");
    assert_eq!(
        identity.audio.track_lengths_ms,
        vec![180_001, 180_002],
        "a document is read against the lengths a pick lays the release out on"
    );
}
