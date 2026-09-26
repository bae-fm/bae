//! How library rows meet across devices: two devices of one library write
//! while apart, sync, and must land on one library with nothing held back —
//! no duplicate of a row both devices described, and no pull that can never
//! apply.

use super::test_devices::{run_two_device_test, TestDevice, TwoDevices};
use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, DbTrackWork, DbWork};
use crate::import::{
    ArtistAssignment, Catalog, ExistingArtist, MetadataRef, ReleaseRecord, ReleaseUserEdit,
};

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
    let artist_id = device
        .manager()
        .find_or_create_artists(&[artist])
        .await
        .expect("resolve the album's artist")
        .remove(0);
    // The album arrives with its release in one write, as an import writes
    // them.
    let album = DbAlbum::new_test(title, &artist_id);
    let release = DbRelease {
        remote: true,
        ..DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string())
    };
    device
        .database()
        .insert_album_with_release_and_tracks(&album, &release, &[], &[])
        .await
        .unwrap();
    (artist_id, album.id, release.id)
}

async fn remote_release_in(device: &TestDevice, album_id: &str) -> String {
    let release = DbRelease {
        remote: true,
        ..DbRelease::new_test(album_id, &uuid::Uuid::new_v4().to_string())
    };
    device.database().insert_release(&release).await.unwrap();
    release.id
}

async fn texts(database: &Database, query: &str) -> Vec<String> {
    database.query_texts_for_test(query).await.unwrap()
}

/// The artists the artist browse lists.
async fn listed_artists(database: &Database) -> Vec<String> {
    database
        .get_artist_page(&[], 0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|summary| summary.artist.id)
        .collect()
}

/// The albums the album browse lists.
async fn listed_albums(database: &Database) -> Vec<String> {
    database
        .get_album_page(&[], 0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|summary| summary.id)
        .collect()
}

async fn album_artist_ids(database: &Database, album_id: &str) -> Vec<String> {
    database
        .get_artists_for_album(album_id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.id)
        .collect()
}

fn assert_nothing_held(devices: &TwoDevices) {
    for (name, device) in [("A", devices.a()), ("B", devices.b())] {
        let held = device.held();
        assert!(held.is_empty(), "device {name} holds {held:?}");
    }
}

/// Both devices have pulled each other's writes: nothing is held, and each
/// sees every release in `release_ids`.
async fn releases_on_both(devices: &TwoDevices, release_ids: &[&str]) {
    let wanted = release_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    devices
        .settle("every release on both devices", move |_, database| {
            let wanted = wanted.clone();
            async move {
                let present = texts(&database, "SELECT id FROM releases").await;
                wanted.iter().all(|id| present.contains(id))
            }
        })
        .await;
}

/// The same MusicBrainz artist, credited on each device before either has
/// seen the other's library, is one artist once they sync.
#[test]
fn one_catalog_artist_credited_on_two_devices_is_one_artist() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        devices.pause();
        let (_, _, release_a) = remote_album_by(
            devices.a(),
            catalog_artist(Some(MB_ARTIST), None),
            "Album A",
        )
        .await;
        let (_, _, release_b) = remote_album_by(
            devices.b(),
            catalog_artist(Some(MB_ARTIST), None),
            "Album B",
        )
        .await;
        devices.resume().await;

        releases_on_both(&devices, &[&release_a, &release_b]).await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                listed_artists(database).await.len(),
                1,
                "one MusicBrainz artist is one library artist on every device"
            );
        }
        assert_nothing_held(&devices);
    });
}

/// The same MusicBrainz work, performed on a track on each device, is one work
/// once they sync.
#[test]
fn one_catalog_work_performed_on_two_devices_is_one_work() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        devices.pause();
        let mut releases = Vec::new();
        for device in [devices.a(), devices.b()] {
            let artist_id = device
                .manager()
                .find_or_create_artists(&[catalog_artist(Some(MB_ARTIST), None)])
                .await
                .unwrap()
                .remove(0);
            let album = DbAlbum::new_test("Album", &artist_id);
            let release = DbRelease {
                remote: true,
                ..DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string())
            };
            let track = DbTrack::new_test(
                &release.id,
                &uuid::Uuid::new_v4().to_string(),
                "Track",
                Some(1),
            );
            device
                .database()
                .insert_album_with_release_and_tracks(
                    &album,
                    &release,
                    std::slice::from_ref(&track),
                    &[],
                )
                .await
                .unwrap();
            let work = DbWork {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Work".to_string(),
                disambiguation: None,
                work_type: None,
                musicbrainz_work_id: MB_WORK.to_string(),
                created_at: chrono::Utc::now(),
            };
            let resolved = device
                .manager()
                .resolve_works_for_import(std::slice::from_ref(&work))
                .await
                .unwrap();
            let track_work = DbTrackWork::new(
                &track.id,
                &resolved.ids[0],
                0,
                Catalog::MusicBrainz,
                uuid::Uuid::new_v4().to_string(),
                chrono::Utc::now(),
            );
            device
                .database()
                .insert_composition_fixture_rows(&resolved.inserts, &[track_work], &[])
                .await
                .unwrap();
            releases.push(release.id);
        }
        devices.resume().await;

        releases_on_both(&devices, &[&releases[0], &releases[1]]).await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                texts(database, "SELECT id FROM works").await.len(),
                1,
                "one MusicBrainz work is one library work on every device"
            );
        }
        assert_nothing_held(&devices);
    });
}

/// Two pressings of one MusicBrainz release group, identified on different
/// devices, belong to one album once they sync — the album a single device
/// files both under.
#[test]
fn one_release_group_identified_on_two_devices_is_one_album() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        devices.pause();
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
                        true,
                    )],
                    false,
                )
                .await
                .unwrap();
            releases.push(release_id);
        }
        devices.resume().await;

        releases_on_both(&devices, &[&releases[0], &releases[1]]).await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                listed_albums(database).await.len(),
                1,
                "one release group is one album on every device"
            );
        }
        assert_nothing_held(&devices);
    });
}

/// The edit crediting `artist_ids`, in order, as a release's album artists.
async fn album_artist_edit(device: &TestDevice, artist_ids: &[&str]) -> ReleaseUserEdit {
    let mut assignments = Vec::new();
    for id in artist_ids {
        let artist = device
            .database()
            .find_artist_by_id(id)
            .await
            .unwrap()
            .expect("the credited artist is in the library");
        assignments.push(ArtistAssignment::Picked {
            artist: ExistingArtist::from(artist),
        });
    }
    ReleaseUserEdit {
        album_title: "Album".to_string(),
        album_artist_assignments: assignments,
        album_year: None,
        pressing: crate::pressing::Pressing::blank(),
        tracks: Vec::new(),
    }
}

/// Both devices credit the same second artist on one album while apart. The
/// credit is one fact, and neither device's edit is refused.
#[test]
fn the_same_album_credit_added_on_two_devices_merges() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        let (first, album_id, release_id) = remote_album_by(
            devices.a(),
            catalog_artist(None, Some(DISCOGS_ARTIST)),
            "Album",
        )
        .await;
        let (second, _, other_release) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Other").await;
        releases_on_both(&devices, &[&release_id, &other_release]).await;

        devices.pause();
        for device in [devices.a(), devices.b()] {
            let edit = album_artist_edit(device, &[&first, &second]).await;
            device
                .manager()
                .apply_release_metadata_user_edit(&release_id, &edit)
                .await
                .unwrap();
        }
        devices.resume().await;

        devices
            .converge("SELECT id || ' ' || album_id || ' ' || artist_id FROM album_artists")
            .await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                album_artist_ids(database, &album_id).await,
                vec![first.clone(), second.clone()]
            );
        }
        assert_nothing_held(&devices);
    });
}

/// Both devices identify one release against the same second catalog while
/// apart. The release has one record per catalog, and neither edit is refused.
#[test]
fn the_same_record_set_on_two_devices_merges() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        let (_, _, release_id) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Album").await;
        let musicbrainz = ReleaseRecord::new(
            &MetadataRef::new(Catalog::MusicBrainz, "7a3e1b2c-4d5e-4f60-8a9b-0c1d2e3f4a5b"),
            Some(MB_GROUP.to_string()),
            true,
        );
        devices
            .a()
            .manager()
            .set_records(&release_id, vec![musicbrainz.clone()], false)
            .await
            .unwrap();
        let records_of =
            format!("SELECT catalog FROM release_records WHERE release_id = '{release_id}'");
        let first = records_of.clone();
        devices
            .settle("the first record on both devices", move |_, database| {
                let first = first.clone();
                async move { texts(&database, &first).await.len() == 1 }
            })
            .await;

        devices.pause();
        let discogs = ReleaseRecord::new(
            &MetadataRef::new(Catalog::Discogs, "249504"),
            Some("20170".to_string()),
            false,
        );
        for device in [devices.a(), devices.b()] {
            device
                .manager()
                .set_records(
                    &release_id,
                    vec![musicbrainz.clone(), discogs.clone()],
                    false,
                )
                .await
                .unwrap();
        }
        devices.resume().await;

        devices
            .converge("SELECT id || ' ' || release_id || ' ' || catalog FROM release_records")
            .await;
        assert_eq!(texts(devices.a().database(), &records_of).await.len(), 2);
        assert_nothing_held(&devices);
    });
}

/// Record, on `device`, the import failure where one incoming artist matched
/// two library artists — the state the identity merge consolidates.
async fn record_identity_conflict(device: &TestDevice, discogs_artist: &str, mb_artist: &str) {
    device
        .database()
        .execute_local_sql_for_test(&format!(
            "INSERT INTO import_candidate_state (content_hash, folder_path) \
                 VALUES ('conflict', '/Album');
             INSERT INTO import_candidate_failure (content_hash, error, failed_at) \
                 VALUES ('conflict', 'artist identity', '2026-01-01T00:00:00Z');
             INSERT INTO import_candidate_artist_identity_conflict \
                 (content_hash, incoming_artist_name, discogs_artist_id, musicbrainz_artist_id, \
                  discogs_library_artist_id, musicbrainz_library_artist_id) \
                 VALUES ('conflict', 'Artist', '{DISCOGS_ARTIST}', '{MB_ARTIST}', \
                         '{discogs_artist}', '{mb_artist}');"
        ))
        .await
        .unwrap();
}

/// A draft on `device` that names `artist_id` as its album artist — local
/// import-pane state no sync carries.
async fn draft_crediting(device: &TestDevice, artist_id: &str) {
    device
        .database()
        .execute_local_sql_for_test(&format!(
            "INSERT INTO import_candidate_state (content_hash, folder_path) \
                 VALUES ('draft', '/Album');
             INSERT INTO import_candidate_edit \
                 (content_hash, album_title, album_year, year, label, \
                  catalog_number, barcode, author, draft_blank, draft_valid) \
                 VALUES ('draft', 'Album', '', '', '', '', '', 'person', 0, 1);
             INSERT INTO import_candidate_album_artist_assignment \
                 (content_hash, position, assignment_kind, artist_id) \
                 VALUES ('draft', 0, 'picked', '{artist_id}');"
        ))
        .await
        .unwrap();
}

/// Device A confirms that its Discogs artist and its MusicBrainz artist are
/// one artist while device B has a draft crediting the Discogs one. B's own
/// draft never stops it from taking A's library.
#[test]
fn a_draft_on_one_device_never_blocks_an_artist_merge_from_another() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        let (discogs, album_id, release_id) = remote_album_by(
            devices.a(),
            catalog_artist(None, Some(DISCOGS_ARTIST)),
            "Album",
        )
        .await;
        let (musicbrainz, _, other_release) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Other").await;
        releases_on_both(&devices, &[&release_id, &other_release]).await;
        draft_crediting(devices.b(), &discogs).await;

        record_identity_conflict(devices.a(), &discogs, &musicbrainz).await;
        devices
            .a()
            .manager()
            .merge_import_artist_identity_conflict("conflict", &musicbrainz)
            .await
            .unwrap();

        devices.converge("SELECT id FROM artists").await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                album_artist_ids(database, &album_id).await,
                vec![musicbrainz.clone()]
            );
        }
        assert_nothing_held(&devices);
    });
}

/// Device A merges two artists while device B, apart, credits the absorbed
/// one on a new album. Both writes land: B's album is credited to the one
/// merged artist on both devices.
#[test]
fn an_artist_merge_and_a_concurrent_credit_of_the_merged_artist_both_land() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        let (discogs, _, release_id) = remote_album_by(
            devices.a(),
            catalog_artist(None, Some(DISCOGS_ARTIST)),
            "Album",
        )
        .await;
        let (musicbrainz, _, other_release) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Other").await;
        releases_on_both(&devices, &[&release_id, &other_release]).await;

        devices.pause();
        record_identity_conflict(devices.a(), &discogs, &musicbrainz).await;
        devices
            .a()
            .manager()
            .merge_import_artist_identity_conflict("conflict", &musicbrainz)
            .await
            .unwrap();
        let (credited, new_album, new_release) = remote_album_by(
            devices.b(),
            catalog_artist(None, Some(DISCOGS_ARTIST)),
            "New",
        )
        .await;
        assert_eq!(credited, discogs, "B credits the artist it already has");
        devices.resume().await;

        releases_on_both(&devices, &[&release_id, &other_release, &new_release]).await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(
                album_artist_ids(database, &new_album).await,
                vec![musicbrainz.clone()]
            );
        }
        assert_nothing_held(&devices);
    });
}

/// Device A deletes an album's only release while device B, apart, adds a
/// release to that album. B's release survives on both devices, in the album
/// it was added to.
#[test]
fn removing_an_albums_last_release_never_takes_a_concurrent_release_with_it() {
    run_two_device_test(|| async {
        let devices = TwoDevices::pair().await;
        let (_, album_id, release_id) =
            remote_album_by(devices.a(), catalog_artist(Some(MB_ARTIST), None), "Album").await;
        releases_on_both(&devices, &[&release_id]).await;

        devices.pause();
        devices
            .a()
            .manager()
            .delete_release(&release_id)
            .await
            .unwrap();
        let added = remote_release_in(devices.b(), &album_id).await;
        devices.resume().await;

        releases_on_both(&devices, &[&added]).await;
        for database in [devices.a().database(), devices.b().database()] {
            assert_eq!(listed_albums(database).await, vec![album_id.clone()]);
        }
        assert_nothing_held(&devices);
    });
}
