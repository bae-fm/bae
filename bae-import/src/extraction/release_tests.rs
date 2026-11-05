use super::*;
use bae_store::Store;
use bae_util::test_logs::capture_warn_logs_async;
use std::sync::Arc;

/// A test library under `root` holding one release of one album and artist,
/// with no files or tracks yet.
async fn library_with_release(root: &std::path::Path) -> (Store, String) {
    use bae_db::{DbAlbum, DbArtist, DbRelease};
    use chrono::Utc;
    use uuid::Uuid;

    let library_root = root.join("library");
    std::fs::create_dir_all(&library_root).unwrap();
    let db = Store::new_test(
        crate::TEST_SCHEMA,
        library_root.join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let database = bae_db::test_fixtures::test_database(&db);
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
    (db, release.id)
}

#[tokio::test]
async fn resolve_release_artwork_paths_warns_on_missing_registered_image_file() {
    use bae_db::NewReleaseFile;
    use bae_util::content_type::ContentType;
    use chrono::Utc;

    let temp = tempfile::TempDir::new().unwrap();
    let (db, release_id) = library_with_release(temp.path()).await;
    let database = bae_db::test_fixtures::test_database(&db);

    let file = NewReleaseFile {
        id: "4c3b28d5-7315-48f1-8352-500492675441".to_string(),
        release_id: release_id.clone(),
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
            &db,
            &bae_db::test_fixtures::test_database(&db),
            &bae_diagnostics::Diagnostics::noop(),
            &release_id,
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
    use bae_db::{DbTrack, NewReleaseFile};
    use bae_util::content_type::ContentType;
    use chrono::Utc;
    use uuid::Uuid;

    let temp = tempfile::TempDir::new().unwrap();
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

    let (db, release_id) = library_with_release(temp.path()).await;
    let database = bae_db::test_fixtures::test_database(&db);

    for (filename, content_type) in [
        ("test_album.log", ContentType::PlainText),
        ("01 Test Track 1.flac", ContentType::Flac),
        ("02 Test Track 2.flac", ContentType::Flac),
    ] {
        let abs = local_dir.join(filename);
        let file = NewReleaseFile {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.clone(),
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
    database.insert_release_medium(&bae_db::DbReleaseMedium::new_test(&release_id)).await.unwrap();
    for n in 1..=2 {
        let track = DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.clone(),
            title: format!("Track {n}"),
            medium_id: bae_db::identity::release_medium_id(&release_id, 1),
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
        resolve_release_identity(&database, &bae_diagnostics::Diagnostics::noop(), &release_id)
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

/// The disc IDs `folder`'s scan reads off its rip files, and the ones
/// re-identify reads off a library release imported from it: one row per
/// file the scan found, each with its scanned audio facts and its original at
/// the file's path.
async fn folder_and_library_disc_ids(folder: &std::path::Path) -> (Vec<String>, Vec<String>) {
    use bae_db::NewReleaseFile;
    use bae_util::content_type::ContentType;
    use chrono::Utc;
    use uuid::Uuid;

    let categorized = crate::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let disc_ids = |reading: crate::rip_reading::RipReading| {
        reading.disc_id.computed().into_iter().map(|disc| disc.disc_id).collect::<Vec<_>>()
    };
    let scanned = disc_ids(crate::rip_reading::read_rip_artifacts(&categorized));

    let library = tempfile::TempDir::new().unwrap();
    let (db, release_id) = library_with_release(library.path()).await;
    let database = bae_db::test_fixtures::test_database(&db);
    for file in categorized.release_files() {
        let row = NewReleaseFile {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.clone(),
            original_filename: file.relative_path.clone(),
            content_type: file
                .source_audio
                .as_ref()
                .map_or(ContentType::PlainText, |audio| audio.content_type.clone()),
            source_audio: file.source_audio.as_ref().map(|audio| {
                bae_db::source_audio::SourceAudioFile {
                    layout: Some(bae_db::source_audio::SourceAudioLayout::Cue),
                    format: audio.format.clone(),
                    content_type: audio.content_type.clone(),
                    duration_ms: audio.duration_ms,
                }
            }),
            created_at: Utc::now(),
        };
        database.insert_file_for_test(&row, &file.version.path).await.unwrap();
    }
    let identity =
        resolve_release_identity(&database, &bae_diagnostics::Diagnostics::noop(), &release_id)
            .await
            .unwrap();
    (scanned, disc_ids(identity.rip))
}

/// A sheet whose `FILE` names the WAV the rip was written as, beside the one
/// FLAC it was renamed to, is bound to that FLAC by the scan's last resort,
/// and re-identify hashes the same disc from the library's copy of it.
#[tokio::test]
async fn a_library_sheet_naming_renamed_audio_hashes_the_disc_its_scan_did() {
    let folder = tempfile::TempDir::new().unwrap();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    std::fs::copy(fixture.join("Test Album.flac"), folder.path().join("rip.flac")).unwrap();
    let sheet = std::fs::read_to_string(fixture.join("Test Album.cue"))
        .unwrap()
        .replace("Test Album.flac", "CDImage.wav");
    std::fs::write(folder.path().join("Album.cue"), sheet).unwrap();

    let (scanned, library) = folder_and_library_disc_ids(folder.path()).await;

    assert_eq!(scanned.len(), 1, "the scan binds the sheet to the one FLAC beside it");
    assert_eq!(library, scanned);
}

/// Two sheets laying out one image: the first in the folder's order carves it
/// and the other is ignored, so the folder is one disc, in the library too.
#[tokio::test]
async fn a_library_image_two_sheets_describe_hashes_one_disc_as_its_scan_did() {
    let folder = tempfile::TempDir::new().unwrap();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    std::fs::copy(fixture.join("Test Album.flac"), folder.path().join("Test Album.flac")).unwrap();
    let sheet = std::fs::read_to_string(fixture.join("Test Album.cue")).unwrap();
    std::fs::write(folder.path().join("Test Album.cue"), &sheet).unwrap();
    std::fs::write(
        folder.path().join("Test Album.flac.cue"),
        sheet.replace("INDEX 01 00:20:00", "INDEX 01 00:15:00"),
    )
    .unwrap();

    let (scanned, library) = folder_and_library_disc_ids(folder.path()).await;

    assert_eq!(scanned.len(), 1, "one sheet carves the image");
    assert_eq!(library, scanned);
}

/// A sheet laying out an MP3 is refused its binding, since bae carves no
/// tracks out of lossy audio: its folder hashes no disc from it, and neither
/// does the library release imported from it.
#[tokio::test]
async fn a_library_sheet_over_an_mp3_hashes_no_disc_as_its_scan_did() {
    bae_audio::init();
    let folder = tempfile::TempDir::new().unwrap();
    // 9s silent stereo MP3, long enough to span the sheet's three 3-second
    // tracks.
    let samples = vec![0i32; 44_100 * 9 * 2];
    let mp3 = bae_audio::encode_i32(
        bae_audio::EncodeFormat::Mp3 { bitrate_kbps: 320 },
        &samples,
        44_100,
        2,
    )
    .expect("encode mp3");
    std::fs::write(folder.path().join("Test Album.mp3"), mp3).unwrap();
    std::fs::write(
        folder.path().join("Test Album.cue"),
        "PERFORMER \"Artist Name\"\n\
         TITLE \"Album Title\"\n\
         FILE \"Test Album.mp3\" WAVE\n  \
         TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 00:03:00\n  \
         TRACK 03 AUDIO\n    INDEX 01 00:06:00\n",
    )
    .unwrap();

    let (scanned, library) = folder_and_library_disc_ids(folder.path()).await;

    assert!(scanned.is_empty(), "the scan binds no sheet over lossy audio");
    assert_eq!(library, scanned);
}
