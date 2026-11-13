//! A release and its reads: its detail, cover and gallery, deleting it, and the
//! album's primary release on two devices.

use super::*;
use crate::manager::tests::*;
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::library::LibraryRows;
use bae_records::{DbRelease, DbTrackWork, DbWork, LibraryImageType, NewReleaseFile};
use bae_util::content_type::ContentType;
use chrono::Utc;

#[tokio::test]
async fn work_detail_release_rows_are_display_ready() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_A, "Track Title", 1);
    let now = Utc::now();
    let work = DbWork {
        id: WORK_A.to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        musicbrainz_work_id: "mb-work-a".to_string(),
        created_at: now,
    };
    let track_work = DbTrackWork::new(
        &track.id,
        &work.id,
        0,
        Catalog::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    );

    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    manager
        .records
        .insert_library_rows_for_test(LibraryRows {
            works: vec![work.clone()],
            track_works: vec![track_work],
            ..LibraryRows::default()
        })
        .await
        .unwrap();
    store_test_cover(manager, &release.id, b"cover").await;

    let detail = views(manager).get_work_detail(&work.id).await.unwrap().expect("work detail");

    assert_eq!(detail.releases.len(), 1);
    let row = &detail.releases[0];
    assert_eq!(row.release_id, release.id);
    assert_eq!(row.album_id, album.id);
    assert_eq!(row.album_title, album.title);
    assert_eq!(
        row.name,
        bae_views::album_detail::ReleaseName::Described {
            year: Some(2024),
            media: bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1).media,
        }
    );
    assert_eq!(row.media, bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1).media);
    let cover = row.cover.as_ref().expect("work release cover");
    assert_eq!(cover.id, release.id);
    assert!(!cover.version.is_empty());
    assert_eq!(cover.image_type, bae_records::LibraryImageType::Cover);
}

#[tokio::test]
async fn test_delete_release_with_single_release_empties_album() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;

    manager.library.manager().delete_release(&release.id).await.unwrap();

    let album_result = views(manager.library.manager()).find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = album_releases(manager.library.manager(), &album.id).await;
    assert!(releases.is_empty());
}

/// A release the library doesn't hold is refused, as an album it doesn't hold
/// is: the request names nothing to delete. Its line says what it acted on
/// changed meanwhile.
#[tokio::test]
async fn delete_release_refuses_a_release_the_library_does_not_hold() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    manager.library.manager().delete_release(&release.id).await.unwrap();

    for release_id in [release.id.as_str(), "not-a-release"] {
        let error = manager.library.manager().delete_release(release_id).await.unwrap_err();
        assert!(
            matches!(error, LibraryError::ReleaseMissing { work: ReleaseWork::Edit, .. }),
            "{release_id}: {error}"
        );
        assert_eq!(
            error.category(),
            bae_diagnostics::UiErrorCategory::ChangedMeanwhile,
            "{release_id}"
        );
    }
}

#[tokio::test]
async fn test_delete_release_with_multiple_releases_preserves_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release1).await;
    insert_release(manager, &release2).await;

    manager.delete_release(&release1.id).await.unwrap();

    let album_result = find_album(manager, &album.id).await;
    assert!(album_result.is_some());
    let releases = album_releases(manager, &album.id).await;
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);
}

/// Deleted rows disappear immediately, offline too; the uploads their files
/// were queued for go with them, and the person's originals stay.
#[tokio::test]
async fn deletion_retires_queued_file_uploads_and_keeps_the_originals() {
    for delete_album in [false, true] {
        let (test, temp) = setup_test_manager().await;
        let manager = test.library.manager();
        let storage = connect_test_cloud(&test).await;
        let source = temp.path().join("source");
        let release = insert_release_with_queued_uploads(
            manager,
            &source,
            "Album",
            &[("track.flac", b"audio")],
        )
        .await;
        store_test_cover(manager, &release.id, b"cover").await;
        finish_bound_uploads(manager).await;
        storage.set_online(false);
        if delete_album {
            manager.delete_album(&release.album_id).await.unwrap();
        } else {
            manager.delete_release(&release.id).await.unwrap();
        }
        assert!(find_release(manager, &release.id).await.is_none());
        assert_eq!(std::fs::read(source.join("track.flac")).unwrap(), b"audio");
        assert_eq!(manager.store.queued_upload_count_for_test().await.unwrap(), 0);
    }
}

/// A closed store refuses deletion; reopening it proves no rows were changed.
#[tokio::test]
async fn deletion_refuses_an_unavailable_database_without_losing_rows() {
    for delete_album in [false, true] {
        let (manager, _temp, album, release) = manager_with_release().await;
        store_test_cover(manager.library.manager(), &release.id, b"cover").await;
        let before = manager.library.manager().store.every_row_for_test().await.unwrap();
        let id = manager.library.manager().store.store_id();
        manager.library.manager().store.close().await.unwrap();
        let error = if delete_album {
            manager.library.manager().delete_album(&album.id).await.unwrap_err()
        } else {
            manager.library.manager().delete_release(&release.id).await.unwrap_err()
        };
        assert!(matches!(error, LibraryError::Database(_)), "{error}");
        let handle = bae_store::configure_store(
            bae_store::test_devices::test_builder(&manager.installation),
            crate::SCHEMA,
        )
        .open(id, |_| ())
        .await
        .unwrap();
        let reopened =
            bae_store::test_devices::store_under(handle, manager.library.manager().clock.clone());
        assert_eq!(reopened.every_row_for_test().await.unwrap(), before);
        reopened.close().await.unwrap();
    }
}

/// Deleting a release cascade-deletes its `covers` row (the FK on `covers.id`
/// to `releases`), including a cover whose file really did reach the cloud —
/// the delete plan reads the published cover's file reference before the row
/// goes, so a lookup failure there would take the delete down with it.
#[tokio::test]
async fn delete_release_removes_its_cover_image() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    // A genuinely Remote release, so storing its cover publishes the file and
    // the delete runs against a cover that names a real cloud object.
    let release1 = ReleaseRef::of(
        manager,
        upload_release_unpaused(manager, &temp_dir.path().join("r1"), "Album One", false).await,
    )
    .await;
    // A sibling release so the album survives the single-release delete.
    let release2 = create_test_release(&release1.album_id);
    insert_release(manager, &release2).await;

    // Give release1 a cover: a `covers` row plus its file in one coven batch.
    store_test_cover(manager, &release1.id, b"image").await;

    wait_for_published_file(manager, bae_records::synced::COVERS_NAMESPACE, &release1.id).await;

    manager.delete_release(&release1.id).await.unwrap();

    // Row removed (the `covers` FK to `releases` cascade-deletes it).
    assert!(manager
        .records
        .find_library_image(&release1.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
}

/// A delete is planned against the release's rows and files before its write
/// opens. A cover replaced in between is a change the plan never saw, so the
/// write checks the plan against the rows it deletes and refuses a stale one.
#[tokio::test]
async fn delete_release_refuses_a_plan_whose_cover_changed() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    store_test_cover(manager.library.manager(), &release.id, b"cover").await;
    let plan = manager
        .library
        .manager()
        .records
        .plan_release_deletion(&release.id)
        .await
        .unwrap()
        .unwrap();
    store_test_cover(manager.library.manager(), &release.id, b"replacement cover").await;

    let error = manager
        .library
        .manager()
        .records
        .delete_release_with_cleanup(plan)
        .await
        .expect_err("a stale delete plan is refused");

    assert!(
        matches!(&error, bae_store::WriteError::Refused(bae_records::DeletionRefusal::PlanChanged(bae_records::Deleted::Release(id))) if *id == release.id),
        "{error:?}"
    );
    assert_eq!(
        LibraryError::from(error).category(),
        bae_diagnostics::UiErrorCategory::ChangedMeanwhile
    );
    assert!(find_release(manager.library.manager(), &release.id).await.is_some());
}

/// Two devices that each make a different release primary while apart agree
/// on one primary once they have pulled each other's writes: both releases
/// are ordered below the album's lowest, and the tie goes to the lower id.
#[test]
fn devices_setting_different_primaries_converge_on_one() {
    crate::test_libraries::run_two_device_test(check_converging_primaries);
}

async fn primary_of(db: &bae_store::Store, album_id: &str) -> String {
    bae_views::LibraryViews::new(db.clone())
        .find_album_detail(album_id)
        .await
        .unwrap()
        .expect("the album holds releases")
        .primary_release_id
}

async fn check_converging_primaries(devices: &mut crate::test_libraries::TwoDevices) {
    let manager = devices.a().manager();
    let album = create_test_album();
    manager.records.insert_album(&album).await.unwrap();
    let releases: Vec<DbRelease> = (0..3).map(|_| create_test_release(&album.id)).collect();
    for release in &releases {
        insert_release(manager, release).await;
    }
    let orders =
        format!("SELECT id || ':' || album_order FROM releases WHERE album_id = '{}'", album.id);
    devices.converge(&orders).await;
    assert_eq!(primary_of(devices.b().store(), &album.id).await, releases[0].id);

    devices.pause().await;
    devices.a().records().set_album_primary_release(&album.id, &releases[1].id).await.unwrap();
    devices.b().records().set_album_primary_release(&album.id, &releases[2].id).await.unwrap();
    devices.resume().await;
    devices.converge(&orders).await;

    let lower = std::cmp::min(&releases[1].id, &releases[2].id).clone();
    assert_eq!(primary_of(devices.a().store(), &album.id).await, lower);
    assert_eq!(primary_of(devices.b().store(), &album.id).await, lower);
}

#[tokio::test]
async fn release_detail_has_no_cover_without_a_cover_row() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let detail = views(manager.library.manager())
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
#[tokio::test]
async fn find_release_detail_does_not_panic_on_traversal_filenames_from_a_peer() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    // An image file whose stored filename is a traversal token, as a peer's
    // device can have stored it. It drives the gallery's file resolution and,
    // as the release's representative file, the pinned-cache check; neither
    // may follow it or panic.
    let file = NewReleaseFile::new(
        &release.id,
        "../../etc/y",
        bae_util::content_type::ContentType::Jpeg,
        bae_records::ReleaseFileRole::Artwork,
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        Utc::now(),
    );
    manager.add_file(&file, 5).await.unwrap();

    // The resolver that fires when a synced release surfaces in the UI. The
    // name must resolve to "no cover" / "no local gallery path", not a panic.
    let detail = views(manager)
        .find_release_detail(&release.id)
        .await
        .expect("resolving a release with a traversal filename must not error")
        .expect("the inserted release must resolve to a detail");
    assert!(detail.summary.cover.is_none(), "there is no cover row, so no cover");
    assert!(
        detail
            .gallery_items
            .iter()
            .all(|item| matches!(item.source, bae_records::GallerySource::ReleaseFile { .. })),
        "with no cover row there is no cover slot; the traversal image file is a \
             release-file item, read by id"
    );
}

/// A release's cover reference carries the cover's version, and replacing the
/// cover (attaching new bytes to its row) moves that version — the changed
/// field that fires the UI's per-field re-render and reloads the cover.
#[tokio::test]
async fn release_cover_version_moves_when_the_cover_row_is_reupserted() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;

    store_test_cover(manager.library.manager(), &release.id, b"cover").await;
    let before = views(manager.library.manager())
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id, "the cover id is the release id");

    // Replace (what change_cover does): same row, new bytes.
    store_test_cover(manager.library.manager(), &release.id, b"replacement cover").await;
    let after = views(manager.library.manager())
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present after overwrite");
    assert_ne!(before.version, after.version, "overwriting the cover must move its version");
}

#[tokio::test]
async fn album_detail_cover_is_versioned_and_moves_on_overwrite() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;

    // No cover row yet: the detail carries no cover reference.
    let detail = views(manager.library.manager())
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("detail present for known album");
    assert!(detail.cover.is_none());

    store_test_cover(manager.library.manager(), &release.id, b"cover").await;
    let before = views(manager.library.manager())
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id);

    // Replace the cover (what change_cover does): new bytes.
    store_test_cover(manager.library.manager(), &release.id, b"replacement cover").await;
    let after = views(manager.library.manager())
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
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    manager.records.insert_album(&album).await.unwrap();

    let detail = views(manager)
        .find_album_detail(&album.id)
        .await
        .expect("empty album aggregate must resolve without an error");

    assert!(detail.is_none());
}

#[tokio::test]
async fn find_release_detail_returns_some_for_known_id() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = None;
    release.pressing.facts = Default::default();
    release.release_name = None;
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let detail = views(manager)
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert_eq!(detail.summary.id, release.id);
    assert_eq!(detail.summary.album_id, album.id);
    // Only release in album, no year/media/release_name → the first release.
    assert_eq!(detail.name, bae_views::album_detail::ReleaseName::Numbered(1));
    assert!(detail.tracks.is_empty());
    assert!(detail.files.is_empty());
}

#[tokio::test]
async fn release_source_audio_summary_uses_every_file_without_track_formats() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;

    let source = |content_type, container, layout, codec: &str, bitrate_kbps: Option<i64>| {
        bae_records::source_audio::SourceAudioFile {
            layout: Some(layout),
            content_type,
            container,
            duration_ms: 90_000,
            format: bae_types::audio_format::AudioFormat {
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
        bae_util::audio_container::AudioContainer::Flac,
        bae_records::source_audio::SourceAudioLayout::Cue,
        "FLAC",
        None,
    );
    let mp3 = source(
        ContentType::Mp3,
        bae_util::audio_container::AudioContainer::Mp3,
        bae_records::source_audio::SourceAudioLayout::File,
        "MP3",
        Some(320),
    );
    for (name, facts) in [("01-disc.flac", flac.clone()), ("02-bonus.mp3", mp3.clone())] {
        let mut file = NewReleaseFile::new(
            &release.id,
            name,
            facts.content_type.clone(),
            bae_records::ReleaseFileRole::Audio,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            Utc::now(),
        );
        file.source_audio = Some(facts);
        manager.library.manager().add_file(&file, 1000).await.unwrap();
    }

    let detail = views(manager.library.manager())
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert!(detail.tracks.is_empty());
    assert_eq!(
        detail.source_audio,
        Some(bae_records::source_audio::SourceAudioSummary::Mixed {
            differences: vec![
                bae_records::source_audio::SourceAudioDifference::Layout {
                    layouts: vec![
                        bae_records::source_audio::SourceAudioLayout::Cue,
                        bae_records::source_audio::SourceAudioLayout::File,
                    ],
                },
                bae_records::source_audio::SourceAudioDifference::Codec {
                    codecs: vec!["FLAC".to_string(), "MP3".to_string()],
                },
            ],
        })
    );
}

#[tokio::test]
async fn find_release_detail_surfaces_seeded_tracks() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;

    // Seed two tracks; the detail resolver must surface both with their
    // titles and track numbers (not just report emptiness).
    let t1 = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Opening", 1);
    let t2 = bae_records::DbTrack::new_test(&release.id, TRACK_2, "Closing", 2);
    manager.library.manager().records.insert_track(&t1).await.unwrap();
    manager.library.manager().records.insert_track(&t2).await.unwrap();

    let detail = views(manager.library.manager())
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert_eq!(detail.tracks.len(), 2);
    let opening =
        detail.tracks.iter().find(|t| t.title == "Opening").expect("opening track surfaced");
    assert_eq!(opening.position_text, "1");
    assert!(detail.tracks.iter().any(|t| t.title == "Closing"), "closing track surfaced");
}

/// A compilation's rows each carry their own artist (the header names none); a
/// single-artist album's rows carry no display artist (they would only repeat
/// the header). Core decides this, so the four front-ends stop rendering it four
/// ways.
#[tokio::test]
async fn display_artist_is_set_only_for_a_compilation() {
    async fn resolve_display_artist(is_compilation: bool) -> Option<String> {
        let (test, _temp_dir) = setup_test_manager().await;
        let manager = test.library.manager();
        let mut album = create_test_album();
        album.is_compilation = is_compilation;
        let release = create_test_release(&album.id);
        let track = bae_records::DbTrack::new_test(&release.id, TRACK_A, "Track Title", 1);
        let guest = DbArtist {
            id: "75c512c4-41b6-438d-89a6-d5929fa0697d".to_string(),
            name: "Guest Performer".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        let track_artist = bae_records::DbTrackArtist::new(&track.id, &guest.id, 0, Utc::now());
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        manager.records.insert_artist(&guest).await.unwrap();
        manager.records.insert_track(&track).await.unwrap();
        manager.records.insert_track_artist(&track_artist).await.unwrap();

        let detail =
            views(manager).find_release_detail(&release.id).await.unwrap().expect("detail present");
        detail.tracks[0].display_artist.as_ref().map(|line| line.plain_text())
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
    let (manager, _temp_dir, _album, release) = manager_with_release().await;

    // An image file for the release with no local copy on this device — the
    // release's images live only in the cloud here.
    let image = bae_records::NewReleaseFile::new(
        &release.id,
        "back.jpg",
        bae_util::content_type::ContentType::Jpeg,
        bae_records::ReleaseFileRole::Artwork,
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        Utc::now(),
    );
    manager.library.manager().add_file(&image, 1234).await.unwrap();

    let detail = views(manager.library.manager())
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
    assert_eq!(
        item.label,
        bae_views::album_detail::GalleryLabel::File { name: "back.jpg".to_string() }
    );
    assert!(
        matches!(item.source, bae_records::GallerySource::ReleaseFile { .. }),
        "a release-file image is read by file id, not as the cover"
    );
}

#[tokio::test]
async fn cover_picker_excludes_unsupported_images_without_hiding_gallery_files() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let mut eligible = Vec::new();
    let mut all_images = Vec::new();
    for (name, content_type, supports_cover) in [
        ("front.jpg", ContentType::Jpeg, true),
        ("back.png", ContentType::Png, true),
        ("scan.gif", ContentType::Gif, true),
        ("scan.webp", ContentType::Webp, true),
        ("booklet.bmp", ContentType::Bmp, false),
        ("drawing.svg", ContentType::Svg, false),
    ] {
        let file = NewReleaseFile::new(
            &release.id,
            name,
            content_type,
            bae_records::ReleaseFileRole::Artwork,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            Utc::now(),
        );
        manager.library.manager().add_file(&file, 100).await.unwrap();
        if supports_cover {
            eligible.push(file.id.clone());
        }
        all_images.push(file.id);
    }
    let detail =
        views(manager.library.manager()).find_release_detail(&release.id).await.unwrap().unwrap();
    let mut cover_ids: Vec<_> = detail.cover_files.iter().map(|file| file.id.clone()).collect();
    let mut image_ids: Vec<_> = detail.image_files.iter().map(|file| file.id.clone()).collect();
    let mut gallery_ids: Vec<_> = detail.gallery_items.iter().map(|item| item.id.clone()).collect();
    eligible.sort();
    all_images.sort();
    cover_ids.sort();
    image_ids.sort();
    gallery_ids.sort();
    assert_eq!(cover_ids, eligible);
    assert_eq!(image_ids, all_images);
    assert_eq!(gallery_ids, all_images);
}

#[tokio::test]
async fn find_release_detail_name_uses_year_and_media() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = Some(2024);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 2);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let detail = views(manager).find_release_detail(&release.id).await.unwrap().unwrap();
    assert_eq!(
        detail.name,
        bae_views::album_detail::ReleaseName::Described {
            year: Some(2024),
            media: bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 2).media,
        }
    );
}

#[tokio::test]
async fn find_release_detail_name_prefers_release_name() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.release_name = Some("Deluxe Edition".to_string());
    release.pressing.year = Some(2024);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let detail = views(manager).find_release_detail(&release.id).await.unwrap().unwrap();
    assert_eq!(
        detail.name,
        bae_views::album_detail::ReleaseName::Named("Deluxe Edition".to_string())
    );
}

#[tokio::test]
async fn find_release_detail_uses_position_for_second_release() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release1 = create_test_release(&album.id);
    release1.pressing.year = None;
    release1.pressing.facts = Default::default();
    let mut release2 = create_test_release(&album.id);
    release2.pressing.year = None;
    release2.pressing.facts = Default::default();
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release1).await;
    insert_release(manager, &release2).await;

    let detail2 = views(manager).find_release_detail(&release2.id).await.unwrap().unwrap();
    assert_eq!(detail2.name, bae_views::album_detail::ReleaseName::Numbered(2));
}

#[tokio::test]
async fn find_release_detail_returns_none_for_unknown_id() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let detail = views(manager).find_release_detail("nonexistent-id").await.unwrap();
    assert!(detail.is_none());
}

// ── Storage page tests ───────────────────────────────────────────

/// A gallery read names a file of the release it reads from; a file of
/// another release is refused, found by the release and file together.
#[tokio::test]
async fn a_gallery_read_refuses_another_releases_file() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let other = create_test_release(&album.id);
    insert_release(manager.library.manager(), &other).await;
    let file = NewReleaseFile::new(
        &other.id,
        "cover.jpg",
        bae_util::content_type::ContentType::Jpeg,
        bae_records::ReleaseFileRole::Artwork,
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        Utc::now(),
    );
    manager.library.manager().add_file(&file, 5).await.unwrap();

    let error = manager
        .library
        .manager()
        .read_gallery_bytes(
            &release.id,
            &bae_records::GallerySource::ReleaseFile { file_id: file.id.clone() },
        )
        .await
        .expect_err("the file is another release's");
    assert!(matches!(error, LibraryError::FileMissing { .. }), "{error}");
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::ChangedMeanwhile);
}

/// Deleting a release deletes the coven file of each of its files: its upload
/// leaves the queue, since no row references the file any more.
#[tokio::test]
async fn deleting_a_release_retires_its_queued_uploads() {
    let (manager, temp, release, _) = queued_upload_fixture("queued").await;
    manager.library.manager().delete_release(&release.id).await.unwrap();
    assert_eq!(manager.library.manager().store.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.library.store().pending().await.unwrap().is_empty());
    assert_eq!(std::fs::read(temp.path().join("queued/a.flac")).unwrap().len(), 1000);
}

const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015"; // was "track-work-a"

const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4"; // was "work-a"
