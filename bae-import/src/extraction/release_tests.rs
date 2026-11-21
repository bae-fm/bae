use super::*;
use bae_store::Store;
use bae_util::test_files::TestFolder;
use std::sync::Arc;

/// A test library in `root` holding one release of one album and artist,
/// with no files or tracks yet.
async fn library_with_release(root: &TestFolder) -> (Store, String) {
    let library_root = root.create_folder("library");
    let db = Store::new_test(crate::TEST_SCHEMA, &library_root, Arc::new(coven::SystemClock))
        .await
        .unwrap();
    let release_id = insert_release(&db).await;
    (db, release_id)
}

/// Insert one release of one album and artist into `db`, with no files or
/// tracks yet, and return its id.
async fn insert_release(db: &Store) -> String {
    use bae_records::{DbAlbum, DbArtist, DbRelease};
    use time::UtcDateTime;

    let records = bae_records::test_fixtures::test_records(db);
    let artist = DbArtist {
        id: bae_util::test_ids::test_uuid("artist-1"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: UtcDateTime::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    let album = DbAlbum {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        title: "Test Album".to_string(),
        year: None,
        is_compilation: false,
        created_at: UtcDateTime::now(),
    };
    records.insert_album(&album).await.unwrap();
    records
        .insert_album_artist(&bae_records::DbAlbumArtist::new_test(&album.id, &artist.id))
        .await
        .unwrap();
    let release = DbRelease {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_types::pressing::Pressing::blank(),
        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: UtcDateTime::now(),
    };
    records.insert_release(&release).await.unwrap();
    release.id
}

/// A release file row named `name`, of `role` and `content_type`.
fn release_file(
    release_id: &str,
    name: &str,
    content_type: bae_util::content_type::ContentType,
    role: bae_records::ReleaseFileRole,
) -> bae_records::NewReleaseFile {
    bae_records::NewReleaseFile {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        release_id: release_id.to_string(),
        original_filename: name.to_string(),
        content_type,
        role,
        source_audio: None,
        created_at: time::UtcDateTime::now(),
    }
}

/// The release's rip log, a copy of the fixture log, as a file row.
fn rip_log(release_id: &str) -> bae_records::NewReleaseFile {
    release_file(
        release_id,
        "test_album.log",
        bae_util::content_type::ContentType::PlainText,
        bae_records::ReleaseFileRole::Document(bae_records::DocumentKind::RipLog),
    )
}

/// Write a copy of the fixture rip log into `dir` in `folder`, returning its
/// path.
fn write_rip_log(folder: &TestFolder, dir: &std::path::Path) -> std::path::PathBuf {
    folder.copy_fixture(
        std::path::Path::new("tests/fixtures/logs/test_album.log"),
        dir.join("test_album.log"),
    )
}

/// Give the release two track rows, so a reading's track count and lengths
/// come from them.
async fn insert_two_tracks(records: &Records, release_id: &str) {
    use bae_records::DbTrack;

    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(release_id))
        .await
        .unwrap();
    for n in 1..=2u32 {
        let id = coven::IdSource::new_id(&coven::UuidIds).to_string();
        let track = DbTrack {
            duration_ms: 180_000 + u64::from(n),
            ..DbTrack::new_test(release_id, &id, &format!("Track {n}"), n)
        };
        records.insert_track(&track).await.unwrap();
    }
}

/// Re-identify's reading of the release `release_id` in `db`.
async fn identity_of(
    db: &Store,
    release_id: &str,
) -> Result<ReleaseIdentity, ReleaseIdentityError> {
    resolve_release_identity(
        &crate::tables::ImportTables::new(db.clone()),
        &bae_records::test_fixtures::test_records(db),
        db.test_sources().files(),
        &bae_diagnostics::Diagnostics::noop(),
        release_id,
    )
    .await
}

/// The coven failure a reading of a rip file failed with.
fn file_read_failure(
    result: Result<ReleaseIdentity, ReleaseIdentityError>,
) -> coven::FileReadError {
    match result {
        Err(ReleaseIdentityError::Document(crate::LibraryDocumentError::Coven {
            error: bae_store::DbError::FileRead(error),
            ..
        })) => *error,
        Err(error) => panic!("the reading failed otherwise: {error}"),
        Ok(_) => panic!("the reading succeeded"),
    }
}

/// An image of the release whose original is gone fails its read with coven's
/// reason, rather than being skipped.
#[tokio::test]
async fn a_missing_image_original_fails_the_artwork_gathering() {
    use bae_util::content_type::ContentType;

    let temp = TestFolder::new();
    let (db, release_id) = library_with_release(&temp).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let file = release_file(
        &release_id,
        "cover.jpg",
        ContentType::Jpeg,
        bae_records::ReleaseFileRole::Artwork,
    );
    let original = temp.write("cover.jpg", b"cover");
    records.insert_file_for_test(&file, &original).await.unwrap();
    temp.remove(&original);

    let images = resolve_release_artwork(&records, &release_id).await.unwrap();
    let (_, image) = &images[0];
    let result = read_library_image(&db, &records, db.test_sources().files(), image).await;

    match result {
        Err(ReleaseArtworkError::ImageRead {
            name,
            error: bae_store::DbError::FileRead(error),
        }) => {
            assert_eq!(name, "cover.jpg");
            assert!(
                matches!(*error, coven::FileReadError::UserFileMissing { .. }),
                "coven reports the original missing: {error}"
            );
        }
        Err(error) => panic!("the read failed otherwise: {error}"),
        Ok(_) => panic!("the read succeeded without the image"),
    }
}

/// The OCR pass lists each image of the release by its name in the release and
/// reads the bytes coven holds for it.
#[tokio::test]
async fn the_artwork_listing_names_each_image_and_coven_reads_its_bytes() {
    use bae_util::content_type::ContentType;

    let temp = TestFolder::new();
    let (db, release_id) = library_with_release(&temp).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let original = temp.write("back.jpg", b"the back cover");
    records
        .insert_file_for_test(
            &release_file(
                &release_id,
                "Scans/back.jpg",
                ContentType::Jpeg,
                bae_records::ReleaseFileRole::Artwork,
            ),
            &original,
        )
        .await
        .unwrap();

    let images = resolve_release_artwork(&records, &release_id).await.unwrap();

    assert_eq!(images.len(), 1, "one image and no cover: {images:?}");
    assert_eq!(images[0].0, std::path::Path::new("Scans/back.jpg"));
    let bytes =
        read_library_image(&db, &records, db.test_sources().files(), &images[0].1).await.unwrap();
    assert_eq!(bytes, b"the back cover");
}

/// Re-identify reads LOG/CUE from the release's library files, not from a
/// folder walk. Covers the local case: LOG + FLAC fixtures in a temp folder
/// with release rows pointing at it must yield the disc ID a folder import
/// would have computed.
#[tokio::test]
async fn test_resolve_release_identity_local() {
    use bae_util::content_type::ContentType;

    let temp = TestFolder::new();
    let local_dir = temp.path().join("rip");
    temp.create_folder(&local_dir);
    write_rip_log(&temp, &local_dir);
    let fixture_dir = std::path::Path::new("tests/fixtures/flac");
    temp.copy_fixture(
        fixture_dir.join("01 Test Track 1.flac"),
        local_dir.join("01 Test Track 1.flac"),
    );
    temp.copy_fixture(
        fixture_dir.join("02 Test Track 2.flac"),
        local_dir.join("02 Test Track 2.flac"),
    );

    let (db, release_id) = library_with_release(&temp).await;
    let records = bae_records::test_fixtures::test_records(&db);

    records
        .insert_file_for_test(&rip_log(&release_id), &local_dir.join("test_album.log"))
        .await
        .unwrap();
    for filename in ["01 Test Track 1.flac", "02 Test Track 2.flac"] {
        let file = release_file(
            &release_id,
            filename,
            ContentType::Flac,
            bae_records::ReleaseFileRole::Other,
        );
        records.insert_file_for_test(&file, &local_dir.join(filename)).await.unwrap();
    }
    // Two track rows, so the assertion below pins the count and the
    // lengths to the DB rather than to whatever a folder walk would have
    // read.
    insert_two_tracks(&records, &release_id).await;

    let identity = identity_of(&db, &release_id).await.unwrap();
    let (rip, track_count) = (identity.rip, identity.audio.track_count);

    assert!(rip.disc_id.computed().len() == 1, "LOG file in local folder must produce a Disc ID");
    assert_eq!(track_count, 2, "track count comes from the DB tracks");
    assert_eq!(
        identity.audio.track_lengths_ms,
        vec![180_001, 180_002],
        "a document is read against the lengths a pick lays the release out on"
    );
}

/// A rip log whose original changed since its import fails the reading with
/// coven's reason, rather than reading the changed bytes.
#[tokio::test]
async fn a_changed_rip_log_original_fails_the_reading() {
    let temp = TestFolder::new();
    let (db, release_id) = library_with_release(&temp).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let log = write_rip_log(&temp, temp.path());
    records.insert_file_for_test(&rip_log(&release_id), &log).await.unwrap();
    insert_two_tracks(&records, &release_id).await;
    temp.write(&log, "a log someone rewrote");

    let failure = file_read_failure(identity_of(&db, &release_id).await);

    assert!(
        matches!(failure, coven::FileReadError::UserFileChanged { .. }),
        "coven reports the original changed: {failure}"
    );
}

/// A rip log whose original is gone fails the reading with coven's reason,
/// rather than being skipped.
#[tokio::test]
async fn a_missing_rip_log_original_fails_the_reading() {
    let temp = TestFolder::new();
    let (db, release_id) = library_with_release(&temp).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let log = write_rip_log(&temp, temp.path());
    records.insert_file_for_test(&rip_log(&release_id), &log).await.unwrap();
    insert_two_tracks(&records, &release_id).await;
    temp.remove(&log);

    let failure = file_read_failure(identity_of(&db, &release_id).await);

    assert!(
        matches!(failure, coven::FileReadError::UserFileMissing { .. }),
        "coven reports the original missing: {failure}"
    );
}

/// A release another device imported and stored in the cloud has no
/// originals here, and re-identify reads its rip log from storage: the log's
/// Disc ID comes out on this device too.
#[test]
fn a_cloud_release_from_another_device_reads_its_rip_log() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let rip = TestFolder::new();
        let log = write_rip_log(&rip, rip.path());
        let release_id = insert_release(devices.a()).await;
        let records = bae_records::test_fixtures::test_records(devices.a());
        records
            .insert_files_placed_for_test(vec![(rip_log(&release_id), log, coven::Place::Cloud)])
            .await
            .unwrap();
        insert_two_tracks(&records, &release_id).await;
        devices
            .converge(
                "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        let files = bae_records::test_fixtures::test_records(devices.b())
            .get_files_for_release(&release_id)
            .await
            .unwrap();
        assert!(
            files.iter().all(|file| file.stored),
            "the rip log reached the other device stored: {files:?}"
        );

        let identity = identity_of(devices.b(), &release_id).await.unwrap();

        assert_eq!(identity.rip.disc_id.computed().len(), 1, "the rip log hashes its disc");
        assert_eq!(identity.audio.track_count, 2);
    });
}

/// The disc IDs `folder`'s scan reads off its rip files, and the ones
/// re-identify reads off a library release imported from it: one row per
/// file the scan found, each with its scanned audio facts and its original at
/// the file's path.
async fn folder_and_library_disc_ids(folder: &std::path::Path) -> (Vec<String>, Vec<String>) {
    folder_and_library_disc_ids_with_decisions(folder, Default::default()).await
}

/// [`folder_and_library_disc_ids`] with the person's `decided` sheet decisions
/// stored for the release, naming files by their names in the folder. The
/// folder's own scan reads none.
async fn folder_and_library_disc_ids_with_decisions(
    folder: &std::path::Path,
    decided: crate::folder_scanner::CandidateFileEdits,
) -> (Vec<String>, Vec<String>) {
    use bae_records::NewReleaseFile;
    use bae_util::content_type::ContentType;
    use time::UtcDateTime;

    let categorized = crate::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let disc_ids = |reading: crate::rip_reading::RipReading| {
        reading.disc_id.computed().into_iter().map(|disc| disc.disc_id).collect::<Vec<_>>()
    };
    let scanned = disc_ids(
        crate::rip_reading::read_rip_artifacts(&categorized, &bae_util::test_files::lease())
            .unwrap(),
    );

    let library = TestFolder::new();
    let (db, release_id) = library_with_release(&library).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let mut file_ids = std::collections::HashMap::new();
    for entry in &categorized.files {
        let file = &entry.file;
        let id = coven::IdSource::new_id(&coven::UuidIds).to_string();
        file_ids.insert(file.relative_path.clone(), id.clone());
        let row = NewReleaseFile {
            id,
            release_id: release_id.clone(),
            original_filename: file.relative_path.clone(),
            content_type: file
                .source_audio
                .as_ref()
                .map_or(ContentType::PlainText, |audio| audio.content_type.clone()),
            role: match entry.role {
                crate::folder_scanner::FileRole::Audio => bae_records::ReleaseFileRole::Audio,
                crate::folder_scanner::FileRole::TrackSheet { .. } => {
                    bae_records::ReleaseFileRole::TrackSheet
                }
                crate::folder_scanner::FileRole::Document { kind } => {
                    bae_records::ReleaseFileRole::Document(kind)
                }
                crate::folder_scanner::FileRole::Artwork => bae_records::ReleaseFileRole::Artwork,
                crate::folder_scanner::FileRole::DamagedImage => {
                    bae_records::ReleaseFileRole::DamagedImage
                }
                crate::folder_scanner::FileRole::Other => bae_records::ReleaseFileRole::Other,
            },
            source_audio: file.source_audio.as_ref().map(|audio| {
                bae_records::source_audio::SourceAudioFile {
                    layout: Some(bae_records::source_audio::SourceAudioLayout::Cue),
                    format: audio.format.clone(),
                    content_type: audio.content_type.clone(),
                    container: audio.container.clone(),
                    duration_ms: audio.duration_ms,
                }
            }),
            created_at: UtcDateTime::now(),
        };
        records.insert_file_for_test(&row, &file.version.path).await.unwrap();
    }
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.store_release_sheet_decisions_for_test(decided, file_ids).await.unwrap();
    let identity = resolve_release_identity(
        &tables,
        &records,
        db.test_sources().files(),
        &bae_diagnostics::Diagnostics::noop(),
        &release_id,
    )
    .await
    .unwrap();
    (scanned, disc_ids(identity.rip))
}

/// A sheet whose `FILE` names the WAV the rip was written as, beside the one
/// FLAC it was renamed to, is bound to that FLAC by the scan's last resort,
/// and re-identify hashes the same disc from the library's copy of it.
#[tokio::test]
async fn a_library_sheet_naming_renamed_audio_hashes_the_disc_its_scan_did() {
    let folder = TestFolder::new();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("rip.flac"));
    let sheet = crate::test_fixtures::read_fixture_text(fixture.join("Test Album.cue"))
        .replace("Test Album.flac", "CDImage.wav");
    folder.write("Album.cue", sheet);

    let (scanned, library) = folder_and_library_disc_ids(folder.path()).await;

    assert_eq!(scanned.len(), 1, "the scan binds the sheet to the one FLAC beside it");
    assert_eq!(library, scanned);
}

/// Two sheets laying out one image: the first in the folder's order carves it
/// and the other is ignored, so the folder is one disc, in the library too.
#[tokio::test]
async fn a_library_image_two_sheets_describe_hashes_one_disc_as_its_scan_did() {
    let folder = TestFolder::new();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("Test Album.flac"));
    let sheet = crate::test_fixtures::read_fixture_text(fixture.join("Test Album.cue"));
    folder.write("Test Album.cue", &sheet);
    folder.write(
        folder.path().join("Test Album.flac.cue"),
        sheet.replace("INDEX 01 00:20:00", "INDEX 01 00:15:00"),
    );

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
    let folder = TestFolder::new();
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
    folder.write("Test Album.mp3", mp3);
    folder.write(
        folder.path().join("Test Album.cue"),
        "PERFORMER \"Artist Name\"\n\
         TITLE \"Album Title\"\n\
         FILE \"Test Album.mp3\" WAVE\n  \
         TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 00:03:00\n  \
         TRACK 03 AUDIO\n    INDEX 01 00:06:00\n",
    );

    let (scanned, library) = folder_and_library_disc_ids(folder.path()).await;

    assert!(scanned.is_empty(), "the scan binds no sheet over lossy audio");
    assert_eq!(library, scanned);
}

/// A sheet the person unbound from its audio in the import pane stays unbound
/// when re-identify reads the release again: a second copy of the image sits
/// beside it, the scan binds the sheet to the image it names and hashes one
/// disc, and the stored cleared binding leaves the library's sheet with none.
#[tokio::test]
async fn a_library_sheet_the_person_unbound_hashes_no_disc() {
    let folder = TestFolder::new();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("Test Album.flac"));
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("Copy.flac"));
    folder.copy_fixture(fixture.join("Test Album.cue"), folder.path().join("Test Album.cue"));

    let (scanned, bound_by_scan) = folder_and_library_disc_ids(folder.path()).await;
    assert_eq!(scanned.len(), 1, "the scan binds the sheet to the image it names");
    assert_eq!(bound_by_scan, scanned);

    let mut decided = crate::folder_scanner::CandidateFileEdits::default();
    decided.sheet_bindings.set_reference(
        "Test Album.cue".to_string(),
        "Test Album.flac".to_string(),
        crate::folder_scanner::UserSheetBinding::Cleared,
    );
    let (_, unbound) = folder_and_library_disc_ids_with_decisions(folder.path(), decided).await;
    assert!(unbound.is_empty(), "re-identify reads the person's cleared binding: {unbound:?}");
}

/// When re-identify reads the release again, it reads the file the person
/// bound a sheet to rather than the one the sheet names: the sheet names a
/// WAV beside two FLAC copies, which the scan cannot choose between, and the
/// stored binding picks one.
#[tokio::test]
async fn a_library_sheet_the_person_bound_hashes_the_disc_of_that_audio() {
    let folder = TestFolder::new();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("a.flac"));
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("b.flac"));
    let sheet = crate::test_fixtures::read_fixture_text(fixture.join("Test Album.cue"))
        .replace("Test Album.flac", "CDImage.wav");
    folder.write("Album.cue", sheet);

    let (scanned, unbound_by_scan) = folder_and_library_disc_ids(folder.path()).await;
    assert!(scanned.is_empty(), "two images and a sheet naming neither: the scan binds none");
    assert!(unbound_by_scan.is_empty());

    let mut decided = crate::folder_scanner::CandidateFileEdits::default();
    decided.sheet_bindings.set_reference(
        "Album.cue".to_string(),
        "CDImage.wav".to_string(),
        crate::folder_scanner::UserSheetBinding::Describes { file_id: "b.flac".to_string() },
    );
    let (_, bound) = folder_and_library_disc_ids_with_decisions(folder.path(), decided).await;
    assert_eq!(bound.len(), 1, "re-identify reads the person's binding: {bound:?}");
}

/// A sheet the person ignored in the import pane stays ignored when
/// re-identify reads the release again: two sheets lay out two images, the
/// scan hashes two discs, and the stored decision leaves the library one.
#[tokio::test]
async fn a_library_sheet_the_person_ignored_hashes_no_disc() {
    let folder = TestFolder::new();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("Test Album.flac"));
    folder.copy_fixture(fixture.join("Test Album.flac"), folder.path().join("Second.flac"));
    let sheet = crate::test_fixtures::read_fixture_text(fixture.join("Test Album.cue"));
    folder.write("Test Album.cue", &sheet);
    folder.write(
        folder.path().join("Second.cue"),
        sheet
            .replace("Test Album.flac", "Second.flac")
            .replace("INDEX 01 00:20:00", "INDEX 01 00:15:00"),
    );

    let (scanned, both) = folder_and_library_disc_ids(folder.path()).await;
    assert_eq!(scanned.len(), 2, "each sheet lays out a disc");
    assert_eq!(both, scanned);

    let mut decided = crate::folder_scanner::CandidateFileEdits::default();
    decided.sheet_places.set("Second.cue".to_string(), crate::folder_scanner::SheetPlace::Ignored);
    let (_, ignored) = folder_and_library_disc_ids_with_decisions(folder.path(), decided).await;
    assert_eq!(ignored.len(), 1, "re-identify reads the person's ignored sheet: {ignored:?}");
}
