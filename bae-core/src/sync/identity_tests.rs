//! How library rows meet across devices: two devices of one library write
//! while apart, sync, and must land on one library with nothing held back —
//! no duplicate of a row both devices described, and no pull that can never
//! apply.

use super::test_devices::{run_two_device_test, TestDevice, TwoDevices};
use crate::db::{Database, DbAlbum, DbArtist, DbRelease};
use crate::import::{
    ArtistAssignment, Catalog, ExistingArtist, MetadataRef, PressingEdit, ReleaseRecord,
    ReleaseUserEdit,
};

const MB_ARTIST: &str = "5b11f4ce-a62d-471e-81fc-a69a8278c7da";
const DISCOGS_ARTIST: &str = "3840";
const MB_GROUP: &str = "0c9f2c1e-5b8a-4c8e-9f3a-1b2c3d4e5f60";

fn catalog_artist(
    musicbrainz_artist_id: Option<&str>,
    discogs_artist_id: Option<&str>,
) -> DbArtist {
    DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Artist".to_string(),
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

async fn texts(database: &Database, query: &str) -> Vec<String> {
    database.query_texts_for_test(query).await.unwrap()
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
        assignments.push(ArtistAssignment::Existing {
            artist: ExistingArtist::from(artist),
        });
    }
    ReleaseUserEdit {
        album_title: "Album".to_string(),
        album_artist_assignments: assignments,
        album_year: None,
        pressing: PressingEdit::blank(),
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
                 (content_hash, album_title, album_year, year, format, label, \
                  catalog_number, country, barcode, author) \
                 VALUES ('draft', 'Album', '', '', '', '', '', '', '', 'person');
             INSERT INTO import_candidate_album_artist_assignment \
                 (content_hash, position, assignment_kind, artist_id) \
                 VALUES ('draft', 0, 'existing', '{artist_id}');"
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
