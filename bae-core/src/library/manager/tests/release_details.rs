/// Insert (or overwrite) a release's `covers` row. The cover reference reads
/// the row (not the bytes), and each upsert stamps a fresh `_updated_at`, so
/// re-calling this moves the cover's version — what `change_cover` does when it
/// replaces a cover in place.
async fn add_cover_row(manager: &LibraryManager, release_id: &str) {
    manager
        .upsert_library_image(&crate::db::DbLibraryImage {
            id: release_id.to_string(),
            blob_id: format!("{release_id}-cover-blob"),
            image_type: LibraryImageType::Cover,
            content_type: crate::util::content_type::ContentType::Jpeg,
            file_size: 5,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: manager.clock.now(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn release_detail_has_no_cover_without_a_cover_row() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert!(detail.summary.cover.is_none());
}

/// A peer can ship a `release_files` row whose `original_filename` is a
/// path-traversal token. Primary keys can no longer carry one — coven validates
/// every synced-table id, on a local write and on an incoming changeset alike —
/// but the path *fragments* on a row are ordinary text it never inspects, and a
/// synced row makes such a value durable. So the display resolver that fires on
/// every sync cycle must treat it as a missing asset rather than panic: a panic
/// here crash-loops every device, every cycle.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn find_release_detail_does_not_panic_on_traversal_filenames_from_a_peer() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file whose stored filename is a traversal token. bae's own write
    // path validates the fragment and refuses it, which is why the value has to
    // be written straight onto the row the way coven applies a peer's changeset
    // — the seam `set_original_filename_for_test` exists for. It drives the
    // gallery's blob resolution and, as the release's representative blob, the
    // pinned-cache check; both must reject the bad fragment, not panic.
    let file = DbFile::new(
        &release.id,
        "cover.jpg",
        5,
        crate::util::content_type::ContentType::Jpeg,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .set_original_filename_for_test(&file.id, "../../etc/y")
        .await
        .unwrap();

    // The resolver that fires when a synced release surfaces in the UI. The
    // bad fragment must resolve to "no cover" / "no local gallery path", not a
    // panic.
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .expect("resolving a release with a traversal filename must not error")
        .expect("the inserted release must resolve to a detail");
    assert!(
        detail.summary.cover.is_none(),
        "there is no cover row, so no cover"
    );
    assert!(
        detail.gallery_items.iter().all(|item| matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        )),
        "with no cover row there is no cover slot; the traversal image file is a \
             release-file item, read by id"
    );
}

/// A release's cover reference carries the `covers` row's `_updated_at` as its
/// version, and overwriting the cover (re-upserting the row) moves that
/// version — the changed field that fires the UI's per-field re-render and
/// reloads the cover.
#[tokio::test]
async fn release_cover_version_moves_when_the_cover_row_is_reupserted() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id, "the cover id is the release id");

    // Overwrite (what change_cover does): same row, fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present after overwrite");
    assert_ne!(
        before.version, after.version,
        "overwriting the cover must move its version"
    );
}

/// Each storage-page row carries its own release's cover reference, not the
/// album's primary-release cover. Two releases of one album, each with its own
/// `covers` row, resolve to their own ids; a non-primary release's row resolves
/// to its own cover rather than the album's primary.
#[tokio::test]
async fn storage_page_rows_carry_each_releases_own_cover() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    // release1 is the album's primary, so its cover is the album-level cover.
    manager
        .set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    add_cover_row(&manager, &release1.id).await;
    add_cover_row(&manager, &release2.id).await;

    let page = manager
        .get_storage_page(
            &crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction: crate::db::SortDirection::Ascending,
            },
            crate::db::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap();

    let row1 = page
        .rows
        .iter()
        .find(|r| r.release.id == release1.id)
        .expect("release1 row present");
    let row2 = page
        .rows
        .iter()
        .find(|r| r.release.id == release2.id)
        .expect("release2 row present");

    // Each release row resolves to that release's own cover.
    assert_eq!(row1.release.cover.as_ref().unwrap().id, release1.id);
    assert_eq!(row2.release.cover.as_ref().unwrap().id, release2.id);

    // The album carries the primary release's cover; the non-primary release's
    // row carries its own, distinct from the album's.
    assert_eq!(row2.album.cover.as_ref().unwrap().id, release1.id);
    assert_ne!(
        row2.release.cover.as_ref().unwrap().id,
        row2.album.cover.as_ref().unwrap().id
    );
}

#[tokio::test]
async fn album_detail_cover_is_versioned_and_moves_on_overwrite() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // No cover row yet: the detail carries no cover reference.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("detail present for known album");
    assert!(detail.cover.is_none());

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id);

    // Overwrite the cover (what change_cover does): fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present after overwrite");

    // The summary the UI re-renders against carries a changed version, which
    // fires the per-field re-render and reloads the cover.
    assert_ne!(before.version, after.version);
}

#[tokio::test]
async fn find_album_detail_returns_none_when_releases_vanish() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();

    let detail = manager
        .find_album_detail(&album.id)
        .await
        .expect("empty album aggregate must resolve without an error");

    assert!(detail.is_none());
}

#[tokio::test]
async fn resolve_album_detail_errors_when_releases_vanish() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();

    let err = manager
        .resolve_album_detail(crate::db::DbAlbumDetail {
            album,
            artists: Vec::new(),
            releases: Vec::new(),
        })
        .await
        .expect_err("empty album detail must return an error");

    assert!(
        matches!(&err, LibraryError::TrackMapping(message) if message.contains("has no releases")),
        "empty album detail error should name the missing releases: {err}"
    );
}

#[tokio::test]
async fn find_release_detail_returns_some_for_known_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = None;
    release.pressing.format = None;
    release.release_name = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert_eq!(detail.summary.id, release.id);
    assert_eq!(detail.summary.album_id, album.id);
    // Only release in album, no year/format/release_name → "Release 1".
    assert_eq!(detail.display_name, "Release 1");
    assert!(detail.tracks.is_empty());
    assert!(detail.files.is_empty());
}

#[tokio::test]
async fn release_source_audio_summary_uses_every_file_without_track_formats() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let source = |content_type, layout, codec: &str, bitrate_kbps: Option<i64>| {
        crate::album_detail::SourceAudioFile {
            layout: Some(layout),
            content_type,
            duration_ms: 90_000,
            format: crate::album_detail::AudioFormat {
                codec: codec.to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: bitrate_kbps.is_none().then_some(16),
                bitrate_kbps,
                channels: 2,
            },
        }
    };
    let flac = source(
        ContentType::Flac,
        crate::album_detail::SourceAudioLayout::Cue,
        "FLAC",
        None,
    );
    let mp3 = source(
        ContentType::Mp3,
        crate::album_detail::SourceAudioLayout::File,
        "MP3",
        Some(320),
    );
    for (name, facts) in [("01-disc.flac", flac.clone()), ("02-bonus.mp3", mp3.clone())] {
        let mut file = DbFile::new(
            &release.id,
            name,
            1000,
            facts.content_type.clone(),
            Uuid::new_v4().to_string(),
            Utc::now(),
            crate::util::fs::hash_bytes(name.as_bytes()),
        );
        file.source_audio = Some(facts);
        manager.add_file(&file).await.unwrap();
    }

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert!(detail.tracks.is_empty());
    assert_eq!(
        detail.source_audio,
        Some(crate::album_detail::SourceAudioSummary::Mixed {
            descriptors: vec![
                flac.descriptor().unwrap(),
                mp3.descriptor().unwrap(),
            ],
        })
    );
}

#[tokio::test]
async fn find_release_detail_surfaces_seeded_tracks() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Seed two tracks; the detail resolver must surface both with their
    // titles and track numbers (not just report emptiness).
    let t1 = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Opening", Some(1));
    let t2 = crate::db::DbTrack::new_test(&release.id, TRACK_2, "Closing", Some(2));
    manager.database.insert_track(&t1).await.unwrap();
    manager.database.insert_track(&t2).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert_eq!(detail.tracks.len(), 2);
    let opening = detail
        .tracks
        .iter()
        .find(|t| t.title == "Opening")
        .expect("opening track surfaced");
    assert_eq!(opening.track_number, Some(1));
    assert!(
        detail.tracks.iter().any(|t| t.title == "Closing"),
        "closing track surfaced"
    );
}

/// A compilation's rows each carry their own artist (the header names none); a
/// single-artist album's rows carry no display artist (they would only repeat
/// the header). Core decides this, so the four front-ends stop rendering it four
/// ways.
#[tokio::test]
async fn display_artist_is_set_only_for_a_compilation() {
    async fn resolve_display_artist(is_compilation: bool) -> Option<String> {
        let (manager, _temp_dir) = setup_test_manager().await;
        let mut album = create_test_album();
        album.is_compilation = is_compilation;
        let release = create_test_release(&album.id);
        let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
        let guest = DbArtist {
            id: "75c512c4-41b6-438d-89a6-d5929fa0697d".to_string(),
            name: "Guest Performer".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        let track_artist = crate::db::DbTrackArtist::new(
            &track.id,
            &guest.id,
            0,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        manager.database.insert_artist(&guest).await.unwrap();
        manager.database.insert_track(&track).await.unwrap();
        manager
            .database
            .insert_track_artist(&track_artist)
            .await
            .unwrap();

        let detail = manager
            .find_release_detail(&release.id)
            .await
            .unwrap()
            .expect("detail present");
        detail.tracks[0].display_artist.clone()
    }

    assert_eq!(
        resolve_display_artist(true).await.as_deref(),
        Some("Guest Performer"),
        "a compilation row shows its own artist"
    );
    assert_eq!(
        resolve_display_artist(false).await,
        None,
        "a single-artist album row shows no artist"
    );
}

#[tokio::test]
async fn gallery_includes_cloud_only_image_files_with_no_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file for the release with no local copy on this device — the
    // release's images live only in the cloud here.
    let image = crate::db::DbFile::new(
        &release.id,
        "back.jpg",
        1234,
        crate::util::content_type::ContentType::Jpeg,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&image).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");

    // The lightbox shows every image the release has: the image file is
    // surfaced as a gallery item read by file id (fetched on demand), a
    // release-file source, not the cover slot.
    let item = detail
        .gallery_items
        .iter()
        .find(|g| g.id == image.id)
        .expect("image file surfaced in gallery");
    assert_eq!(item.label, "back.jpg");
    assert!(
        matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        ),
        "a release-file image is read by file id, not as the cover"
    );
}

/// `change_cover` resizes whatever the user picks to a ≤600 JPEG thumbnail
/// before storing it: a 900×300 PNG release image lands as a 600×200 JPEG blob
/// (downscaled to fit 600, aspect kept), and the `covers` row records JPEG.
#[tokio::test]
async fn change_cover_stores_a_resized_jpeg_thumbnail() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An oversized non-JPEG release image on disk, registered as the release's
    // user-provided file so `change_cover` reads it back through coven.
    let source_dir = TempDir::new().unwrap();
    let cover_bytes = {
        let img = ::image::RgbImage::from_pixel(900, 300, ::image::Rgb([20, 160, 90]));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    std::fs::write(source_dir.path().join("art.png"), &cover_bytes).unwrap();
    let file = DbFile::new(
        &release.id,
        "art.png",
        cover_bytes.len() as i64,
        ContentType::Png,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(&cover_bytes),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .change_cover(
            &release.id,
            CoverSelection::ReleaseImage {
                file_id: file.id.clone(),
            },
        )
        .await
        .unwrap();

    // The stored blob decodes as a ≤600 JPEG, not the 900×300 PNG source.
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        ::image::guess_format(&stored).unwrap(),
        ::image::ImageFormat::Jpeg
    );
    let decoded = ::image::load_from_memory(&stored).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 200));

    // The row describes the stored thumbnail: JPEG, and its size matches.
    let row = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row stored");
    assert_eq!(row.content_type, ContentType::Jpeg);
    assert_eq!(row.file_size, stored.len() as i64);
}

/// A cover can be changed again and again. coven's `(namespace, blob id)` names one
/// immutable byte-string — a blob's bytes are never rewritten under a live id — so
/// each change mints a NEW `blob_id`, repoints the `covers` row at it, and deletes
/// the blob it replaced. The row's hash and size describe the newly stored bytes,
/// and the old blob's cloud object is queued for deletion.
#[tokio::test]
async fn change_cover_twice_replaces_the_cover_blob() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Two visibly different release images, so the two stored thumbnails differ.
    let source_dir = TempDir::new().unwrap();
    let png = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let add_source = |name: &str, bytes: &[u8]| {
        std::fs::write(source_dir.path().join(name), bytes).unwrap();
        DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            ContentType::Png,
            Uuid::new_v4().to_string(),
            Utc::now(),
            crate::util::fs::hash_bytes(bytes),
        )
    };
    let green = add_source("green.png", &png([20, 160, 90]));
    let red = add_source("red.png", &png([200, 40, 40]));
    manager.add_file(&green).await.unwrap();
    manager.add_file(&red).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    let change_to = async |file: &DbFile| {
        manager
            .change_cover(
                &release.id,
                CoverSelection::ReleaseImage {
                    file_id: file.id.clone(),
                },
            )
            .await
    };
    let cover_row = async || {
        manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored")
    };

    change_to(&green).await.unwrap();
    let first = cover_row().await;

    // The second change is the one that used to fail: it re-put a blob under an id
    // the `covers` row already referenced.
    change_to(&red).await.unwrap();
    let second = cover_row().await;

    // The row moved to a new blob, and describes the bytes that blob holds.
    assert_ne!(
        second.blob_id, first.blob_id,
        "a replaced cover is a new blob, not new bytes under the old id"
    );
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        second.content_hash.as_str(),
        crate::util::fs::hash_bytes(&stored).as_str()
    );
    assert_eq!(second.file_size, stored.len() as i64);

    // The bytes really are the second image's, not the first's.
    let first_stored_len = first.file_size;
    assert_ne!(
        stored.len() as i64,
        first_stored_len,
        "the two source images must produce different thumbnails for this test to mean anything"
    );

    // The row now points at the new blob, so the old one has no row reference to
    // address it by; what must hold is that its bytes are gone from this device —
    // the replace declared its deletion, so coven reclaimed them. This release is
    // Local, so there is no cloud object behind the old blob and nothing to
    // tombstone; the Remote case is `delete_release_removes_its_cover_image`.
    assert!(
        !manager
            .local_blob_exists_for_test(crate::sync::COVERS_NAMESPACE, &first.blob_id)
            .expect("a valid blob path"),
        "the replaced cover blob's bytes must be reclaimed"
    );
}

/// On a browsable home a cover's cloud key is the row's readable `cloud_path`, and
/// that path carries the blob id — so replacing a cover writes a NEW object rather
/// than overwriting the one it replaces. A reused key cannot be made to converge:
/// two devices replacing the same cover would race for one object, and a device
/// applying a changeset written before a replacement could never satisfy that
/// changeset's content hash. Distinct keys leave the superseded object readable
/// until its tombstone is collected.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn replacing_a_cover_on_a_browsable_home_writes_a_distinct_cloud_key() {
    let (manager, _temp_dir) = setup_browsable_test_manager().await;
    manager
        .connect_test_cloud_home(Arc::new(InMemoryCloudHome::new()), CloudCipher::Plaintext)
        .await
        .expect("connect browsable in-memory cloud home");
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let jpeg = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        crate::util::cover::resize_cover(&buf.into_inner()).unwrap()
    };
    let store_cover = async |bytes: Vec<u8>| {
        let mut image = DbLibraryImage::cover(
            &release.id,
            &Uuid::new_v4().to_string(),
            "local",
            None,
            &bytes,
            manager.clock.now(),
        );
        image.cloud_path = manager
            .database
            .cover_cloud_path_for_storage(
                crate::config::HomeStorage::Browsable,
                &image.id,
                &image.blob_id,
                &image.content_type,
            )
            .await
            .unwrap();
        manager
            .store_library_image_blob(&image, &bytes)
            .await
            .unwrap();
        wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await;
        let stored = manager
            .database
            .row_blob_ref(crate::sync::COVERS_NAMESPACE, &release.id)
            .await
            .unwrap();
        let image = manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored");
        (image, stored)
    };

    let (first, first_stored) = store_cover(jpeg([20, 160, 90])).await;
    let (second, second_stored) = store_cover(jpeg([200, 40, 40])).await;

    // Each row's readable path names its own blob, so the two never collide.
    assert_eq!(
        first.cloud_path.as_deref(),
        Some(format!("{}/{}/cover-{}.jpg", album.id, release.id, first.blob_id).as_str())
    );
    assert_ne!(
        first.cloud_path, second.cloud_path,
        "a replaced cover must not reuse the object its predecessor occupies"
    );

    // The two keys really are distinct objects, so writing the second never
    // overwrites the first.
    let old_blob = crate::sync::image_blob_ref(
        crate::sync::COVERS_NAMESPACE,
        &first.blob_id,
        first.cloud_path.clone(),
    );
    let new_blob = crate::sync::image_blob_ref(
        crate::sync::COVERS_NAMESPACE,
        &second.blob_id,
        second.cloud_path.clone(),
    );
    let old_key = manager.database.blob_cloud_key(&old_blob).unwrap();
    let new_key = manager.database.blob_cloud_key(&new_blob).unwrap();
    assert_ne!(old_key, new_key);
    assert!(
        first_stored.stored().is_some() && second_stored.stored().is_some(),
        "both covers reached the cloud"
    );
    assert_ne!(first_stored.stored(), second_stored.stored());
}

/// Queueing an album expands to its PRIMARY release's tracks, not the
/// earliest-imported one. When the user picks a non-default primary (e.g. a
/// later remaster over the original vinyl rip), enqueueing the album must
/// play the chosen release.
#[tokio::test]
async fn resolve_to_track_ids_expands_album_to_primary_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    // release1 is imported first (older created_at, so it sorts first);
    // release2 is the user's chosen primary.
    let mut release1 = create_test_release(&album.id);
    release1.created_at = Utc::now() - chrono::Duration::days(1);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let old = crate::db::DbTrack::new_test(
        &release1.id,
        "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
        "Old A",
        Some(1),
    );
    let p1 = crate::db::DbTrack::new_test(
        &release2.id,
        "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
        "New A",
        Some(1),
    );
    let p2 = crate::db::DbTrack::new_test(
        &release2.id,
        "cc4181bc-58f5-4722-8116-fab2099f5d32",
        "New B",
        Some(2),
    );
    manager.database.insert_track(&old).await.unwrap();
    manager.database.insert_track(&p1).await.unwrap();
    manager.database.insert_track(&p2).await.unwrap();

    manager
        .set_album_primary_release(&album.id, &release2.id)
        .await
        .unwrap();

    let resolved = manager
        .resolve_to_track_ids(std::slice::from_ref(&album.id))
        .await
        .unwrap();
    assert!(resolved.contains(&"cc4180bc-58f5-456f-8116-f9b2099f5b7f".to_string()));
    assert!(resolved.contains(&"cc4181bc-58f5-4722-8116-fab2099f5d32".to_string()));
    assert!(
        !resolved.contains(&"48ae00a1-d7a5-443c-8240-f999fc4ddfcc".to_string()),
        "must not expand to the non-primary release's tracks"
    );
    assert_eq!(resolved.len(), 2);
}

#[tokio::test]
async fn resolve_to_track_ids_rejects_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let err = manager
        .resolve_to_track_ids(&["missing-id".to_string()])
        .await
        .unwrap_err();
    assert!(
        matches!(err, LibraryError::TrackMapping(message) if message.contains("missing-id")),
        "unknown ids must fail instead of being treated as track ids"
    );
}

#[tokio::test]
async fn find_release_detail_display_name_uses_year_format_fallback() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "2024 CD");
}

#[tokio::test]
async fn find_release_detail_display_name_prefers_release_name() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.release_name = Some("Deluxe Edition".to_string());
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "Deluxe Edition");
}

#[tokio::test]
async fn find_release_detail_uses_position_for_second_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release1 = create_test_release(&album.id);
    release1.pressing.year = None;
    release1.pressing.format = None;
    let mut release2 = create_test_release(&album.id);
    release2.pressing.year = None;
    release2.pressing.format = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let detail2 = manager
        .find_release_detail(&release2.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail2.display_name, "Release 2");
}

#[tokio::test]
async fn find_release_detail_returns_none_for_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let detail = manager.find_release_detail("nonexistent-id").await.unwrap();
    assert!(detail.is_none());
}


// ── Storage page tests ───────────────────────────────────────────
