//! How library rows meet across devices: two devices of one library write
//! while apart, sync, and must land on one library with nothing held back —
//! no duplicate of a row both devices described, and no pull that can never
//! apply.

use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::{MetadataRef, ReleaseRecord};
use bae_import::{ArtistAssignment, ReleaseUserEdit};
use bae_library::test_libraries::{run_two_device_test, TestDevice, TwoDevices};
use bae_records::test_fixtures::library::LibraryRows;
use bae_records::ExistingArtist;
use bae_records::{DbAlbum, DbArtist, DbRelease, DbTrack, DbTrackWork, DbWork};

const MB_ARTIST: &str = "5b11f4ce-a62d-471e-81fc-a69a8278c7da";
const DISCOGS_ARTIST: &str = "3840";
const MB_WORK: &str = "1d8e2b8f-8e9a-3b5c-9d1e-2f4a6b8c0d1e";
const MB_GROUP: &str = "0c9f2c1e-5b8a-4c8e-9f3a-1b2c3d4e5f60";

/// An artist a catalog names. Each catalog's artist carries its own name, so
/// a MusicBrainz artist and a Discogs artist are two library artists until a
/// person merges them, rather than one joined by name.
fn catalog_artist(
    musicbrainz_artist_id: Option<&str>,
    discogs_artist_id: Option<&str>,
) -> DbArtist {
    let name = match (musicbrainz_artist_id, discogs_artist_id) {
        (Some(_), _) => "Artist Name One",
        (None, Some(_)) => "Artist Name Two",
        (None, None) => "Artist Name",
    };
    DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_artist_id.map(str::to_string),
        musicbrainz_artist_id: musicbrainz_artist_id.map(str::to_string),
        created_at: chrono::Utc::now(),
    }
}

/// A Remote album credited to `artist`, found or created the way a metadata
/// edit resolves its artists, holding one release.
/// Returns `(artist_id, album_id, release_id)`.
async fn remote_album_by(
    device: &TestDevice,
    artist: DbArtist,
    title: &str,
) -> (String, String, String) {
    let artist_id = credited_artist(device, artist).await;
    let (album_id, release_id) = album_credited_to(device, &artist_id, title).await;
    (artist_id, album_id, release_id)
}

/// The library artist `artist` resolves to on `device`, as a metadata edit
/// resolves it.
async fn credited_artist(device: &TestDevice, artist: DbArtist) -> String {
    device
        .manager()
        .find_or_create_artists(&[artist])
        .await
        .expect("resolve the album's artist")
        .remove(0)
}

/// A Remote album credited to the library artist `artist_id`, holding one
/// release. Returns `(album_id, release_id)`.
async fn album_credited_to(device: &TestDevice, artist_id: &str, title: &str) -> (String, String) {
    // The album arrives with its release in one write, as an import writes
    // them.
    let album = DbAlbum::new_test(title);
    let credit = bae_records::DbAlbumArtist::new_test(&album.id, artist_id);
    let release = DbRelease { ..DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string()) };
    device
        .records()
        .insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();
    (album.id, release.id)
}

async fn remote_release_in(device: &TestDevice, album_id: &str) -> String {
    let release = DbRelease { ..DbRelease::new_test(album_id, &uuid::Uuid::new_v4().to_string()) };
    device.records().insert_release(&release).await.unwrap();
    release.id
}

async fn texts(db: &bae_store::Store, query: &str) -> Vec<String> {
    db.query_texts_for_test(query).await.unwrap()
}

/// The artists the artist browse lists.
async fn listed_artists(db: &bae_store::Store) -> Vec<String> {
    bae_views::LibraryViews::new(db.clone())
        .get_artist_page(&[], 0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|summary| summary.raw.artist.id)
        .collect()
}

/// The albums the album browse lists.
async fn listed_albums(db: &bae_store::Store) -> Vec<String> {
    bae_views::LibraryViews::new(db.clone())
        .get_album_page(&[], 0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|summary| summary.id)
        .collect()
}

async fn album_artist_ids(records: &bae_records::Records, album_id: &str) -> Vec<String> {
    records
        .get_artists_for_album(album_id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.artist.id)
        .collect()
}

async fn assert_nothing_held(devices: &TwoDevices) {
    for (name, device) in [("A", devices.a()), ("B", devices.b())] {
        let held = device.store().pending().await.unwrap();
        assert!(held.is_empty(), "device {name} holds {held:?}");
    }
}

/// Both devices have pulled each other's writes: nothing is held, and each
/// sees every release in `release_ids`.
async fn releases_on_both(devices: &TwoDevices, release_ids: &[&str]) {
    let wanted = release_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    devices
        .settle("every release on both devices", async |_, database| {
            let present = texts(database, "SELECT id FROM releases").await;
            wanted.iter().all(|id| present.contains(id))
        })
        .await;
}

/// The same MusicBrainz artist, credited on each device before either has
/// seen the other's library, is one artist once they sync.
#[test]
fn one_catalog_artist_credited_on_two_devices_is_one_artist() {
    run_two_device_test(async |devices| {
        devices.pause().await;
        let (_, _, release_a) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Album A").await;
        let (_, _, release_b) =
            remote_album_by(devices.b(), catalog_artist(Some(MB_ARTIST), None), "Album B").await;
        devices.resume().await;

        releases_on_both(devices, &[&release_a, &release_b]).await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                listed_artists(db).await.len(),
                1,
                "one MusicBrainz artist is one library artist on every device"
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// The same MusicBrainz work, performed on a track on each device, is one work
/// once they sync.
#[test]
fn one_catalog_work_performed_on_two_devices_is_one_work() {
    run_two_device_test(async |devices| {
        devices.pause().await;
        let mut releases = Vec::new();
        for device in [devices.a(), devices.b()] {
            let artist_id = device
                .manager()
                .find_or_create_artists(&[catalog_artist(Some(MB_ARTIST), None)])
                .await
                .unwrap()
                .remove(0);
            let album = DbAlbum::new_test("Album");
            let credit = bae_records::DbAlbumArtist::new_test(&album.id, &artist_id);
            let release =
                DbRelease { ..DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string()) };
            let track =
                DbTrack::new_test(&release.id, &uuid::Uuid::new_v4().to_string(), "Track", 1);
            device
                .records()
                .insert_album_with_release_and_tracks(
                    &album,
                    &[credit],
                    &release,
                    &[bae_records::DbReleaseMedium::new_test(&release.id)],
                    std::slice::from_ref(&track),
                    &[],
                )
                .await
                .unwrap();
            let work = DbWork {
                id: bae_records::identity::work_id(MB_WORK),
                title: "Work".to_string(),
                disambiguation: None,
                work_type: None,
                musicbrainz_work_id: MB_WORK.to_string(),
                created_at: chrono::Utc::now(),
            };
            let track_work = DbTrackWork::new(
                &track.id,
                &work.id,
                0,
                Catalog::MusicBrainz,
                uuid::Uuid::new_v4().to_string(),
                chrono::Utc::now(),
            );
            device
                .records()
                .insert_library_rows_for_test(LibraryRows {
                    works: vec![work],
                    track_works: vec![track_work],
                    ..LibraryRows::default()
                })
                .await
                .unwrap();
            releases.push(release.id);
        }
        devices.resume().await;

        releases_on_both(devices, &[&releases[0], &releases[1]]).await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                texts(db, "SELECT id FROM works").await.len(),
                1,
                "one MusicBrainz work is one library work on every device"
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// Two pressings of one MusicBrainz release group, identified on different
/// devices, belong to one album once they sync — the album a single device
/// files both under.
#[test]
fn one_release_group_identified_on_two_devices_is_one_album() {
    run_two_device_test(async |devices| {
        devices.pause().await;
        let mut releases = Vec::new();
        for (device, pressing) in [
            (devices.a(), "7a3e1b2c-4d5e-4f60-8a9b-0c1d2e3f4a5b"),
            (devices.b(), "8b4f2c3d-5e6f-4a71-9b0c-1d2e3f4a5b6c"),
        ] {
            let (_, _, release_id) =
                remote_album_by(device, catalog_artist(Some(MB_ARTIST), None), "Album").await;
            device
                .manager()
                .set_records(
                    &release_id,
                    vec![ReleaseRecord::new(
                        &MetadataRef::new(Catalog::MusicBrainz, pressing),
                        Some(MB_GROUP.to_string()),
                    )],
                )
                .await
                .unwrap();
            releases.push(release_id);
        }
        devices.resume().await;

        releases_on_both(devices, &[&releases[0], &releases[1]]).await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                listed_albums(db).await.len(),
                1,
                "one release group is one album on every device"
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// The edit crediting `artist_ids`, in order, as a release's album artists.
async fn album_artist_edit(device: &TestDevice, artist_ids: &[&str]) -> ReleaseUserEdit {
    let mut assignments = Vec::new();
    for id in artist_ids {
        let artist = device
            .records()
            .find_artist_by_id(id)
            .await
            .unwrap()
            .expect("the credited artist is in the library");
        assignments.push(ArtistAssignment::Picked {
            credit_name: None,
            artist: ExistingArtist::from(artist),
        });
    }
    ReleaseUserEdit {
        album_title: "Album".to_string(),
        album_artist_assignments: bae_import::Credited::plain_list(assignments),
        album_year: None,
        pressing: bae_types::pressing::Pressing::blank(),
        tracks: Vec::new(),
    }
}

/// Both devices credit the same second artist on one album while apart. The
/// credit is one fact, and neither device's edit is refused.
#[test]
fn the_same_album_credit_added_on_two_devices_merges() {
    run_two_device_test(async |devices| {
        let (first, album_id, release_id) =
            remote_album_by(devices.a(), catalog_artist(None, Some(DISCOGS_ARTIST)), "Album").await;
        let (second, _, other_release) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Other").await;
        releases_on_both(devices, &[&release_id, &other_release]).await;

        devices.pause().await;
        for device in [devices.a(), devices.b()] {
            let edit = album_artist_edit(device, &[&first, &second]).await;
            device.manager().apply_release_metadata_user_edit(&release_id, &edit).await.unwrap();
        }
        devices.resume().await;

        devices
            .converge("SELECT id || ' ' || album_id || ' ' || artist_id FROM album_artists")
            .await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                album_artist_ids(&bae_records::test_fixtures::test_records(db), &album_id).await,
                vec![first.clone(), second.clone()]
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// Both devices identify one release against the same second catalog while
/// apart. The release has one record per catalog, and neither edit is refused.
#[test]
fn the_same_record_set_on_two_devices_merges() {
    run_two_device_test(async |devices| {
        let (_, _, release_id) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Album").await;
        let musicbrainz = ReleaseRecord::new(
            &MetadataRef::new(Catalog::MusicBrainz, "7a3e1b2c-4d5e-4f60-8a9b-0c1d2e3f4a5b"),
            Some(MB_GROUP.to_string()),
        );
        devices.a().manager().set_records(&release_id, vec![musicbrainz.clone()]).await.unwrap();
        let records_of =
            format!("SELECT catalog FROM release_records WHERE release_id = '{release_id}'");
        let first = records_of.clone();
        devices
            .settle("the first record on both devices", async |_, database| {
                texts(database, &first).await.len() == 1
            })
            .await;

        devices.pause().await;
        let discogs = ReleaseRecord::new(
            &MetadataRef::new(Catalog::Discogs, "249504"),
            Some("20170".to_string()),
        );
        for device in [devices.a(), devices.b()] {
            device
                .manager()
                .set_records(&release_id, vec![musicbrainz.clone(), discogs.clone()])
                .await
                .unwrap();
        }
        devices.resume().await;

        devices
            .converge("SELECT id || ' ' || release_id || ' ' || catalog FROM release_records")
            .await;
        assert_eq!(texts(devices.a().store(), &records_of).await.len(), 2);
        assert_nothing_held(devices).await;
    });
}

/// Two library artists holding one MusicBrainz id, on both devices: device
/// A gives its Discogs artist the MusicBrainz id while device B, apart,
/// credits the MusicBrainz artist on its own. Returns the artist a write
/// meeting the id keeps — the one with the smaller id — the one it merges
/// into it, and the albums' releases.
async fn one_id_on_two_artists(devices: &TwoDevices) -> (String, String, Vec<String>) {
    devices.pause().await;
    let (discogs, _, discogs_release) =
        remote_album_by(devices.a(), catalog_artist(None, Some(DISCOGS_ARTIST)), "Album").await;
    let filled =
        credited_artist(devices.a(), catalog_artist(Some(MB_ARTIST), Some(DISCOGS_ARTIST))).await;
    assert_eq!(filled, discogs, "A's Discogs artist takes the MusicBrainz id");
    let (musicbrainz, _, musicbrainz_release) =
        remote_album_by(devices.b(), catalog_artist(Some(MB_ARTIST), None), "Other").await;
    assert_ne!(musicbrainz, discogs);
    devices.resume().await;
    let releases = vec![discogs_release, musicbrainz_release];
    releases_on_both(devices, &releases.iter().map(String::as_str).collect::<Vec<_>>()).await;
    let (survivor, absorbed) =
        if discogs < musicbrainz { (discogs, musicbrainz) } else { (musicbrainz, discogs) };
    (survivor, absorbed, releases)
}

/// A draft on `device` that names `artist_id` as its album artist — local
/// import-pane state no sync carries: a folder of one track under a watched
/// folder, scanned, and its album credited to the artist in the pane. The
/// folder lives as long as the directory returned.
async fn draft_crediting(device: &TestDevice, artist_id: &str) -> tempfile::TempDir {
    let store = device.store();
    let dir = tempfile::tempdir().unwrap();
    let watched = std::fs::canonicalize(dir.path()).unwrap();
    let folder = watched.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(
        bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"),
        folder.join("01 Test Track 1.flac"),
    )
    .unwrap();
    let tables = bae_import::tables::ImportTables::new(store.clone());
    tables.add_watched_import_folder(&watched).await.unwrap();
    let candidate = bae_import::folder_scanner::FolderCandidate {
        path: folder.clone(),
        file_root: folder.clone(),
        name: "Album".to_string(),
        files: bae_import::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            bae_import::ReleaseFileScope::Recursive,
            &bae_import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap(),
        watched_folder_path: watched.clone(),
        scope: bae_import::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: bae_import::RelativeFolder::of(std::path::Path::new("Album")).unwrap(),
        grouping: None,
    };
    let generation =
        tables.begin_folder_scan(&watched, bae_import::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(
            &watched,
            generation,
            &bae_import::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    tables.finish_folder_scan(&watched, generation, None).await.unwrap();
    let content_hash = candidate.files.content_hash();
    let metadata_revision = tables
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .map_or(0, |state| state.metadata_revision);
    let artist = device.records().find_artist_by_id(artist_id).await.unwrap().unwrap();
    bae_import::CandidatePreparations::new(store.clone())
        .set_album_artists_prepared(
            &watched,
            &candidate.key(),
            &bae_import::CandidateAsRead { content_hash, file_edit_revision: 0, metadata_revision },
            &[bae_import::Credited::plain(ArtistAssignment::picked(ExistingArtist::from(artist)))],
        )
        .await
        .unwrap();
    dir
}

/// Device A credits a MusicBrainz id two of its artists hold, merging them,
/// while device B has a draft crediting the one merged away. B's own draft
/// never stops it from taking A's library.
#[test]
fn a_draft_on_one_device_never_blocks_an_artist_merge_from_another() {
    run_two_device_test(async |devices| {
        let (survivor, absorbed, _) = one_id_on_two_artists(devices).await;
        let (album_id, release_id) = album_credited_to(devices.a(), &absorbed, "Credited").await;
        releases_on_both(devices, &[&release_id]).await;
        let _draft_folder = draft_crediting(devices.b(), &absorbed).await;

        let credited = credited_artist(devices.a(), catalog_artist(Some(MB_ARTIST), None)).await;
        assert_eq!(credited, survivor, "the write keeps the artist with the smaller id");

        devices.converge("SELECT id || ' ' || into_artist_id FROM artist_merges").await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                album_artist_ids(&bae_records::test_fixtures::test_records(db), &album_id).await,
                vec![survivor.clone()]
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// Device A merges two artists while device B, apart, credits the absorbed
/// one on a new album. Both writes land: B's album is credited to the one
/// merged artist on both devices.
#[test]
fn an_artist_merge_and_a_concurrent_credit_of_the_merged_artist_both_land() {
    run_two_device_test(async |devices| {
        let (survivor, absorbed, mut releases) = one_id_on_two_artists(devices).await;

        devices.pause().await;
        let credited = credited_artist(devices.a(), catalog_artist(Some(MB_ARTIST), None)).await;
        assert_eq!(credited, survivor, "the write keeps the artist with the smaller id");
        let (new_album, new_release) = album_credited_to(devices.b(), &absorbed, "New").await;
        devices.resume().await;

        releases.push(new_release);
        releases_on_both(devices, &releases.iter().map(String::as_str).collect::<Vec<_>>()).await;
        devices.converge("SELECT id || ' ' || into_artist_id FROM artist_merges").await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(
                album_artist_ids(&bae_records::test_fixtures::test_records(db), &new_album).await,
                vec![survivor.clone()]
            );
        }
        assert_nothing_held(devices).await;
    });
}

/// Device A deletes an album's only release while device B, apart, adds a
/// release to that album. B's release survives on both devices, in the album
/// it was added to.
#[test]
fn removing_an_albums_last_release_never_takes_a_concurrent_release_with_it() {
    run_two_device_test(async |devices| {
        let (_, album_id, release_id) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Album").await;
        releases_on_both(devices, &[&release_id]).await;

        devices.pause().await;
        devices.a().manager().delete_release(&release_id).await.unwrap();
        let added = remote_release_in(devices.b(), &album_id).await;
        devices.resume().await;

        releases_on_both(devices, &[&added]).await;
        for db in [devices.a().store(), devices.b().store()] {
            assert_eq!(listed_albums(db).await, vec![album_id.clone()]);
        }
        assert_nothing_held(devices).await;
    });
}
